//! The rekordbox mirror against synthetic exports. No real export is read; the paths and
//! names are made up, and the library files are rows in a temporary database.

use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use oc_paths::{Availability, Flavor, LocationPath, NameProfile, RelPath};
use oc_rekordbox::xml::{self, Collection};
use tempfile::{tempdir, TempDir};

use crate::mirror::{mirror_sources, read_export, MirrorSource};
use crate::roots::{NewRoot, ProfileSource, RootKind};
use crate::scan::{ObservedFile, ScanPass};
use crate::{Database, Error};

const EXPORT_PATH: &str = "/exports/collection.xml";

struct Library {
    _directory: TempDir,
    database: Database,
    root: i64,
}

fn rel(parts: &[&str]) -> RelPath {
    RelPath::from_components(parts.iter().map(|part| part.as_bytes()), Flavor::Posix).unwrap()
}

fn scan(database: &mut Database, root: i64, files: &[&[&str]], now: i64) {
    let mut dirs = vec![
        Vec::new(),
        vec![b"Artist".to_vec()],
        vec![b"Artist".to_vec(), b"Rel".to_vec()],
    ];
    dirs.dedup();
    database
        .apply_scan(
            root,
            &ScanPass {
                root_availability: Availability::Available,
                files: files
                    .iter()
                    .map(|parts| ObservedFile {
                        path: rel(parts),
                        size: 1,
                        mtime_ms: 1,
                        container: "AIFF".into(),
                        file_token: None,
                    })
                    .collect(),
                existing_dirs: dirs.clone(),
                enumerated_dirs: dirs,
                confirm_mass_missing: false,
            },
            now,
        )
        .unwrap();
}

/// A library of three indexed files, reachable from an export made on a Windows drive, with
/// catalog tracks linked to the first three.
fn library() -> Library {
    let directory = tempdir().unwrap();
    let mut database = Database::open(directory.path()).unwrap();
    let root = database
        .add_root(
            &NewRoot {
                path: Path::new("/Volumes/Music"),
                label: "Music",
                kind: RootKind::Local,
                profile: NameProfile::FoldCase,
                profile_source: ProfileSource::UserSet,
                dir_token: None,
                volume_token: None,
            },
            1,
        )
        .unwrap();
    database
        .add_alias(root, &LocationPath::parse(b"/G:/Library").unwrap(), 1)
        .unwrap();
    scan(
        &mut database,
        root,
        &[
            &["Artist", "Rel", "One.aif"],
            &["Artist", "Rel", "Two.aif"],
            &["Artist", "Rel", "Three.aif"],
        ],
        10,
    );
    database
        .connection
        .execute_batch(
            "INSERT INTO artist (id, name, sort_key) VALUES (1, 'Artist', 'artist');
             INSERT INTO release (id, artist_id, title, sort_key, artist_key, added_ms)
               VALUES (1, 1, 'Rel', 'rel', 'artist', 0);
             INSERT INTO track (id, release_id, artist_id, title, title_key, artist_key, added_ms)
               VALUES (1, 1, 1, 'One', 'one', 'artist', 0), (2, 1, 1, 'Two', 'two', 'artist', 0),
                      (3, 1, 1, 'Three', 'three', 'artist', 0), (4, 1, 1, 'Unlinked', 'unlinked', 'artist', 0);"
        )
        .unwrap();
    for (track, name) in [(1, "One.aif"), (2, "Two.aif"), (3, "Three.aif")] {
        database
            .connection
            .execute(
                "UPDATE file SET track_id = ?1 WHERE path_raw = ?2",
                rusqlite::params![track, rel(&["Artist", "Rel", name]).as_stored()],
            )
            .unwrap();
    }
    Library {
        _directory: directory,
        database,
        root,
    }
}

/// An export with five entries. Entry 3 spells its file with different case, entry 4 is on a
/// drive folder outside the library, entry 5 points at a file nobody indexed.
fn export(first_bpm: &str, with_first: bool) -> Collection {
    let first = if with_first {
        format!(
            "<TRACK TrackID=\"1\" Name=\"One\" Artist=\"A &amp; B\" AverageBpm=\"{first_bpm}\" \
             Tonality=\"8A\" Rating=\"255\" \
             Location=\"file://localhost/G:/Library/Artist/Rel/One.aif\">\
             <TEMPO Inizio=\"0.0255\" Bpm=\"{first_bpm}\" Metro=\"4/4\" Battito=\"1\"/>\
             <TEMPO Inizio=\"30.5\" Bpm=\"129.5\" Metro=\"4/4\" Battito=\"1\"/>\
             <POSITION_MARK Name=\"Drop\" Type=\"0\" Start=\"12.345678\" Num=\"1\"/>\
             <POSITION_MARK Name=\"Loop\" Type=\"4\" Start=\"60.25\" End=\"64.5\" Num=\"0\" Red=\"255\"/>\
             </TRACK>"
        )
    } else {
        String::new()
    };
    let count = if with_first { 5 } else { 4 };
    let ids = if with_first {
        "<TRACK Key=\"3\"/><TRACK Key=\"1\"/><TRACK Key=\"3\"/><TRACK Key=\"99\"/>"
    } else {
        "<TRACK Key=\"3\"/><TRACK Key=\"3\"/>"
    };
    let entries = if with_first { 4 } else { 2 };
    let text = format!(
        "<DJ_PLAYLISTS Version=\"1.0.0\"><PRODUCT Name=\"Synthetic\" Version=\"1\"/>\
         <COLLECTION Entries=\"{count}\">{first}\
         <TRACK TrackID=\"2\" Name=\"Two\" AverageBpm=\"0.00\" Tonality=\"\" \
                Location=\"file://localhost/G:/Library/Artist/Rel/Two.aif\"/>\
         <TRACK TrackID=\"3\" Name=\"Three\" AverageBpm=\"140.00\" Tonality=\"11B\" \
                Location=\"file://localhost/G:/Library/artist/REL/three.AIF\"/>\
         <TRACK TrackID=\"4\" Name=\"Bin\" Location=\"file://localhost/G:/$RECYCLE.BIN/x.aif\"/>\
         <TRACK TrackID=\"5\" Name=\"Gone\" AverageBpm=\"100.00\" \
                Location=\"file://localhost/G:/Library/Artist/Rel/Gone.aif\"/>\
         </COLLECTION>\
         <PLAYLISTS><NODE Type=\"0\" Name=\"ROOT\" Count=\"2\">\
         <NODE Type=\"1\" Name=\"Set\" KeyType=\"0\" Entries=\"{entries}\">{ids}</NODE>\
         <NODE Type=\"1\" Name=\"Empty\" KeyType=\"0\" Entries=\"0\"/>\
         </NODE></PLAYLISTS></DJ_PLAYLISTS>"
    );
    xml::read(Cursor::new(text.as_bytes())).unwrap()
}

fn source(mtime: Option<i64>) -> MirrorSource<'static> {
    MirrorSource {
        path: Path::new(EXPORT_PATH),
        file_mtime_ms: mtime,
    }
}

fn scalar(database: &Database, sql: &str) -> i64 {
    database
        .connection
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

fn text(database: &Database, sql: &str) -> String {
    database
        .connection
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

#[test]
fn a_first_read_stores_entries_matches_cues_tempos_and_playlists() {
    let mut library = library();
    let outcome = library
        .database
        .replace_mirror(&source(Some(77)), &export("128.00", true), 100)
        .unwrap();
    assert_eq!(
        (
            outcome.entries,
            outcome.matched,
            outcome.unmapped,
            outcome.not_indexed
        ),
        (5, 3, 1, 1)
    );
    assert_eq!((outcome.generation, outcome.unchanged), (1, false));
    let db = &library.database;
    let status = |key: i64| {
        text(
            db,
            &format!("SELECT match_status || '/' || COALESCE(match_kind, '-') FROM rb_track WHERE rb_key = {key}"),
        )
    };
    assert_eq!(status(1), "matched/exact_bytes");
    assert_eq!(status(2), "matched/exact_bytes");
    assert_eq!(status(3), "matched/key_equal");
    assert_eq!(status(4), "unmapped/-");
    assert_eq!(status(5), "not_indexed/-");
    assert_eq!(
        scalar(db, "SELECT COUNT(*) FROM rb_track WHERE match_status = 'not_indexed' AND file_id IS NULL AND root_id IS NOT NULL"),
        1,
        "a not-indexed entry still says which root it belongs to"
    );

    // Cues and tempo points keep rekordbox's fractional seconds exactly.
    let cue = db
        .connection
        .query_row(
            "SELECT c.kind, c.number, c.start_seconds, c.end_seconds, c.name FROM rb_cue c \
             JOIN rb_track t ON t.id = c.rb_track_id WHERE t.rb_key = 1 ORDER BY c.position",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(cue, (0, Some(1), 12.345_678, None, "Drop".to_owned()));
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM rb_cue"), 2);
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM rb_tempo"), 2);
    assert_eq!(
        text(db, "SELECT group_concat(start_seconds || ':' || bpm || ':' || meter || ':' || beat, ' ') FROM rb_tempo"),
        "0.0255:128.0:4/4:1 30.5:129.5:4/4:1"
    );
    assert_eq!(
        text(db, "SELECT artist FROM rb_track WHERE rb_key = 1"),
        "A & B"
    );

    // Playlists: order and duplicates kept, an ID missing from the export stays unresolved.
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM playlist"), 3);
    assert_eq!(
        text(db, "SELECT group_concat(rb_key, ',') FROM (SELECT rb_key FROM playlist_item ORDER BY position)"),
        "3,1,3,99"
    );
    assert_eq!(
        scalar(db, "SELECT COUNT(*) FROM playlist_item i LEFT JOIN rb_track t ON t.rb_key = i.rb_key WHERE t.id IS NULL"),
        1
    );
    assert_eq!(
        text(db, "SELECT c.name || ' in ' || p.name FROM playlist c JOIN playlist p ON p.id = c.parent_id WHERE c.kind = 'playlist' AND c.name = 'Set'"),
        "Set in ROOT"
    );
    let sources = mirror_sources(&db.connection).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(
        (
            sources[0].generation,
            sources[0].entries,
            sources[0].file_mtime_ms,
            sources[0].last_read_ms
        ),
        (1, 5, Some(77), Some(100))
    );
    assert_eq!(sources[0].path_raw, EXPORT_PATH.as_bytes());
}

#[test]
fn tempo_and_key_reach_linked_tracks_with_their_source_and_generation() {
    let mut library = library();
    let outcome = library
        .database
        .replace_mirror(&source(None), &export("128.00", true), 100)
        .unwrap();
    assert_eq!(outcome.derived_set, 3);
    let db = &library.database;
    let derived = |id: i64| {
        db.connection
            .query_row(
                "SELECT bpm, musical_key, derived_source_id, derived_generation FROM track WHERE id = ?1",
                [id],
                |row| Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<i64>>(2)?, row.get::<_, Option<i64>>(3)?)),
            )
            .unwrap()
    };
    assert_eq!(
        derived(1),
        (
            Some(128.0),
            "8A".to_owned(),
            Some(outcome.source_id),
            Some(1)
        )
    );
    assert_eq!(
        derived(2),
        (None, String::new(), Some(outcome.source_id), Some(1)),
        "0.00 means not analyzed"
    );
    assert_eq!(
        derived(3),
        (
            Some(140.0),
            "11B".to_owned(),
            Some(outcome.source_id),
            Some(1)
        )
    );
    assert_eq!(
        derived(4),
        (None, String::new(), None, None),
        "an unlinked track is untouched"
    );
}

#[test]
fn reading_an_unchanged_export_again_changes_nothing() {
    let mut library = library();
    let collection = export("128.00", true);
    library
        .database
        .replace_mirror(&source(Some(1)), &collection, 100)
        .unwrap();
    let ids_before = text(
        &library.database,
        "SELECT group_concat(id) FROM (SELECT id FROM rb_track ORDER BY id)",
    );
    let again = library
        .database
        .replace_mirror(&source(Some(2)), &collection, 200)
        .unwrap();
    assert!(again.unchanged);
    assert_eq!(again.generation, 1);
    assert_eq!(
        text(
            &library.database,
            "SELECT group_concat(id) FROM (SELECT id FROM rb_track ORDER BY id)"
        ),
        ids_before,
        "the rows were not rewritten"
    );
    let status = &mirror_sources(&library.database.connection).unwrap()[0];
    assert_eq!(
        (status.generation, status.last_read_ms, status.file_mtime_ms),
        (1, Some(200), Some(2))
    );
    assert_eq!(
        scalar(
            &library.database,
            "SELECT derived_generation FROM track WHERE id = 1"
        ),
        1
    );
}

#[test]
fn a_changed_export_advances_the_generation_and_replaces_the_values() {
    let mut library = library();
    library
        .database
        .replace_mirror(&source(None), &export("128.00", true), 100)
        .unwrap();
    let next = library
        .database
        .replace_mirror(&source(None), &export("131.50", true), 200)
        .unwrap();
    assert!(!next.unchanged);
    assert_eq!(next.generation, 2);
    let db = &library.database;
    assert_eq!(
        scalar(db, "SELECT COUNT(*) FROM rb_track"),
        5,
        "replaced, not added to"
    );
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM playlist"), 3);
    assert_eq!(
        text(db, "SELECT CAST(bpm AS TEXT) FROM track WHERE id = 1"),
        "131.5"
    );
    assert_eq!(
        scalar(db, "SELECT derived_generation FROM track WHERE id = 1"),
        2
    );
}

#[test]
fn an_entry_that_leaves_the_export_takes_its_tempo_and_key_with_it() {
    let mut library = library();
    library
        .database
        .replace_mirror(&source(None), &export("128.00", true), 100)
        .unwrap();
    assert_eq!(
        scalar(
            &library.database,
            "SELECT COUNT(*) FROM track WHERE bpm IS NOT NULL"
        ),
        2
    );
    let next = library
        .database
        .replace_mirror(&source(None), &export("128.00", false), 200)
        .unwrap();
    assert_eq!(next.entries, 4);
    let db = &library.database;
    assert_eq!(
        db.connection
            .query_row(
                "SELECT bpm, musical_key, derived_source_id FROM track WHERE id = 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, Option<f64>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                    ))
                }
            )
            .unwrap(),
        (None, String::new(), None),
        "no stale value is left looking current"
    );
    assert_eq!(
        scalar(db, "SELECT derived_generation FROM track WHERE id = 3"),
        2
    );
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM rb_cue"), 0);
}

#[test]
fn indexing_a_file_changes_how_the_same_export_reads() {
    let mut library = library();
    let collection = export("128.00", true);
    let first = library
        .database
        .replace_mirror(&source(None), &collection, 100)
        .unwrap();
    assert_eq!((first.matched, first.not_indexed), (3, 1));
    scan(
        &mut library.database,
        library.root,
        &[
            &["Artist", "Rel", "One.aif"],
            &["Artist", "Rel", "Two.aif"],
            &["Artist", "Rel", "Three.aif"],
            &["Artist", "Rel", "Gone.aif"],
        ],
        150,
    );
    let second = library
        .database
        .replace_mirror(&source(None), &collection, 200)
        .unwrap();
    assert!(!second.unchanged, "the match results differ");
    assert_eq!(
        (second.generation, second.matched, second.not_indexed),
        (2, 4, 0)
    );
}

#[test]
fn a_failed_read_leaves_the_previous_mirror_and_records_the_error() {
    let mut library = library();
    library
        .database
        .replace_mirror(&source(None), &export("128.00", true), 100)
        .unwrap();
    library
        .database
        .record_mirror_failure(Path::new(EXPORT_PATH), "invalid XML syntax", 300)
        .unwrap();
    let status = &mirror_sources(&library.database.connection).unwrap()[0];
    assert_eq!(status.generation, 1);
    assert_eq!(status.last_error.as_deref(), Some("invalid XML syntax"));
    assert_eq!(
        (status.last_read_ms, status.last_attempt_ms),
        (Some(100), Some(300))
    );
    assert_eq!(
        scalar(&library.database, "SELECT COUNT(*) FROM rb_track"),
        5
    );
    assert_eq!(
        scalar(
            &library.database,
            "SELECT derived_generation FROM track WHERE id = 1"
        ),
        1
    );
    library
        .database
        .replace_mirror(&source(None), &export("128.00", true), 400)
        .unwrap();
    let status = &mirror_sources(&library.database.connection).unwrap()[0];
    assert_eq!(status.last_error, None, "a good read clears the error");
}

#[test]
fn a_replacement_that_breaks_halfway_changes_nothing() {
    let mut library = library();
    library
        .database
        .replace_mirror(&source(None), &export("128.00", true), 100)
        .unwrap();
    let mut broken = export("999.00", true);
    let duplicate = broken.tracks[1].clone();
    broken.tracks.push(duplicate);
    let result = library.database.replace_mirror(&source(None), &broken, 200);
    assert!(matches!(result, Err(Error::Sqlite(_))), "{result:?}");
    let db = &library.database;
    assert_eq!(scalar(db, "SELECT generation FROM rb_source"), 1);
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM rb_track"), 5);
    assert_eq!(scalar(db, "SELECT COUNT(*) FROM rb_cue"), 2);
    assert_eq!(
        text(db, "SELECT CAST(bpm AS TEXT) FROM track WHERE id = 1"),
        "128.0"
    );
    assert_eq!(
        scalar(db, "SELECT derived_generation FROM track WHERE id = 1"),
        1
    );
}

#[test]
fn two_exports_are_mirrored_independently() {
    let mut library = library();
    let other = MirrorSource {
        path: Path::new("/exports/other.xml"),
        file_mtime_ms: None,
    };
    library
        .database
        .replace_mirror(&source(None), &export("128.00", true), 100)
        .unwrap();
    library
        .database
        .replace_mirror(&other, &export("128.00", false), 110)
        .unwrap();
    library
        .database
        .replace_mirror(&source(None), &export("130.00", true), 120)
        .unwrap();
    let sources = mirror_sources(&library.database.connection).unwrap();
    let generations: Vec<_> = sources.iter().map(|s| (s.entries, s.generation)).collect();
    assert_eq!(generations, [(5, 2), (4, 1)]);
    assert_eq!(
        scalar(&library.database, "SELECT COUNT(*) FROM rb_track"),
        9
    );
}

#[test]
fn a_real_file_is_read_then_mirrored_and_a_bad_one_is_an_error() {
    let mut library = library();
    let folder = tempdir().unwrap();
    let path = folder.path().join("collection.xml");
    fs::write(
        &path,
        "<DJ_PLAYLISTS><COLLECTION Entries=\"1\"><TRACK TrackID=\"1\" AverageBpm=\"124.00\" \
         Location=\"file://localhost/G:/Library/Artist/Rel/One.aif\"/></COLLECTION></DJ_PLAYLISTS>",
    )
    .unwrap();
    let read = read_export(&path).unwrap();
    assert!(read.file_mtime_ms.is_some_and(|ms| ms > 1_000_000_000_000));
    let outcome = library
        .database
        .replace_mirror(
            &MirrorSource {
                path: &path,
                file_mtime_ms: read.file_mtime_ms,
            },
            &read.collection,
            500,
        )
        .unwrap();
    assert_eq!((outcome.entries, outcome.matched), (1, 1));

    fs::write(
        folder.path().join("bad.xml"),
        "<DJ_PLAYLISTS><COLLECTION Entries=\"2\">",
    )
    .unwrap();
    assert!(matches!(
        read_export(&folder.path().join("bad.xml")),
        Err(Error::Export(_))
    ));
    assert!(matches!(
        read_export(&folder.path().join("none.xml")),
        Err(Error::Io(_))
    ));
}

#[test]
fn a_big_export_is_replaced_in_one_pass() {
    let mut library = library();
    let mut tracks = String::new();
    for n in 1..=3_000 {
        writeln!(
            tracks,
            "<TRACK TrackID=\"{n}\" Name=\"T{n}\" AverageBpm=\"{}.00\" \
             Location=\"file://localhost/G:/Library/Artist/Rel/Bulk%20{n}.aif\"/>",
            100 + n % 50
        )
        .unwrap();
    }
    let collection = xml::read(Cursor::new(
        format!("<DJ_PLAYLISTS><COLLECTION Entries=\"3000\">{tracks}</COLLECTION></DJ_PLAYLISTS>")
            .as_bytes(),
    ))
    .unwrap();
    let files: Vec<[String; 3]> = (1..=1_500)
        .map(|n| {
            [
                "Artist".to_owned(),
                "Rel".to_owned(),
                format!("Bulk {n}.aif"),
            ]
        })
        .collect();
    let borrowed: Vec<Vec<&str>> = files
        .iter()
        .map(|f| f.iter().map(String::as_str).collect())
        .collect();
    let refs: Vec<&[&str]> = borrowed.iter().map(Vec::as_slice).collect();
    scan(&mut library.database, library.root, &refs, 50);
    let outcome = library
        .database
        .replace_mirror(&source(None), &collection, 60)
        .unwrap();
    assert_eq!(
        (outcome.entries, outcome.matched, outcome.not_indexed),
        (3_000, 1_500, 1_500)
    );
}

#[test]
fn a_track_reads_back_the_cues_and_grid_rekordbox_gave_its_file() {
    use crate::mirror::{read_track_analysis, CueRow, TempoRow};

    let mut library = library();
    assert_eq!(
        read_track_analysis(&library.database.connection, 1).unwrap(),
        None,
        "nothing is known before an export is read"
    );
    library
        .database
        .replace_mirror(&source(Some(77)), &export("128.00", true), 100)
        .unwrap();
    let db = &library.database.connection;
    let analysis = read_track_analysis(db, 1).unwrap().unwrap();
    assert_eq!((analysis.generation, analysis.bpm), (1, Some(128.0)));
    assert_eq!(analysis.musical_key, "8A");
    assert_eq!(
        analysis.cues,
        [
            CueRow {
                kind: 0,
                number: Some(1),
                start_seconds: 12.345_678,
                end_seconds: None,
                name: "Drop".into()
            },
            CueRow {
                kind: 4,
                number: Some(0),
                start_seconds: 60.25,
                end_seconds: Some(64.5),
                name: "Loop".into()
            },
        ],
        "in time order, with fractions kept"
    );
    assert_eq!(analysis.tempos.len(), 2);
    assert_eq!(
        analysis.tempos[0],
        TempoRow {
            start_seconds: 0.0255,
            bpm: 128.0,
            meter: Some("4/4".into()),
            beat: Some(1)
        }
    );
    assert_eq!(analysis.tempos[1].bpm, 129.5);
    assert_eq!(
        read_track_analysis(db, 4).unwrap(),
        None,
        "a track with no matched entry has none"
    );
    assert_eq!(read_track_analysis(db, 999).unwrap(), None);
}

#[test]
fn when_two_exports_match_the_one_read_last_wins() {
    use crate::mirror::read_track_analysis;

    let mut library = library();
    library
        .database
        .replace_mirror(&source(Some(77)), &export("128.00", true), 100)
        .unwrap();
    let second = MirrorSource {
        path: Path::new("/exports/newer.xml"),
        file_mtime_ms: Some(88),
    };
    library
        .database
        .replace_mirror(&second, &export("140.00", true), 200)
        .unwrap();
    let analysis = read_track_analysis(&library.database.connection, 1)
        .unwrap()
        .unwrap();
    assert_eq!(analysis.bpm, Some(140.0));
}
