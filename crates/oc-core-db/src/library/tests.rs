use std::collections::HashSet;

use rusqlite::Connection;
use tempfile::{tempdir, TempDir};

use super::{read_page, sort_key, Format, LibraryQuery, LibraryReader, Sort, View, MAX_WINDOW};
use crate::synthetic::{
    artist_name, release_title, seed, track_bpm, track_format, track_title, MAX_TRACKS,
};
use crate::{check_integrity, Database, Error};

const TRACKS: u32 = 1_000;

fn seeded(tracks: u32) -> (TempDir, Database) {
    let directory = tempdir().unwrap();
    let mut database = Database::open(directory.path()).unwrap();
    seed(&mut database, tracks).unwrap();
    (directory, database)
}

struct Expected {
    id: i64,
    index: u32,
    title: String,
    artist: String,
    release: String,
    release_id: i64,
}

fn expected(tracks: u32) -> Vec<Expected> {
    (0..tracks)
        .map(|index| {
            let release_index = index / 10;
            let artist_index = release_index % 240;
            Expected {
                id: i64::from(index) + 1,
                index,
                title: track_title(index),
                artist: artist_name(artist_index),
                release: release_title(release_index),
                release_id: i64::from(release_index) + 1,
            }
        })
        .collect()
}

fn ids(items: &[Expected]) -> Vec<i64> {
    items.iter().map(|row| row.id).collect()
}

/// Every window of a query, concatenated.
fn all_items(connection: &Connection, query: &LibraryQuery) -> Vec<super::LibraryItem> {
    let first = read_page(connection, query, 0, MAX_WINDOW).unwrap();
    let mut items = first.items;
    let total = usize::try_from(first.total).unwrap();
    while items.len() < total {
        let page = read_page(connection, query, items.len(), MAX_WINDOW).unwrap();
        assert_eq!(
            usize::try_from(page.total).unwrap(),
            total,
            "total is stable"
        );
        assert_ne!(page.items.len(), 0);
        items.extend(page.items);
    }
    items
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn matches_terms(row: &Expected, terms: &[&str]) -> bool {
    let text = format!("{} {} {}", row.title, row.artist, row.release);
    let found = words(&text);
    terms.iter().all(|term| {
        found
            .iter()
            .any(|word| word.starts_with(&term.to_lowercase()))
    })
}

#[test]
fn seeding_builds_the_expected_shape_and_a_clean_database() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let total = |view| {
        read_page(&reader, &LibraryQuery::new(view), 0, 1)
            .unwrap()
            .total
    };
    assert_eq!(total(View::Tracks), 1_000);
    assert_eq!(total(View::Releases), 100);
    assert_eq!(total(View::Artists), 100);
    check_integrity(&database.connection).unwrap();
    let (empty_dir, empty) = seeded(0);
    let reader = empty.reader().unwrap();
    assert_eq!(total_of(&reader, View::Tracks), 0);
    drop(empty_dir);
}

fn total_of(connection: &Connection, view: View) -> u64 {
    read_page(connection, &LibraryQuery::new(view), 0, 1)
        .unwrap()
        .total
}

#[test]
fn seeding_refuses_an_oversized_library() {
    let directory = tempdir().unwrap();
    let mut database = Database::open(directory.path()).unwrap();
    assert!(matches!(
        seed(&mut database, MAX_TRACKS + 1),
        Err(Error::InvalidSyntheticSize(_))
    ));
}

#[test]
fn recent_lists_the_newest_track_first_and_reports_the_window() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let page = read_page(&reader, &LibraryQuery::new(View::Tracks), 0, 5).unwrap();
    assert_eq!(page.total, 1_000);
    assert_eq!(page.start, 0);
    let got: Vec<i64> = page.items.iter().map(|item| item.id).collect();
    assert_eq!(got, [1_000, 999, 998, 997, 996]);
    let first = &page.items[0];
    assert_eq!(first.title, track_title(999));
    assert_eq!(first.artist, artist_name(99));
    assert_eq!(first.release, release_title(99));
    assert_eq!(first.format.as_deref(), Some(track_format(999)));
    assert_eq!(first.kind, View::Tracks);
}

#[test]
fn every_sort_matches_an_independent_ordering_and_windows_tile_it() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let rows = expected(TRACKS);
    let orderings: [(Sort, Vec<i64>); 4] = [
        (Sort::Recent, ids(&rows).into_iter().rev().collect()),
        (Sort::Title, {
            let mut sorted: Vec<&Expected> = rows.iter().collect();
            sorted.sort_by_key(|row| (sort_key(&row.title), row.id));
            sorted.iter().map(|row| row.id).collect()
        }),
        (Sort::Artist, {
            let mut sorted: Vec<&Expected> = rows.iter().collect();
            sorted.sort_by_key(|row| {
                (
                    sort_key(&row.artist),
                    row.release_id,
                    row.index % 10,
                    row.id,
                )
            });
            sorted.iter().map(|row| row.id).collect()
        }),
        (Sort::Bpm, {
            let mut sorted: Vec<&Expected> = rows.iter().collect();
            sorted.sort_by_key(|row| {
                let no_tempo = row.index % 17 == 0;
                (
                    no_tempo,
                    if no_tempo { 0 } else { 110 + row.index % 31 },
                    row.id,
                )
            });
            sorted.iter().map(|row| row.id).collect()
        }),
    ];
    for (sort, want) in orderings {
        let mut query = LibraryQuery::new(View::Tracks);
        query.sort = sort;
        let got: Vec<i64> = all_items(&reader, &query).iter().map(|i| i.id).collect();
        assert!(got == want, "{sort:?} order differs from the model");
        let unique: HashSet<_> = got.iter().collect();
        assert_eq!(unique.len(), 1_000, "{sort:?} lists each track once");
    }
}

#[test]
fn tracks_without_a_tempo_sort_last() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let mut query = LibraryQuery::new(View::Tracks);
    query.sort = Sort::Bpm;
    let items = all_items(&reader, &query);
    let missing = items.iter().filter(|item| item.bpm.is_none()).count();
    assert_eq!(missing, 59);
    assert!(items[items.len() - missing..]
        .iter()
        .all(|item| item.bpm.is_none()));
    assert_eq!(
        items[0].bpm,
        track_bpm(u32::try_from(items[0].id - 1).unwrap())
    );
}

#[test]
fn formats_filter_by_the_active_file() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    for (format, name, want) in [
        (Format::Aiff, "AIFF", 334),
        (Format::Mp3, "MP3", 333),
        (Format::Flac, "FLAC", 333),
    ] {
        let mut query = LibraryQuery::new(View::Tracks);
        query.format = Some(format);
        let items = all_items(&reader, &query);
        assert_eq!(items.len(), want, "{name}");
        assert!(items
            .iter()
            .all(|item| item.format.as_deref() == Some(name)));
    }
}

#[test]
fn search_matches_word_prefixes_in_any_column_and_requires_every_term() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let rows = expected(TRACKS);
    for terms in [
        vec!["night"],
        vec!["NIGHT"],
        vec!["ni"],
        vec!["night", "studies"],
        vec!["drive", "ensemble"],
        vec!["000100"],
        vec!["colour", "theory", "quiet"],
    ] {
        let mut query = LibraryQuery::new(View::Tracks);
        query.search = terms.join("  ");
        query.sort = Sort::Title;
        let got: HashSet<i64> = all_items(&reader, &query).iter().map(|i| i.id).collect();
        let want: HashSet<i64> = rows
            .iter()
            .filter(|row| matches_terms(row, &terms))
            .map(|row| row.id)
            .collect();
        assert_eq!(got, want, "{terms:?}");
    }
    let mut query = LibraryQuery::new(View::Tracks);
    query.search = "night".into();
    assert!(read_page(&reader, &query, 0, 1).unwrap().total > 0);
}

#[test]
fn blank_search_is_no_filter_and_unsearchable_text_matches_nothing() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let mut query = LibraryQuery::new(View::Tracks);
    query.search = "   ".into();
    assert_eq!(read_page(&reader, &query, 0, 1).unwrap().total, 1_000);
    for text in ["!!!", "-", "\"\"", "***"] {
        query.search = text.into();
        let page = read_page(&reader, &query, 0, 10).unwrap();
        assert_eq!((page.total, page.items.len()), (0, 0), "{text}");
    }
}

#[test]
fn search_syntax_in_user_text_is_treated_as_words() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let mut query = LibraryQuery::new(View::Tracks);
    for text in [
        "night\" OR \"x",
        "NEAR(night drive)",
        "title:night",
        "night*",
        "(night",
        "night' OR 1=1 --",
        "-night",
    ] {
        query.search = text.into();
        read_page(&reader, &query, 0, 10).unwrap_or_else(|e| panic!("{text:?}: {e}"));
    }
    query.search = "title:night".into();
    assert_eq!(
        read_page(&reader, &query, 0, 1).unwrap().total,
        0,
        "a column filter is two words, not syntax"
    );
}

#[test]
fn artist_and_release_filters_narrow_tracks_and_releases() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let mut query = LibraryQuery::new(View::Tracks);
    query.release_id = Some(5);
    let items = all_items(&reader, &query);
    assert_eq!(
        items.iter().map(|i| i.id).collect::<Vec<_>>(),
        (41..=50).rev().collect::<Vec<_>>()
    );
    assert!(items.iter().all(|item| item.release_id == 5));
    query.release_id = None;
    query.artist_id = Some(3);
    assert_eq!(all_items(&reader, &query).len(), 10);
    query.view = View::Releases;
    let releases = all_items(&reader, &query);
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, 3);
    assert_eq!(releases[0].track_count, 10);
}

#[test]
fn release_and_artist_views_report_counts_and_obey_sort_search_and_format() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    for view in [View::Releases, View::Artists] {
        let items = all_items(&reader, &LibraryQuery::new(view));
        assert_eq!(items.len(), 100);
        assert!(items
            .iter()
            .all(|item| item.track_count == 10 && item.kind == view));
    }
    let mut query = LibraryQuery::new(View::Releases);
    query.sort = Sort::Title;
    let titles: Vec<String> = all_items(&reader, &query)
        .into_iter()
        .map(|i| i.title)
        .collect();
    let mut want: Vec<String> = (0..100).map(release_title).collect();
    want.sort_by_key(|title| sort_key(title));
    assert_eq!(titles, want);
    query.sort = Sort::Recent;
    assert_eq!(all_items(&reader, &query)[0].id, 100);
    query.sort = Sort::Bpm;
    assert_eq!(
        all_items(&reader, &query)
            .iter()
            .map(|i| i.id)
            .collect::<HashSet<_>>()
            .len(),
        100,
        "bpm falls back to a total order"
    );

    let mut artists = LibraryQuery::new(View::Artists);
    artists.search = "ensemble".into();
    let found = all_items(&reader, &artists);
    assert_eq!(found.len(), 25);
    assert!(found.iter().all(|item| item.title.starts_with("Ensemble")));
    // A track's own title finds its artist and its release too.
    let rows = expected(TRACKS);
    let track = rows
        .iter()
        .find(|row| row.title.ends_with("000100"))
        .unwrap();
    artists.search = "000100".into();
    let found = all_items(&reader, &artists);
    assert_eq!(
        found.iter().map(|item| &item.title).collect::<Vec<_>>(),
        [&track.artist]
    );
    let mut releases = LibraryQuery::new(View::Releases);
    releases.search = "000100".into();
    let found = all_items(&reader, &releases);
    assert_eq!(
        found.iter().map(|item| &item.title).collect::<Vec<_>>(),
        [&track.release]
    );
    artists.search = String::new();
    artists.format = Some(Format::Flac);
    assert_eq!(all_items(&reader, &artists).len(), 100);
    artists.artist_id = Some(7);
    assert_eq!(all_items(&reader, &artists).len(), 1);
}

#[test]
fn a_release_reads_in_track_order_not_newest_first() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let mut query = LibraryQuery::new(View::Tracks);
    query.release_id = Some(3);
    query.sort = Sort::Track;
    let ids: Vec<i64> = all_items(&reader, &query)
        .iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(
        ids,
        (21..=30).collect::<Vec<i64>>(),
        "tracks 1 to 10 of the release"
    );
    query.sort = Sort::Recent;
    let newest: Vec<i64> = all_items(&reader, &query)
        .iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(newest, (21..=30).rev().collect::<Vec<i64>>());
    // An artist's tracks go release by release.
    let mut artist = LibraryQuery::new(View::Tracks);
    artist.artist_id = Some(1);
    artist.sort = Sort::Track;
    let releases: Vec<i64> = all_items(&reader, &artist)
        .iter()
        .map(|item| item.release_id)
        .collect();
    let mut grouped = releases.clone();
    grouped.dedup();
    assert_eq!(
        grouped.len(),
        releases.len() / 10,
        "each release's tracks together"
    );
}

#[test]
fn windows_are_validated_and_overshooting_returns_the_total() {
    let (_directory, database) = seeded(TRACKS);
    let reader = database.reader().unwrap();
    let query = LibraryQuery::new(View::Tracks);
    assert!(matches!(
        read_page(&reader, &query, 0, 0),
        Err(Error::InvalidWindow)
    ));
    assert!(matches!(
        read_page(&reader, &query, 0, MAX_WINDOW + 1),
        Err(Error::InvalidWindow)
    ));
    let past = read_page(&reader, &query, 5_000, 50).unwrap();
    assert_eq!(
        (past.items.len(), past.total, past.start),
        (0, 1_000, 5_000)
    );
    let tail = read_page(&reader, &query, 990, 50).unwrap();
    assert_eq!(tail.items.len(), 10);
}

#[test]
fn the_same_seed_gives_identical_pages() {
    let (_a, first) = seeded(300);
    let (_b, second) = seeded(300);
    let (first, second) = (first.reader().unwrap(), second.reader().unwrap());
    for view in [View::Tracks, View::Releases, View::Artists] {
        for sort in [Sort::Recent, Sort::Title, Sort::Artist, Sort::Bpm] {
            let mut query = LibraryQuery::new(view);
            query.sort = sort;
            assert_eq!(
                read_page(&first, &query, 10, 50).unwrap(),
                read_page(&second, &query, 10, 50).unwrap(),
                "{view:?} {sort:?}"
            );
        }
    }
}

#[test]
fn the_library_tables_enforce_the_path_and_presence_rules() {
    let (_directory, database) = seeded(20);
    let connection = &database.connection;
    let duplicate = connection.execute(
        "INSERT INTO file (track_id, root_id, path_raw, path_key, lifecycle, presence, size, \
         mtime_ms, container, last_seen_ms, created_ms) \
         SELECT track_id, root_id, path_raw, path_key, lifecycle, presence, size, mtime_ms, \
         container, last_seen_ms, created_ms FROM file WHERE id = 1",
        [],
    );
    assert!(duplicate.is_err(), "one key per root");
    let missing_without_time =
        connection.execute("UPDATE file SET presence = 'missing' WHERE id = 1", []);
    assert!(missing_without_time.is_err(), "missing needs a time");
    let present_with_time =
        connection.execute("UPDATE file SET missing_since_ms = 5 WHERE id = 1", []);
    assert!(present_with_time.is_err(), "present has no missing time");
    assert!(connection
        .execute("UPDATE file SET lifecycle = 'offline' WHERE id = 1", [])
        .is_err());
    assert!(connection
        .execute(
            "UPDATE library_root SET availability = 'missing' WHERE id = 1",
            []
        )
        .is_err());
    assert!(connection
        .execute("INSERT INTO track (release_id, artist_id, title, title_key, artist_key, added_ms) VALUES (999, 1, 'x', 'x', 'x', 0)", [])
        .is_err(), "foreign keys are enforced");
}

#[test]
fn the_cached_reader_agrees_with_the_direct_query() {
    let (_directory, database) = seeded(TRACKS);
    let mut cached = LibraryReader::new(database.reader().unwrap()).unwrap();
    let direct = database.reader().unwrap();
    let mut queries = Vec::new();
    for view in [View::Tracks, View::Releases, View::Artists] {
        for sort in [Sort::Recent, Sort::Title, Sort::Artist, Sort::Bpm] {
            let mut query = LibraryQuery::new(view);
            query.sort = sort;
            queries.push(query.clone());
            query.format = Some(Format::Mp3);
            queries.push(query.clone());
            query.search = "night".into();
            queries.push(query.clone());
            query.format = None;
            query.artist_id = Some(4);
            queries.push(query);
        }
    }
    // More queries than the cache holds, so eviction and reload are exercised.
    for query in &queries {
        for (start, count) in [(0, 10), (95, 7), (990, 50), (5_000, 10), (0, 200)] {
            assert!(
                cached.read_page(query, start, count).unwrap()
                    == read_page(&direct, query, start, count).unwrap(),
                "{query:?} at {start}+{count}"
            );
        }
    }
    let mut nothing = LibraryQuery::new(View::Tracks);
    nothing.search = "!!!".into();
    assert_eq!(cached.read_page(&nothing, 0, 10).unwrap().total, 0);
    assert!(matches!(
        cached.read_page(&nothing, 0, MAX_WINDOW + 1),
        Err(Error::InvalidWindow)
    ));
}

#[test]
fn a_commit_by_the_writer_reaches_the_cached_reader_on_its_next_call() {
    let (_directory, database) = seeded(100);
    let mut reader = LibraryReader::new(database.reader().unwrap()).unwrap();
    let query = LibraryQuery::new(View::Tracks);
    assert_eq!(reader.read_page(&query, 0, 1).unwrap().total, 100);
    assert_eq!(
        reader.read_page(&query, 0, 1).unwrap().total,
        100,
        "now cached"
    );
    database
        .connection
        .execute(
            "INSERT INTO track (release_id, artist_id, title, title_key, artist_key, added_ms) \
             VALUES (1, 1, 'Late Arrival', 'late arrival', 'x', 9999999999999)",
            [],
        )
        .unwrap();
    let page = reader.read_page(&query, 0, 1).unwrap();
    assert_eq!(page.total, 101);
    assert_eq!(page.items[0].title, "Late Arrival");
}

fn time_it(label: &str, mut run: impl FnMut() -> u64) {
    use std::time::Instant;
    for _ in 0..3 {
        run();
    }
    let mut total = 0;
    let mut millis: Vec<f64> = (0..30)
        .map(|_| {
            let began = Instant::now();
            total = run();
            began.elapsed().as_secs_f64() * 1_000.0
        })
        .collect();
    millis.sort_by(f64::total_cmp);
    eprintln!(
        "{label:<48} total {total:>7}  p50 {:>7.2} ms  p95 {:>7.2} ms  max {:>7.2} ms",
        millis[millis.len() / 2],
        millis[millis.len() * 95 / 100],
        millis[millis.len() - 1]
    );
}

/// `cargo test -p oc-core-db -- --ignored --nocapture benchmark_100k`
#[test]
#[ignore = "benchmark: run explicitly with --ignored --nocapture"]
fn benchmark_100k() {
    use std::time::Instant;
    let directory = tempdir().unwrap();
    let mut database = Database::open(directory.path()).unwrap();
    let began = Instant::now();
    seed(&mut database, MAX_TRACKS).unwrap();
    eprintln!(
        "seed {MAX_TRACKS} tracks: {:.2} s",
        began.elapsed().as_secs_f64()
    );
    let size = std::fs::metadata(directory.path().join(crate::DATABASE_NAME))
        .unwrap()
        .len();
    eprintln!(
        "database file {} MiB (WAL not checkpointed)",
        size / 1_048_576
    );
    let mut reader = LibraryReader::new(database.reader().unwrap()).unwrap();
    let window = 100;
    let mut scenarios: Vec<(String, LibraryQuery, usize)> = Vec::new();
    let mut tracks = LibraryQuery::new(View::Tracks);
    for sort in [Sort::Recent, Sort::Title, Sort::Artist, Sort::Bpm] {
        tracks.sort = sort;
        scenarios.push((format!("tracks {sort:?}"), tracks.clone(), 99_900));
    }
    tracks.sort = Sort::Title;
    tracks.format = Some(Format::Aiff);
    scenarios.push(("tracks Title, AIFF".into(), tracks.clone(), 33_200));
    tracks.format = None;
    for text in ["night", "ni", "night drive", "000500", "studio 001"] {
        tracks.search = text.into();
        tracks.sort = Sort::Recent;
        scenarios.push((format!("tracks search {text:?}"), tracks.clone(), 0));
    }
    tracks.search = "night".into();
    tracks.sort = Sort::Title;
    tracks.format = Some(Format::Flac);
    scenarios.push(("tracks search night, FLAC, Title".into(), tracks, 0));
    let mut releases = LibraryQuery::new(View::Releases);
    releases.sort = Sort::Title;
    scenarios.push(("releases Title".into(), releases.clone(), 9_900));
    releases.search = "nightfall".into();
    scenarios.push(("releases search nightfall".into(), releases, 0));
    let mut artists = LibraryQuery::new(View::Artists);
    artists.sort = Sort::Title;
    scenarios.push(("artists Title".into(), artists.clone(), 0));
    artists.search = "ensemble".into();
    scenarios.push(("artists search ensemble".into(), artists, 0));

    eprintln!("-- direct query: first window / deep window (count + offset each time)");
    for (label, query, deep) in &scenarios {
        time_it(&format!("{label} first"), || {
            read_page(reader.connection(), query, 0, window)
                .unwrap()
                .total
        });
        if *deep > 0 {
            time_it(&format!("{label} deep ({deep})"), || {
                read_page(reader.connection(), query, *deep, window)
                    .unwrap()
                    .total
            });
        }
    }
    eprintln!("-- cached reader: cold = first window of a new query, warm = any window after");
    for (label, query, deep) in &scenarios {
        time_it(&format!("{label} cold"), || {
            reader.clear_cache();
            reader.read_page(query, 0, window).unwrap().total
        });
        time_it(&format!("{label} warm (start {deep})"), || {
            reader.read_page(query, *deep, window).unwrap().total
        });
    }
}

/// SQLCipher's SQLite (3.51.3) returned the wrong rows for a correlated `EXISTS` filter with an
/// `OFFSET`; every page must be the matching slice of the whole ordered list.
#[test]
fn every_page_is_a_slice_of_the_whole_list() {
    let (_directory, database) = seeded(TRACKS);
    let connection = database.reader().unwrap();
    for view in [View::Tracks, View::Releases, View::Artists] {
        for format in [None, Some(Format::Mp3)] {
            let mut query = LibraryQuery::new(view);
            query.sort = Sort::Recent;
            query.format = format;
            // The first window is read at offset 0, which every SQLite version gets right.
            let whole: Vec<i64> = read_page(&connection, &query, 0, MAX_WINDOW)
                .unwrap()
                .items
                .iter()
                .map(|item| item.id)
                .collect();
            assert!(whole.len() > 20, "{query:?}");
            for start in [1, 13, whole.len() - 7] {
                let page = read_page(&connection, &query, start, 7).unwrap();
                let ids: Vec<i64> = page.items.iter().map(|item| item.id).collect();
                let end = start + 7;
                assert_eq!(ids, whole[start..end], "{query:?} at {start}");
            }
        }
    }
}
