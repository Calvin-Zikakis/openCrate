use std::fs;
use std::path::{Path, PathBuf};

use rbl_db::fixture::{self, Shape};
use rbl_db::{Library, LibraryLocation, OpenMode};

use super::snapshot::windows_install_version;
use super::*;

const NOW: i64 = 1_792_800_000_000; // 2026-10-24 00:00 UTC

fn library(dir: &Path) -> LibraryLocation {
    fixture::build(dir, Shape::default()).unwrap()
}

#[test]
fn rbl_db_opens_a_fixture() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::open(library(temp.path()), OpenMode::ReadOnly).unwrap();
    assert_eq!(library.live_track_count().unwrap(), 40);
}

#[test]
fn a_snapshot_copies_and_checks_the_database() {
    let live = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let location = library(live.path());
    let snapshot = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    assert_eq!(snapshot.tracks, 40);
    assert_eq!(snapshot.playlists, 3);
    assert!(snapshot.dir.starts_with(backups.path()));
    assert!(snapshot.dir.join("snapshot.json").is_file());
    assert_eq!(snapshot.master_db, snapshot.dir.join("master.db"));
    assert!(snapshot
        .dir
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with("-read"));
}

#[test]
fn a_running_rekordbox_refuses_the_snapshot() {
    let live = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let location = library(live.path());
    let error = take_snapshot(&location, backups.path(), "read", true, NOW).unwrap_err();
    assert!(error.to_string().contains("rekordbox"), "{error}");
    assert_eq!(fs::read_dir(backups.path()).unwrap().count(), 0);
}

#[test]
fn a_corrupt_copy_is_refused_and_removed() {
    let live = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let location = library(live.path());
    let bytes = fs::read(&location.master_db).unwrap();
    fs::write(&location.master_db, &bytes[..bytes.len() / 3]).unwrap();
    assert!(take_snapshot(&location, backups.path(), "read", false, NOW).is_err());
    assert_eq!(fs::read_dir(backups.path()).unwrap().count(), 0);
}

#[test]
fn wal_rows_reach_the_copy() {
    let live = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let location = library(live.path());
    let mut open = Library::open(location.clone(), OpenMode::ReadWrite).unwrap();
    let connection = open.connection_mut();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    connection
        .pragma_update(None, "wal_autocheckpoint", 0)
        .unwrap();
    connection
        .execute(
            "UPDATE djmdPlaylist SET Name = 'Only in the WAL' WHERE ID = ?1",
            [fixture::playlist_id(0)],
        )
        .unwrap();
    assert!(live.path().join("master.db-wal").metadata().unwrap().len() > 0);
    // The writer stays open, as rekordbox's agent would, so nothing is checkpointed.
    let snapshot = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    assert!(
        !snapshot.dir.join("master.db-wal").exists(),
        "the copy is folded into one file"
    );
    let copy = Library::open(
        LibraryLocation {
            master_db: snapshot.master_db.clone(),
            ..location
        },
        OpenMode::ReadOnly,
    )
    .unwrap();
    let name: String = copy
        .connection()
        .query_row(
            "SELECT Name FROM djmdPlaylist WHERE ID = ?1",
            [fixture::playlist_id(0)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "Only in the WAL");
    drop(open);
}

#[test]
fn two_snapshots_in_one_second_get_their_own_folders() {
    let live = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let location = library(live.path());
    let first = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    let second = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    assert_ne!(first.dir, second.dir);
}

#[test]
fn the_plist_version_is_read() {
    let plist = "<plist><dict>\n\t<key>CFBundleName</key>\n\t<string>rekordbox</string>\n\t\
                 <key>CFBundleShortVersionString</key>\n\t<string>7.2.19.0342</string>\n</dict></plist>";
    assert_eq!(plist_version(plist).as_deref(), Some("7.2.19.0342"));
    assert_eq!(plist_version("<plist/>"), None);
}

#[test]
fn the_windows_version_comes_from_the_install_folder() {
    let program_files = tempfile::tempdir().unwrap();
    let install = |folder: &str, exe: bool| {
        let dir = program_files.path().join(folder);
        fs::create_dir_all(&dir).unwrap();
        if exe {
            fs::write(dir.join("rekordbox.exe"), b"").unwrap();
        }
    };
    install("rekordbox/rekordbox 7.2.14", true);
    install("Pioneer/rekordbox 6.8.5", true);
    install("rekordbox 7.2.19", true);
    // A folder without the program, and one not named for a version, do not count.
    install("rekordbox/rekordbox 7.9.0", false);
    install("rekordbox/rekordbox beta", true);
    let bases = [program_files.path().to_path_buf()];
    assert_eq!(windows_install_version(&bases).as_deref(), Some("7.2.19"));
    assert_eq!(windows_install_version(&[]), None);
}

#[test]
fn the_newest_installed_version_wins_numerically() {
    let versions = ["7.2.19.0342", "10.0.1.0001", "7.10.0.0001", "7.9.0.0001"];
    assert_eq!(
        newest_version(versions.iter().map(|v| (*v).to_owned())).as_deref(),
        Some("10.0.1.0001")
    );
    assert_eq!(
        newest_version(["7.9.0", "7.10.0"].iter().map(|v| (*v).to_owned())).as_deref(),
        Some("7.10.0")
    );
    assert_eq!(newest_version(std::iter::empty()), None);
}

#[test]
fn folder_names_use_the_utc_date() {
    // 2026-10-23 01:20:00 UTC
    assert_eq!(folder_stamp(1_792_718_400_000), "20261023-012000");
    assert_eq!(folder_stamp(0), "19700101-000000");
}

fn snapshot_of(
    live: &Path,
    backups: &Path,
    change: impl FnOnce(&rusqlite::Connection),
) -> Snapshot {
    let location = library(live);
    {
        let mut open = Library::open(location.clone(), OpenMode::ReadWrite).unwrap();
        change(open.connection_mut());
    }
    take_snapshot(&location, backups, "read", false, NOW).unwrap()
}

fn read(snapshot: &Snapshot) -> crate::xml::Collection {
    let passphrase = library_passphrase();
    read_snapshot(&snapshot.master_db, &passphrase).unwrap()
}

fn library_passphrase() -> String {
    let temp = tempfile::tempdir().unwrap();
    library(temp.path()).passphrase
}

fn root_children(collection: &crate::xml::Collection) -> &[crate::xml::PlaylistNode] {
    match collection.playlists.as_slice() {
        [crate::xml::PlaylistNode::Folder {
            attributes,
            children,
        }] => {
            assert_eq!(attributes.get("Name").map(String::as_str), Some("ROOT"));
            children
        }
        other => panic!("expected one ROOT folder, got {other:?}"),
    }
}

#[test]
fn the_fixture_reads_as_a_collection() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let collection = read(&snapshot_of(live.path(), backups.path(), |_| {}));
    assert_eq!(collection.tracks.len(), 40);
    let first = collection
        .tracks
        .iter()
        .find(|track| track.id == 10_000)
        .unwrap();
    assert_eq!(first.location, "/fixture/audio/track000.mp3");
    assert_eq!(first.location_bytes, b"/fixture/audio/track000.mp3");
    assert_eq!(
        first.attributes.get("Name").map(String::as_str),
        Some("Track 000")
    );
    assert_eq!(
        first.attributes.get("AverageBpm").map(String::as_str),
        Some("128.00")
    );
    assert_eq!(
        first.attributes.get("TotalTime").map(String::as_str),
        Some("300")
    );
    let playlists = root_children(&collection);
    assert_eq!(playlists.len(), 3);
    match &playlists[1] {
        crate::xml::PlaylistNode::Playlist {
            attributes,
            track_ids,
        } => {
            assert_eq!(
                attributes.get("Name").map(String::as_str),
                Some("Playlist 1")
            );
            assert_eq!(track_ids, &[10_005, 10_006, 10_007, 10_008, 10_009]);
        }
        other @ crate::xml::PlaylistNode::Folder { .. } => {
            panic!("expected a playlist, got {other:?}")
        }
    }
    assert_eq!(collection.unresolved_track_ids, Vec::<u64>::new());
}

#[test]
fn folders_nest_and_follow_their_order() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let snapshot = snapshot_of(live.path(), backups.path(), |connection| {
        connection
            .execute_batch(
                "INSERT INTO djmdPlaylist (ID, Seq, Name, Attribute, ParentID, rb_local_deleted, created_at, updated_at)
                 VALUES ('950000', 0, 'House', 1, 'root', 0, '', '');
                 UPDATE djmdPlaylist SET ParentID = '950000', Seq = 2 WHERE ID = '900000';
                 UPDATE djmdPlaylist SET ParentID = '950000', Seq = 1 WHERE ID = '900002';",
            )
            .unwrap();
    });
    let collection = read(&snapshot);
    let top = root_children(&collection);
    let names = |nodes: &[crate::xml::PlaylistNode]| -> Vec<String> {
        nodes
            .iter()
            .map(|node| match node {
                crate::xml::PlaylistNode::Folder { attributes, .. }
                | crate::xml::PlaylistNode::Playlist { attributes, .. } => {
                    attributes["Name"].clone()
                }
            })
            .collect()
    };
    assert_eq!(names(top), ["House", "Playlist 1"]);
    let crate::xml::PlaylistNode::Folder { children, .. } = &top[0] else {
        panic!("House is a folder")
    };
    assert_eq!(names(children), ["Playlist 2", "Playlist 0"]);
}

#[test]
fn each_playlist_and_folder_carries_its_rekordbox_id() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let snapshot = snapshot_of(live.path(), backups.path(), |connection| {
        connection
            .execute_batch(
                "INSERT INTO djmdPlaylist (ID, Seq, Name, Attribute, ParentID, rb_local_deleted, created_at, updated_at)
                 VALUES ('950000', 0, 'House', 1, 'root', 0, '', '');",
            )
            .unwrap();
    });
    let collection = read(&snapshot);
    let ids: Vec<(String, String)> = root_children(&collection)
        .iter()
        .map(|node| match node {
            crate::xml::PlaylistNode::Folder { attributes, .. }
            | crate::xml::PlaylistNode::Playlist { attributes, .. } => (
                attributes["Name"].clone(),
                attributes.get("RekordboxId").cloned().unwrap_or_default(),
            ),
        })
        .collect();
    assert!(ids.contains(&("House".into(), "950000".into())), "{ids:?}");
    assert!(
        ids.contains(&("Playlist 1".into(), fixture::playlist_id(1))),
        "{ids:?}"
    );
}

#[test]
fn deleted_rows_are_left_out() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let snapshot = snapshot_of(live.path(), backups.path(), |connection| {
        connection
            .execute_batch(
                "UPDATE djmdContent SET rb_local_deleted = 1 WHERE ID = '10005';
                 UPDATE djmdPlaylist SET rb_local_deleted = 1 WHERE ID = '900002';
                 UPDATE djmdSongPlaylist SET rb_local_deleted = 1
                   WHERE PlaylistID = '900001' AND ContentID = '10006';",
            )
            .unwrap();
    });
    let collection = read(&snapshot);
    assert_eq!(collection.tracks.len(), 39);
    let playlists = root_children(&collection);
    assert_eq!(playlists.len(), 2);
    let crate::xml::PlaylistNode::Playlist { track_ids, .. } = &playlists[1] else {
        panic!("a playlist")
    };
    // A deleted entry is gone; an entry for a deleted track stays as an unresolved reference.
    assert_eq!(track_ids, &[10_005, 10_007, 10_008, 10_009]);
    assert_eq!(collection.unresolved_track_ids, [10_005]);
}

#[test]
fn tags_rating_colour_and_names_are_read() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let snapshot = snapshot_of(live.path(), backups.path(), |connection| {
        connection
            .execute_batch(&format!(
                "INSERT INTO djmdArtist (ID, Name, rb_local_deleted, created_at, updated_at) VALUES ('1', 'Studio 001', 0, '', '');
                 INSERT INTO djmdAlbum (ID, Name, rb_local_deleted, created_at, updated_at) VALUES ('2', 'Night Drive EP', 0, '', '');
                 INSERT INTO djmdGenre (ID, Name, rb_local_deleted, created_at, updated_at) VALUES ('3', 'House', 0, '', '');
                 INSERT INTO djmdLabel (ID, Name, rb_local_deleted, created_at, updated_at) VALUES ('4', 'Example Records', 0, '', '');
                 INSERT INTO djmdKey (ID, ScaleName, rb_local_deleted, created_at, updated_at) VALUES ('5', '8A', 0, '', '');
                 INSERT INTO djmdColor (ID, ColorCode, SortKey, Commnt, rb_local_deleted, created_at, updated_at) VALUES ('6', 16711680, 1, 'Red', 0, '', '');
                 UPDATE djmdContent SET ArtistID = '1', AlbumID = '2', GenreID = '3', LabelID = '4', KeyID = '5',
                   ColorID = '6', Rating = 3, ReleaseYear = 2024, Commnt = 'Warm opener' WHERE ID = '10000';
                 INSERT INTO djmdSongMyTag (ID, MyTagID, ContentID, TrackNo, rb_local_deleted, created_at, updated_at)
                   VALUES ('a', '{warm}', '10000', 1, 0, '', ''), ('b', '{peak}', '10000', 2, 0, '', '');",
                warm = fixture::MY_TAG_WARM_UP,
                peak = fixture::MY_TAG_PEAK,
            ))
            .unwrap();
    });
    let collection = read(&snapshot);
    let track = collection
        .tracks
        .iter()
        .find(|track| track.id == 10_000)
        .unwrap();
    let get = |key: &str| track.attributes.get(key).map(String::as_str);
    assert_eq!(get("Artist"), Some("Studio 001"));
    assert_eq!(get("Album"), Some("Night Drive EP"));
    assert_eq!(get("Genre"), Some("House"));
    assert_eq!(get("Label"), Some("Example Records"));
    assert_eq!(get("Tonality"), Some("8A"));
    assert_eq!(get("Colour"), Some("Red"));
    assert_eq!(get("Rating"), Some("153"));
    assert_eq!(get("Year"), Some("2024"));
    assert_eq!(get("Comments"), Some("Warm opener"));
    assert_eq!(get("MyTags"), Some("Warm-up; Peak"));
    let other = collection
        .tracks
        .iter()
        .find(|track| track.id == 10_001)
        .unwrap();
    assert_eq!(
        other.attributes.get("Rating").map(String::as_str),
        Some("0")
    );
    assert_eq!(other.attributes.get("MyTags"), None);
}

#[test]
fn cues_are_read_in_seconds() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let snapshot = snapshot_of(live.path(), backups.path(), |connection| {
        connection
            .execute_batch(
                "INSERT INTO djmdCue (ID, ContentID, InMsec, OutMsec, Kind, Comment, rb_local_deleted, created_at, updated_at) VALUES
                   ('c1', '10000', 1500, -1, 0, 'Intro', 0, '', ''),
                   ('c2', '10000', 32000, -1, 1, '', 0, '', ''),
                   ('c3', '10000', 64000, 72000, 5, 'Drop loop', 0, '', ''),
                   ('c4', '10000', 90000, -1, 10, '', 0, '', ''),
                   ('c5', '10000', 99000, -1, 2, '', 1, '', '');",
            )
            .unwrap();
    });
    let collection = read(&snapshot);
    let track = collection
        .tracks
        .iter()
        .find(|track| track.id == 10_000)
        .unwrap();
    let cues: Vec<(u8, Option<i32>, f64, Option<f64>)> = track
        .cues
        .iter()
        .map(|cue| (cue.kind, cue.number, cue.start_seconds, cue.end_seconds))
        .collect();
    assert_eq!(
        cues,
        [
            (0, Some(-1), 1.5, None),
            (0, Some(0), 32.0, None),
            (4, Some(3), 64.0, Some(72.0)),
            (0, Some(8), 90.0, None),
        ]
    );
    assert_eq!(
        track.cues[0].attributes.get("Name").map(String::as_str),
        Some("Intro")
    );
}

fn fake_backup(backups: &Path, name: &str, taken_ms: i64, pinned: bool) -> PathBuf {
    let dir = backups.join(name);
    fs::create_dir_all(&dir).unwrap();
    let snapshot = Snapshot {
        master_db: dir.join("master.db"),
        dir: dir.clone(),
        taken_ms,
        reason: "read".into(),
        db_version: None,
        app_version: None,
        tracks: 0,
        playlists: 0,
        pinned,
    };
    fs::write(
        dir.join("snapshot.json"),
        serde_json::to_string(&snapshot).unwrap(),
    )
    .unwrap();
    dir
}

#[test]
fn list_is_newest_first() {
    let backups = tempfile::tempdir().unwrap();
    fake_backup(backups.path(), "a", 1, false);
    fake_backup(backups.path(), "c", 3, true);
    fake_backup(backups.path(), "b", 2, false);
    fs::create_dir(backups.path().join("not-a-backup")).unwrap();
    let times: Vec<i64> = list_backups(backups.path())
        .iter()
        .map(|b| b.taken_ms)
        .collect();
    assert_eq!(times, [3, 2, 1]);
    assert_eq!(list_backups(&backups.path().join("missing")), Vec::new());
}

#[test]
fn prune_keeps_twenty_and_every_pinned_backup() {
    let backups = tempfile::tempdir().unwrap();
    for i in 0..25 {
        fake_backup(backups.path(), &format!("b{i:02}"), 100 + i, false);
    }
    fake_backup(backups.path(), "old-pinned-1", 1, true);
    fake_backup(backups.path(), "old-pinned-2", 2, true);
    fs::create_dir(backups.path().join("not-a-backup")).unwrap();
    assert_eq!(prune(backups.path(), 20).unwrap(), 5);
    let left = list_backups(backups.path());
    assert_eq!(left.len(), 22);
    assert!(left.iter().filter(|b| !b.pinned).all(|b| b.taken_ms >= 105));
    assert!(
        backups.path().join("not-a-backup").is_dir(),
        "only backups are removed"
    );
}

#[test]
fn pinning_is_remembered() {
    let backups = tempfile::tempdir().unwrap();
    let dir = fake_backup(backups.path(), "a", 1, false);
    set_pinned(&dir, true).unwrap();
    assert!(list_backups(backups.path())[0].pinned);
    set_pinned(&dir, false).unwrap();
    assert!(!list_backups(backups.path())[0].pinned);
}

fn playlist_count(location: &LibraryLocation) -> u32 {
    Library::open(location.clone(), OpenMode::ReadOnly)
        .unwrap()
        .live_playlist_count()
        .unwrap()
}

#[test]
fn restore_brings_back_the_old_library() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let location = library(live.path());
    let before = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    {
        let mut open = Library::open(location.clone(), OpenMode::ReadWrite).unwrap();
        let connection = open.connection_mut();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection
            .execute(
                "DELETE FROM djmdPlaylist WHERE ID = ?1",
                [fixture::playlist_id(0)],
            )
            .unwrap();
    }
    assert_eq!(playlist_count(&location), 2);
    let saved = restore(&before.dir, &location, backups.path(), false, NOW + 1000).unwrap();
    assert_eq!(playlist_count(&location), 3);
    assert!(saved.pinned);
    assert_eq!(saved.reason, "before-restore");
    assert_eq!(
        saved.playlists, 2,
        "the state replaced by the restore is kept"
    );
    assert!(list_backups(backups.path())
        .iter()
        .any(|b| b.dir == saved.dir && b.pinned));
}

#[test]
fn restore_refuses_while_running() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let location = library(live.path());
    let before = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    let original = fs::read(&location.master_db).unwrap();
    assert!(restore(&before.dir, &location, backups.path(), true, NOW).is_err());
    assert_eq!(fs::read(&location.master_db).unwrap(), original);
    assert_eq!(
        list_backups(backups.path()).len(),
        1,
        "no extra backup either"
    );
}

#[test]
fn restore_refuses_a_broken_backup() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let location = library(live.path());
    let before = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    let bytes = fs::read(&before.master_db).unwrap();
    fs::write(&before.master_db, &bytes[..bytes.len() / 3]).unwrap();
    let original = fs::read(&location.master_db).unwrap();
    assert!(restore(&before.dir, &location, backups.path(), false, NOW).is_err());
    assert_eq!(fs::read(&location.master_db).unwrap(), original);
    let leftovers: Vec<_> = fs::read_dir(live.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("opencrate"))
        .collect();
    assert_eq!(leftovers, Vec::<String>::new());
}

fn nodes(tree: &[crate::xml::PlaylistNode]) -> (usize, usize, usize) {
    tree.iter()
        .fold((0, 0, 0), |(folders, lists, items), node| match node {
            crate::xml::PlaylistNode::Folder { children, .. } => {
                let (f, l, i) = nodes(children);
                (folders + 1 + f, lists + l, items + i)
            }
            crate::xml::PlaylistNode::Playlist { track_ids, .. } => {
                (folders, lists + 1, items + track_ids.len())
            }
        })
}

/// Reads the rekordbox library installed on this computer through a backup copy. Prints counts
/// only, never names. Run by hand with rekordbox closed:
/// `cargo test -p oc-rekordbox --release -- --ignored --nocapture live_installed_read`
#[test]
#[ignore = "reads the installed rekordbox library; run by hand"]
fn live_installed_read() {
    assert!(!rekordbox_running(), "quit rekordbox and its agent first");
    let location = installed_library().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    let snapshot = take_snapshot(&location, backups.path(), "read", false, 0).unwrap();
    let copied = started.elapsed();
    let collection = read_snapshot(&snapshot.master_db, &location.passphrase).unwrap();
    let total = started.elapsed();
    let [root] = collection.playlists.as_slice() else {
        panic!("one ROOT folder")
    };
    let crate::xml::PlaylistNode::Folder { children, .. } = root else {
        panic!("ROOT is a folder")
    };
    let (folders, lists, items) = nodes(children);
    let tagged = collection
        .tracks
        .iter()
        .filter(|track| track.attributes.contains_key("MyTags"))
        .count();
    let rated = collection
        .tracks
        .iter()
        .filter(|track| track.attributes.get("Rating").is_some_and(|r| r != "0"))
        .count();
    let cues: usize = collection.tracks.iter().map(|track| track.cues.len()).sum();
    let analysed = collection
        .tracks
        .iter()
        .filter(|track| {
            track
                .attributes
                .get("AverageBpm")
                .is_some_and(|b| b != "0.00")
        })
        .count();
    eprintln!(
        "rekordbox {:?}, DBVersion {:?}\n\
         tracks: rbl-db {} / read {}; playlist nodes: rbl-db {} / read {} ({folders} folders, {lists} playlists, {items} entries)\n\
         unresolved references {}; with My Tags {tagged}; rated {rated}; analysed {analysed}; cues {cues}\n\
         copy and check {copied:?}; total {total:?}",
        snapshot.app_version,
        snapshot.db_version,
        snapshot.tracks,
        collection.tracks.len(),
        snapshot.playlists,
        folders + lists,
        collection.unresolved_track_ids.len(),
    );
    assert_eq!(
        u32::try_from(collection.tracks.len()).unwrap(),
        snapshot.tracks
    );
}

#[test]
fn restore_works_when_the_current_library_is_damaged() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let location = library(live.path());
    let good = take_snapshot(&location, backups.path(), "read", false, NOW).unwrap();
    let bytes = fs::read(&location.master_db).unwrap();
    let damaged = &bytes[..bytes.len() / 3];
    fs::write(&location.master_db, damaged).unwrap();
    let saved = restore(&good.dir, &location, backups.path(), false, NOW + 1000).unwrap();
    assert_eq!(playlist_count(&location), 3);
    assert!(saved.pinned);
    assert_eq!(saved.reason, "before-restore-damaged");
    assert_eq!(
        fs::read(&saved.master_db).unwrap(),
        damaged,
        "the damaged library is kept as it was"
    );
}

#[test]
fn restoring_a_raw_backup_keeps_what_was_still_in_its_log() {
    let (live, backups) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let location = library(live.path());
    // A change that stays in the write-ahead log, as a crash would leave it.
    {
        let mut open = Library::open(location.clone(), OpenMode::ReadWrite).unwrap();
        let connection = open.connection_mut();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection
            .pragma_update(None, "wal_autocheckpoint", 0)
            .unwrap();
        connection
            .execute(
                "UPDATE djmdPlaylist SET Name = 'Only in the WAL' WHERE ID = ?1",
                [fixture::playlist_id(0)],
            )
            .unwrap();
        // Saved raw while the writer still holds the log open, as keep_raw does for a damaged library.
        let raw =
            snapshot::keep_raw(&location, backups.path(), "before-restore-damaged", NOW).unwrap();
        assert!(
            raw.dir.join("master.db-wal").is_file(),
            "the log was saved beside the copy"
        );
        drop(open);
        // Then the live library moves on, so the restore has something to undo.
        let mut open = Library::open(location.clone(), OpenMode::ReadWrite).unwrap();
        open.connection_mut()
            .execute(
                "UPDATE djmdPlaylist SET Name = 'Later' WHERE ID = ?1",
                [fixture::playlist_id(0)],
            )
            .unwrap();
        drop(open);
        restore(&raw.dir, &location, backups.path(), false, NOW + 1000).unwrap();
    }
    let name: String = Library::open(location, OpenMode::ReadOnly)
        .unwrap()
        .connection()
        .query_row(
            "SELECT Name FROM djmdPlaylist WHERE ID = ?1",
            [fixture::playlist_id(0)],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(name, "Only in the WAL");
}
