use std::fs;
use std::path::Path;

use rbl_db::fixture::{self, Shape};
use rbl_db::{Library, LibraryLocation, OpenMode};

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
fn folder_names_use_the_utc_date() {
    // 2026-10-23 01:20:00 UTC
    assert_eq!(folder_stamp(1_792_718_400_000), "20261023-012000");
    assert_eq!(folder_stamp(0), "19700101-000000");
}
