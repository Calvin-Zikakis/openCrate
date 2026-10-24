//! The whole path from a folder of audio to a browsable catalog, on synthetic files only:
//! register a root, scan it, read tags, then change, delete and remove files.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use oc_paths::Availability;
use oc_tag::fixtures::{aiff, flac, mp3, wav, wav_frames, write_tags};
use oc_tag::TagInfo;
use tempfile::{tempdir, TempDir};

use crate::library::{Format, LibraryItem, LibraryQuery, Sort, View};
use crate::service::{LibraryService, ScanOptions, ScanPhase, ScanProgress};
use crate::Error;

struct Fixture {
    _database: TempDir,
    folder: TempDir,
    service: LibraryService,
}

fn tags(title: &str, artist: &str, album: &str, track: u32) -> TagInfo {
    TagInfo {
        title: Some(title.into()),
        artist: Some(artist.into()),
        album: Some(album.into()),
        track_number: Some(track),
        ..TagInfo::default()
    }
}

fn put(folder: &Path, relative: &str, bytes: &[u8], info: Option<&TagInfo>) -> PathBuf {
    let path = folder.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    if let Some(info) = info {
        write_tags(&path, info);
    }
    path
}

/// Push a file's modified time forward so a rewrite is always seen as a change.
fn touch_later(path: &Path, seconds: u64) {
    let later = SystemTime::now() + Duration::from_secs(seconds);
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(later)
        .unwrap();
}

fn fixture() -> Fixture {
    let folder = tempdir().unwrap();
    let root = folder.path();
    let mut night = tags("Night Drive", "Studio 001", "Between Spaces", 1);
    night.year = Some(2019);
    put(
        root,
        "Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif",
        &aiff(2),
        Some(&night),
    );
    put(
        root,
        "Studio 001/Between Spaces/02 - Studio 001 - Soft Focus.wav",
        &wav(1),
        Some(&tags("Soft Focus", "Studio 001", "Between Spaces", 2)),
    );
    put(
        root,
        "Ensemble 002/Quiet Signals/01 - Open Water.mp3",
        &mp3(40),
        Some(&tags("Open Water", "Ensemble 002", "Quiet Signals", 1)),
    );
    put(
        root,
        "Ensemble 002/Quiet Signals/03 - Low Orbit.flac",
        &flac(
            3,
            &[
                ("TITLE", "Low Orbit"),
                ("ARTIST", "Ensemble 002"),
                ("ALBUM", "Quiet Signals"),
                ("TRACKNUMBER", "3"),
            ],
        ),
        None,
    );
    put(root, "Loose/untagged track.wav", &wav(1), None);
    put(root, "Broken/notes.mp3", b"this is not audio", None);
    let mut cut = tags("Cut", "Guest Artist", "Compilation", 1);
    cut.album_artist = Some("Various".into());
    put(
        root,
        "Various/Compilation/01 - Guest Artist - Cut.wav",
        &wav(1),
        Some(&cut),
    );

    let database = tempdir().unwrap();
    let service = LibraryService::open(database.path(), 2).unwrap();
    Fixture {
        _database: database,
        folder,
        service,
    }
}

fn read(fixture: &Fixture, query: &LibraryQuery) -> (u64, Vec<LibraryItem>) {
    let page = fixture.service.read_page(query, 0, 200).unwrap();
    (page.total, page.items)
}

fn names(items: &[LibraryItem]) -> Vec<String> {
    items.iter().map(|item| item.title.clone()).collect()
}

fn query(view: View, search: &str) -> LibraryQuery {
    let mut query = LibraryQuery::new(view);
    query.search = search.into();
    query.sort = Sort::Title;
    query
}

fn scan(fixture: &Fixture, root: i64) -> crate::service::ScanSummary {
    fixture
        .service
        .scan_root(root, &ScanOptions::default())
        .unwrap()
}

#[test]
fn a_folder_of_audio_becomes_a_browsable_catalog() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Test library")
        .unwrap();
    let summary = scan(&fixture, root);
    assert_eq!(summary.availability, Availability::Available);
    assert_eq!((summary.scan.inserted, summary.tags_read), (7, 7));
    assert_eq!(summary.catalog.tracks_created, 7);
    assert_eq!(summary.catalog.tag_errors, 1, "the file that is not audio");

    let (total, tracks) = read(&fixture, &query(View::Tracks, ""));
    assert_eq!(total, 7);
    assert_eq!(
        names(&tracks),
        [
            "Cut",
            "Low Orbit",
            "Night Drive",
            "notes",
            "Open Water",
            "Soft Focus",
            "untagged track"
        ]
    );
    let night = tracks.iter().find(|t| t.title == "Night Drive").unwrap();
    assert_eq!(
        (night.artist.as_str(), night.release.as_str()),
        ("Studio 001", "Between Spaces")
    );
    assert_eq!(
        (night.format.as_deref(), night.duration_ms),
        (Some("AIFF"), 2_000)
    );
    let flac = tracks.iter().find(|t| t.title == "Low Orbit").unwrap();
    assert_eq!(
        (flac.format.as_deref(), flac.duration_ms),
        (Some("FLAC"), 3_000)
    );

    // Untagged and unreadable files are catalogued from their place in the library.
    let loose = tracks.iter().find(|t| t.title == "untagged track").unwrap();
    assert_eq!(
        (loose.artist.as_str(), loose.release.as_str()),
        ("Unknown Artist", "Loose")
    );
    let broken = tracks.iter().find(|t| t.title == "notes").unwrap();
    assert_eq!(broken.release, "Broken");
    assert_eq!(
        broken.format.as_deref(),
        Some("MP3"),
        "kept from the extension when unreadable"
    );

    let (artists, artist_rows) = read(&fixture, &query(View::Artists, ""));
    assert_eq!(artists, 4, "Various has a release but no track of its own");
    assert_eq!(
        names(&artist_rows),
        [
            "Ensemble 002",
            "Guest Artist",
            "Studio 001",
            "Unknown Artist"
        ]
    );
    assert_eq!(
        artist_rows
            .iter()
            .find(|a| a.title == "Unknown Artist")
            .unwrap()
            .track_count,
        2
    );
    let (releases, release_rows) = read(&fixture, &query(View::Releases, ""));
    assert_eq!(releases, 5);
    let compilation = release_rows
        .iter()
        .find(|r| r.title == "Compilation")
        .unwrap();
    assert_eq!(
        (compilation.artist.as_str(), compilation.track_count),
        ("Various", 1)
    );

    // Search, formats and sorts work on the new catalog.
    assert_eq!(read(&fixture, &query(View::Tracks, "night")).0, 1);
    assert_eq!(
        read(&fixture, &query(View::Tracks, "quiet sig")).0,
        2,
        "release words reach its tracks"
    );
    assert_eq!(read(&fixture, &query(View::Releases, "various")).0, 1);
    assert_eq!(read(&fixture, &query(View::Artists, "guest")).0, 1);
    let mut aiff_only = LibraryQuery::new(View::Tracks);
    aiff_only.format = Some(Format::Aiff);
    assert_eq!(read(&fixture, &aiff_only).0, 1);
    aiff_only.format = Some(Format::Mp3);
    assert_eq!(read(&fixture, &aiff_only).0, 2);
}

#[test]
fn a_second_scan_of_an_unchanged_folder_reads_nothing_again() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Test")
        .unwrap();
    scan(&fixture, root);
    let again = scan(&fixture, root);
    assert_eq!(
        (again.scan.unchanged, again.scan.inserted, again.tags_read),
        (7, 0, 0)
    );
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 7);
}

#[test]
fn a_retagged_file_updates_its_track_and_the_search_index() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Test")
        .unwrap();
    scan(&fixture, root);
    let before = read(&fixture, &query(View::Tracks, "soft")).1[0].id;

    let path = fixture
        .folder
        .path()
        .join("Studio 001/Between Spaces/02 - Studio 001 - Soft Focus.wav");
    write_tags(
        &path,
        &tags("Velvet Rope Remix", "Studio 001", "Between Spaces", 2),
    );
    touch_later(&path, 60);
    let summary = scan(&fixture, root);
    assert_eq!(
        (
            summary.scan.updated,
            summary.tags_read,
            summary.catalog.tracks_updated
        ),
        (1, 1, 1)
    );

    assert_eq!(
        read(&fixture, &query(View::Tracks, "soft")).0,
        0,
        "the old title is gone from search"
    );
    let found = read(&fixture, &query(View::Tracks, "velvet rope"));
    assert_eq!(
        (found.0, found.1[0].id),
        (1, before),
        "the same track, not a new one"
    );
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 7);
}

#[test]
fn a_deleted_file_stays_in_the_catalog_and_a_vanished_root_changes_nothing() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Test")
        .unwrap();
    scan(&fixture, root);
    fs::remove_file(fixture.folder.path().join("Loose/untagged track.wav")).unwrap();
    let after_delete = scan(&fixture, root);
    assert_eq!(after_delete.scan.marked_missing, 1);
    assert_eq!(
        read(&fixture, &query(View::Tracks, "")).0,
        7,
        "browsable, not deleted"
    );

    fs::remove_dir_all(fixture.folder.path()).unwrap();
    let offline = scan(&fixture, root);
    assert_eq!(offline.availability, Availability::Unavailable);
    assert!(offline.scan.skipped);
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 7);
    let missing: i64 = {
        let lease = fixture
            .service
            .read_page(&query(View::Tracks, ""), 0, 1)
            .unwrap();
        i64::try_from(lease.total).unwrap()
    };
    assert_eq!(missing, 7);
}

#[test]
fn an_unplugged_folder_is_noticed_without_a_scan_and_marks_nothing_missing() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Library")
        .unwrap();
    scan(&fixture, root);
    let flagged = |title: &str| read(&fixture, &query(View::Tracks, title)).1[0].missing;
    assert!(!flagged("night"));
    assert_eq!(
        fixture.service.check_roots().unwrap()[0].root.availability,
        Availability::Available
    );

    // The drive goes away: the check notices, the library stays browsable, nothing is "missing".
    let moved = fixture.folder.path().with_extension("unplugged");
    fs::rename(fixture.folder.path(), &moved).unwrap();
    let after = fixture.service.check_roots().unwrap();
    assert_eq!(after[0].root.availability, Availability::Unavailable);
    assert_eq!((after[0].files, after[0].missing), (7, 0));
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 7);
    assert!(!flagged("night"), "an offline folder is not a missing file");

    // A different directory at the same path is not trusted until the owner says so.
    fs::create_dir(fixture.folder.path()).unwrap();
    assert_eq!(
        fixture.service.check_roots().unwrap()[0].root.availability,
        Availability::Unverified
    );
    let summary = scan(&fixture, root);
    assert!(summary.scan.skipped && summary.scan.marked_missing == 0);
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 7);

    // The original comes back: same path, but it is a new directory object, so confirm it.
    fs::remove_dir(fixture.folder.path()).unwrap();
    fs::rename(&moved, fixture.folder.path()).unwrap();
    fixture.service.confirm_root(root).unwrap();
    let confirmed = fixture.service.check_roots().unwrap();
    assert_eq!(confirmed[0].root.availability, Availability::Available);
    let again = scan(&fixture, root);
    assert_eq!((again.scan.unchanged, again.scan.marked_missing), (7, 0));

    // A real deletion is a missing file and is flagged on the track.
    fs::remove_file(
        fixture
            .folder
            .path()
            .join("Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif"),
    )
    .unwrap();
    scan(&fixture, root);
    assert!(flagged("night"));
    assert!(!flagged("water"));
    assert!(matches!(
        fixture.service.confirm_root(999),
        Err(Error::UnknownRoot(999))
    ));
}

#[test]
fn the_database_folder_can_never_be_inside_or_around_a_library() {
    // The database folder is the library's own parent here: the worst case, a database inside the
    // music folder, where a sync tool would capture it mid-write.
    let library = tempdir().unwrap();
    let data = library.path().join("openCrate data");
    fs::create_dir_all(&data).unwrap();
    let service = LibraryService::open(&data, 1).unwrap();
    assert!(
        matches!(
            service.add_root(library.path(), "Parent"),
            Err(Error::Root(_))
        ),
        "contains the database"
    );
    assert!(
        matches!(service.add_root(&data, "Itself"), Err(Error::Root(_))),
        "is the database folder"
    );
    fs::create_dir_all(data.join("snapshots")).unwrap();
    assert!(
        matches!(
            service.add_root(&data.join("snapshots"), "Inside"),
            Err(Error::Root(_))
        ),
        "lies inside it"
    );
    let message = service.add_root(&data, "Itself").unwrap_err().to_string();
    assert!(
        message.contains("where openCrate keeps its own data"),
        "{message}"
    );

    // A sibling folder is fine.
    let music = library.path().join("Music");
    fs::create_dir_all(&music).unwrap();
    assert!(service.add_root(&music, "Music").is_ok());
}

#[test]
fn roots_are_validated_and_a_cancelled_scan_applies_nothing() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Test")
        .unwrap();
    let inside = fixture.folder.path().join("Studio 001");
    assert!(matches!(
        fixture.service.add_root(&inside, "Nested"),
        Err(Error::Root(_))
    ));
    assert!(matches!(
        fixture.service.add_root(fixture.folder.path(), "Again"),
        Err(Error::Root(_))
    ));
    let a_file = fixture.folder.path().join("Broken/notes.mp3");
    assert!(matches!(
        fixture.service.add_root(&a_file, "File"),
        Err(Error::Root(_))
    ));
    assert!(matches!(
        fixture
            .service
            .add_root(&fixture.folder.path().join("nope"), "Missing"),
        Err(Error::Root(oc_paths::RootError::Io(_)))
    ));
    assert!(matches!(
        fixture.service.scan_root(999, &ScanOptions::default()),
        Err(Error::UnknownRoot(999))
    ));

    let stop = AtomicBool::new(true);
    let summary = fixture
        .service
        .scan_root(
            root,
            &ScanOptions {
                cancel: Some(&stop),
                ..ScanOptions::default()
            },
        )
        .unwrap();
    assert!(summary.cancelled);
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 0);
}

fn scan_with_progress(fixture: &Fixture, root: i64) -> Vec<ScanProgress> {
    let seen = std::cell::RefCell::new(Vec::new());
    let record = |progress: &ScanProgress| seen.borrow_mut().push(*progress);
    fixture
        .service
        .scan_root(
            root,
            &ScanOptions {
                progress: Some(&record),
                ..ScanOptions::default()
            },
        )
        .unwrap();
    seen.into_inner()
}

#[test]
fn progress_arrives_in_order_and_only_moves_forward() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Test")
        .unwrap();
    let steps = scan_with_progress(&fixture, root);
    let phases: Vec<ScanPhase> = steps.iter().map(|step| step.phase).collect();
    assert_eq!(
        &phases[..4],
        [
            ScanPhase::Checking,
            ScanPhase::Walking,
            ScanPhase::Applying,
            ScanPhase::ReadingTags
        ]
    );
    assert_eq!(phases.last(), Some(&ScanPhase::Finished));
    assert!(
        phases.windows(2).all(|pair| pair[0] as u8 <= pair[1] as u8),
        "{phases:?}"
    );
    let applying = steps
        .iter()
        .find(|step| step.phase == ScanPhase::Applying)
        .unwrap();
    assert_eq!(applying.files_seen, 7);
    let last = steps.last().unwrap();
    assert_eq!(
        (last.files_seen, last.tags_total, last.tags_read),
        (7, 7, 7)
    );
    assert!(steps
        .windows(2)
        .all(|pair| pair[0].tags_read <= pair[1].tags_read));

    // Nothing left to read the second time.
    let again = scan_with_progress(&fixture, root);
    assert_eq!(
        again.last().map(|step| (step.tags_total, step.tags_read)),
        Some((0, 0))
    );

    // A folder that has gone is checked and finished without a walk.
    fs::remove_dir_all(fixture.folder.path()).unwrap();
    let gone: Vec<ScanPhase> = scan_with_progress(&fixture, root)
        .iter()
        .map(|step| step.phase)
        .collect();
    assert_eq!(gone, [ScanPhase::Checking, ScanPhase::Finished]);
}

#[test]
fn the_folder_list_reports_availability_and_counts() {
    let fixture = fixture();
    assert_eq!(fixture.service.roots().unwrap().len(), 0);
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Test library")
        .unwrap();
    let listed = fixture.service.roots().unwrap();
    assert_eq!(
        (listed[0].root.id, listed[0].files, listed[0].missing),
        (root, 0, 0)
    );
    scan(&fixture, root);
    fs::remove_file(fixture.folder.path().join("Loose/untagged track.wav")).unwrap();
    scan(&fixture, root);
    let listed = fixture.service.roots().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].files, listed[0].missing), (7, 1));
    assert_eq!(listed[0].root.label, "Test library");
    assert_eq!(listed[0].root.availability, Availability::Available);
}

#[test]
fn removing_a_folder_removes_its_index_and_leaves_the_music_alone() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Library")
        .unwrap();
    scan(&fixture, root);
    fixture.service.add_alias(root, "/G:/Library").unwrap();
    let exports = tempdir().unwrap();
    let path = windows_export(exports.path());
    assert_eq!(fixture.service.import_export(&path).unwrap().matched, 2);
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 7);

    assert_eq!(fixture.service.remove_root(root).unwrap(), 7);
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 0);
    assert_eq!(
        read(&fixture, &query(View::Tracks, "night")).0,
        0,
        "the search index is cleaned too"
    );
    assert_eq!(
        read(&fixture, &query(View::Artists, "")).0,
        0,
        "empty artists are not shown"
    );
    assert_eq!(read(&fixture, &query(View::Releases, "")).0, 0);
    assert_eq!(fixture.service.roots().unwrap().len(), 0);
    assert!(
        fixture.folder.path().join("Broken/notes.mp3").exists(),
        "the music itself is untouched"
    );

    // The mirror survives; its entries are simply outside every folder again.
    let counts = fixture.service.mirrors().unwrap().remove(0).counts;
    assert_eq!((counts.matched, counts.unmapped), (0, 4));
    assert!(matches!(
        fixture.service.remove_root(root),
        Err(Error::UnknownRoot(_))
    ));

    // The same folder can be added and scanned again from scratch.
    let again = fixture
        .service
        .add_root(fixture.folder.path(), "Library")
        .unwrap();
    assert_eq!(scan(&fixture, again).scan.inserted, 7);
    assert_eq!(read(&fixture, &query(View::Tracks, "")).0, 7);
}

/// An export written on a Windows machine whose music lived at `G:\Library`: the library is
/// now at another path, so nothing matches until the prefix is explained.
fn windows_export(folder: &Path) -> PathBuf {
    let xml = "<DJ_PLAYLISTS Version=\"1.0.0\"><PRODUCT Name=\"Synthetic\"/>\
        <COLLECTION Entries=\"4\">\
        <TRACK TrackID=\"1\" Name=\"Night Drive\" AverageBpm=\"128.00\" Tonality=\"8A\" \
         Location=\"file://localhost/G:/Library/Studio%20001/Between%20Spaces/01%20-%20Studio%20001%20-%20Night%20Drive.aif\">\
         <POSITION_MARK Name=\"Drop\" Type=\"0\" Start=\"31.5\" Num=\"1\"/></TRACK>\
        <TRACK TrackID=\"2\" Name=\"Open Water\" AverageBpm=\"122.50\" Tonality=\"5A\" \
         Location=\"file://localhost/G:/Library/Ensemble%20002/Quiet%20Signals/01%20-%20Open%20Water.mp3\"/>\
        <TRACK TrackID=\"3\" Name=\"Deleted\" Location=\"file://localhost/G:/$RECYCLE.BIN/x.aif\"/>\
        <TRACK TrackID=\"4\" Name=\"Gone\" AverageBpm=\"100.00\" \
         Location=\"file://localhost/G:/Library/Studio%20001/Between%20Spaces/99%20-%20Gone.aif\"/>\
        </COLLECTION><PLAYLISTS><NODE Type=\"0\" Name=\"ROOT\" Count=\"1\">\
        <NODE Type=\"1\" Name=\"Set\" KeyType=\"0\" Entries=\"3\"><TRACK Key=\"1\"/><TRACK Key=\"2\"/><TRACK Key=\"9\"/></NODE>\
        </NODE></PLAYLISTS></DJ_PLAYLISTS>";
    let path = folder.join("collection.xml");
    fs::write(&path, xml).unwrap();
    path
}

#[test]
fn an_export_from_another_machine_is_explained_with_evidence_then_tempo_and_key_appear() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Library")
        .unwrap();
    scan(&fixture, root);
    let exports = tempdir().unwrap();
    let path = windows_export(exports.path());

    // Nothing is in the library under that drive prefix yet.
    let first = fixture.service.import_export(&path).unwrap();
    assert_eq!(
        (
            first.entries,
            first.matched,
            first.unmapped,
            first.generation
        ),
        (4, 0, 4, 1)
    );
    let mirror = fixture.service.mirrors().unwrap().remove(0);
    assert_eq!(
        (
            mirror.counts.unmapped,
            mirror.counts.playlists,
            mirror.counts.unresolved_references
        ),
        (4, 1, 1)
    );
    assert_eq!(read(&fixture, &query(View::Tracks, "night")).1[0].bpm, None);

    // The index itself shows which prefix makes entries match, and how many.
    let suggestions = fixture
        .service
        .alias_suggestions(first.source_id, 5)
        .unwrap();
    assert_eq!(suggestions[0].prefix, "/G:/Library");
    assert_eq!((suggestions[0].root_id, suggestions[0].entries), (root, 2));
    assert_eq!(suggestions[0].root_label, "Library");
    assert!(
        suggestions
            .iter()
            .all(|s| s.prefix != "/G:/Library/Studio 001"),
        "the wrong depth matches nothing"
    );

    assert!(fixture
        .service
        .add_alias(root, &suggestions[0].prefix)
        .unwrap());
    assert!(
        !fixture.service.add_alias(root, "/G:/Library").unwrap(),
        "already known"
    );
    let second = fixture.service.reread_mirror(first.source_id).unwrap();
    assert_eq!(
        (
            second.matched,
            second.unmapped,
            second.not_indexed,
            second.generation
        ),
        (2, 1, 1, 2)
    );
    assert_eq!(second.derived_set, 2);

    // The library now shows rekordbox's tempo and key for the matched tracks.
    let night = &read(&fixture, &query(View::Tracks, "night")).1[0];
    assert_eq!((night.bpm, night.musical_key.as_str()), (Some(128.0), "8A"));
    let water = &read(&fixture, &query(View::Tracks, "water")).1[0];
    assert_eq!((water.bpm, water.musical_key.as_str()), (Some(122.5), "5A"));
    let mut by_tempo = query(View::Tracks, "");
    by_tempo.sort = Sort::Bpm;
    assert_eq!(
        read(&fixture, &by_tempo).1[0].title,
        "Open Water",
        "sorted by rekordbox's tempo"
    );

    let counts = fixture.service.mirrors().unwrap().remove(0).counts;
    assert_eq!(
        (counts.matched, counts.not_indexed, counts.unmapped),
        (2, 1, 1)
    );

    // Unchanged export, unchanged library: nothing to do.
    let again = fixture.service.import_export(&path).unwrap();
    assert!(again.unchanged);
}

#[test]
fn a_playlist_lists_its_matched_tracks_in_the_playlists_own_order() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Library")
        .unwrap();
    scan(&fixture, root);
    fixture.service.add_alias(root, "/G:/Library").unwrap();
    let exports = tempdir().unwrap();
    let path = windows_export(exports.path());
    fixture.service.import_export(&path).unwrap();

    let rows = fixture.service.playlists().unwrap();
    let set = rows.iter().find(|row| row.name == "Set").unwrap();
    let folder = rows.iter().find(|row| row.name == "ROOT").unwrap();
    assert_eq!(
        (set.is_folder, set.parent_id, set.entries, set.in_library),
        (false, Some(folder.id), 3, 2)
    );
    assert!(folder.is_folder && folder.parent_id.is_none());

    let mut in_set = LibraryQuery::new(View::Tracks);
    in_set.playlist_id = Some(set.id);
    in_set.sort = Sort::Playlist;
    let (total, tracks) = read(&fixture, &in_set);
    assert_eq!(
        (total, names(&tracks)),
        (2, vec!["Night Drive".to_owned(), "Open Water".to_owned()]),
        "export order"
    );
    assert_eq!(tracks[0].bpm, Some(128.0));

    // Other sorts, search and filters still apply inside a playlist.
    in_set.sort = Sort::Title;
    assert_eq!(
        names(&read(&fixture, &in_set).1),
        ["Night Drive", "Open Water"]
    );
    in_set.search = "water".into();
    assert_eq!(read(&fixture, &in_set).0, 1);
    in_set.search = String::new();
    in_set.format = Some(Format::Mp3);
    assert_eq!(names(&read(&fixture, &in_set).1), ["Open Water"]);

    // The playlist order is the export's, not alphabetical: a reversed export flips it.
    let reversed = fs::read_to_string(&path).unwrap().replace(
        "<TRACK Key=\"1\"/><TRACK Key=\"2\"/><TRACK Key=\"9\"/>",
        "<TRACK Key=\"2\"/><TRACK Key=\"1\"/><TRACK Key=\"9\"/>",
    );
    fs::write(&path, reversed).unwrap();
    fixture.service.import_export(&path).unwrap();
    let set_now = fixture
        .service
        .playlists()
        .unwrap()
        .into_iter()
        .find(|row| row.name == "Set")
        .unwrap();
    let mut again = LibraryQuery::new(View::Tracks);
    again.playlist_id = Some(set_now.id);
    again.sort = Sort::Playlist;
    assert_eq!(
        names(&read(&fixture, &again).1),
        ["Open Water", "Night Drive"]
    );

    // Without a playlist the playlist sort is just the default order, and other views ignore it.
    let mut plain = LibraryQuery::new(View::Tracks);
    plain.sort = Sort::Playlist;
    assert_eq!(read(&fixture, &plain).0, 7);
    let mut releases = LibraryQuery::new(View::Releases);
    releases.playlist_id = Some(set_now.id);
    releases.sort = Sort::Playlist;
    assert_eq!(read(&fixture, &releases).0, 5);
}

#[test]
fn a_broken_export_is_recorded_and_the_previous_mirror_stays() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Library")
        .unwrap();
    scan(&fixture, root);
    fixture.service.add_alias(root, "/G:/Library").unwrap();
    let exports = tempdir().unwrap();
    let path = windows_export(exports.path());
    let good = fixture.service.import_export(&path).unwrap();
    assert_eq!(good.matched, 2);

    fs::write(&path, "<DJ_PLAYLISTS><COLLECTION Entries=\"9\">").unwrap();
    assert!(matches!(
        fixture.service.reread_mirror(good.source_id),
        Err(Error::Export(_))
    ));
    let mirror = fixture.service.mirrors().unwrap().remove(0);
    assert_eq!((mirror.status.generation, mirror.counts.matched), (1, 2));
    assert!(mirror.status.last_error.is_some());
    assert_eq!(
        read(&fixture, &query(View::Tracks, "night")).1[0].bpm,
        Some(128.0),
        "values stay"
    );

    assert!(matches!(
        fixture
            .service
            .import_export(&exports.path().join("none.xml")),
        Err(Error::Io(_))
    ));
    assert!(matches!(
        fixture.service.reread_mirror(404),
        Err(Error::UnknownSource(404))
    ));
    for bad in ["", "relative/path", "/G:/a/../b", "/G:/a//b"] {
        assert!(
            matches!(
                fixture.service.add_alias(root, bad),
                Err(Error::InvalidAlias)
            ),
            "{bad:?}"
        );
    }
    assert!(matches!(
        fixture.service.add_alias(99, "/G:/Library"),
        Err(Error::UnknownRoot(99))
    ));
}

/// `cargo test -p oc-core-db --release -- --ignored --nocapture benchmark_scan`
#[test]
#[ignore = "benchmark: run explicitly with --ignored --nocapture"]
fn benchmark_scan_9000_files() {
    use std::time::Instant;
    const FILES: usize = 9_000;
    let folder = tempdir().unwrap();
    let began = Instant::now();
    let body = wav_frames(2_000);
    for n in 0..FILES {
        let artist = format!("Artist {:03}", n / 90);
        let release = format!("Release {:02}", (n / 10) % 9);
        let path = folder
            .path()
            .join(&artist)
            .join(&release)
            .join(format!("{:02} - Track {n:05}.wav", n % 10 + 1));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &body).unwrap();
        write_tags(
            &path,
            &tags(
                &format!("Track {n:05}"),
                &artist,
                &release,
                u32::try_from(n % 10 + 1).unwrap(),
            ),
        );
    }
    eprintln!(
        "built {FILES} tagged files in {:.1} s",
        began.elapsed().as_secs_f64()
    );

    let database = tempdir().unwrap();
    let service = LibraryService::open(database.path(), 2).unwrap();
    let root = service.add_root(folder.path(), "Bench").unwrap();
    for (label, threads) in [("first scan", 4), ("rescan (nothing changed)", 4)] {
        let began = Instant::now();
        let summary = service
            .scan_root(
                root,
                &ScanOptions {
                    tag_threads: threads,
                    ..ScanOptions::default()
                },
            )
            .unwrap();
        eprintln!(
            "{label}: {:.2} s  (inserted {}, unchanged {}, tags read {}, tracks created {})",
            began.elapsed().as_secs_f64(),
            summary.scan.inserted,
            summary.scan.unchanged,
            summary.tags_read,
            summary.catalog.tracks_created
        );
    }
    let page = service
        .read_page(&LibraryQuery::new(View::Tracks), 0, 1)
        .unwrap();
    assert_eq!(usize::try_from(page.total).unwrap(), FILES);
    let size = fs::metadata(database.path().join(crate::DATABASE_NAME))
        .unwrap()
        .len();
    eprintln!("database {} KiB (WAL not checkpointed)", size / 1024);
}

#[test]
fn genres_are_counted_by_how_each_track_spells_them() {
    let fixture = fixture();
    let root = fixture
        .service
        .add_root(fixture.folder.path(), "Library")
        .unwrap();
    let with_genre = |title: &str, genre: &str| TagInfo {
        genre: Some(genre.into()),
        ..tags(title, "Studio 001", "Genres", 1)
    };
    put(
        fixture.folder.path(),
        "g/1.aif",
        &aiff(1),
        Some(&with_genre("One", "Deep House")),
    );
    put(
        fixture.folder.path(),
        "g/2.aif",
        &aiff(1),
        Some(&with_genre("Two", "Deep House")),
    );
    put(
        fixture.folder.path(),
        "g/3.aif",
        &aiff(1),
        Some(&with_genre("Three", "deep-house ")),
    );
    scan(&fixture, root);
    let genres = fixture.service.genres().unwrap();
    assert!(genres.contains(&("Deep House".to_owned(), 2)), "{genres:?}");
    assert!(
        genres.contains(&("deep-house".to_owned(), 1)),
        "trimmed: {genres:?}"
    );
    let ids = fixture
        .service
        .tracks_with_genres(&["deep-house".to_owned()])
        .unwrap();
    assert_eq!(ids.len(), 1);
}
