//! The whole scan path on a real temporary folder: probe, register, check, walk, apply.
//! Nothing here touches a real library; every folder is created and removed by the test.

use std::fs;
use std::path::{Path, PathBuf};

use oc_paths::{check_new_root, probe_root, Availability, FileId, NameProfile, WalkOptions};
use tempfile::{tempdir, TempDir};

use crate::roots::{check_root, list_roots, NewRoot, ProfileSource, RootKind};
use crate::scan::{walk_stored_root, ScanOutcome, ScanPass};
use crate::Database;

struct Setup {
    _database_directory: TempDir,
    library: TempDir,
    root_path: PathBuf,
    database: Database,
    root_id: i64,
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn setup(files: &[&str]) -> Setup {
    let library = tempdir().unwrap();
    for file in files {
        write(&library.path().join(file), b"audio");
    }
    let root_path = check_new_root(library.path(), &[], &[]).unwrap();
    let profile = probe_root(&root_path, 1_000).unwrap().profile();
    let database_directory = tempdir().unwrap();
    let mut database = Database::open(database_directory.path()).unwrap();
    let root_id = database
        .add_root(
            &NewRoot {
                path: &root_path,
                label: "Test library",
                kind: RootKind::Local,
                profile,
                profile_source: ProfileSource::StatProbe,
                dir_token: FileId::of(&root_path).unwrap().persistent_token(),
                volume_token: None,
            },
            1,
        )
        .unwrap();
    Setup {
        _database_directory: database_directory,
        library,
        root_path,
        database,
        root_id,
    }
}

/// What the application does for one root: check it, walk it away from the writer, then
/// apply the pass. An unavailable or unverified root is recorded and applies nothing.
fn scan(setup: &mut Setup, now: i64) -> ScanOutcome {
    let root = list_roots(&setup.database.connection)
        .unwrap()
        .into_iter()
        .find(|root| root.id == setup.root_id)
        .unwrap();
    let (availability, reason) = check_root(&root);
    setup
        .database
        .set_availability(root.id, availability, &reason, now)
        .unwrap();
    let pass = if availability == Availability::Available {
        match walk_stored_root(&root.path_raw, &WalkOptions::default(), false) {
            Ok(pass) => pass,
            Err(error) => {
                setup
                    .database
                    .set_availability(root.id, Availability::Unavailable, &error.to_string(), now)
                    .unwrap();
                empty_pass(Availability::Unavailable)
            }
        }
    } else {
        empty_pass(availability)
    };
    setup.database.apply_scan(root.id, &pass, now).unwrap()
}

fn empty_pass(root_availability: Availability) -> ScanPass {
    ScanPass {
        root_availability,
        files: Vec::new(),
        existing_dirs: Vec::new(),
        enumerated_dirs: Vec::new(),
        confirm_mass_missing: false,
    }
}

fn presence_of(setup: &Setup, relative: &[&str]) -> Option<(i64, String)> {
    let raw = oc_paths::RelPath::from_components(
        relative.iter().map(|part| part.as_bytes()),
        oc_paths::Flavor::HOST,
    )
    .unwrap();
    setup
        .database
        .connection
        .query_row(
            "SELECT id, presence FROM file WHERE path_raw = ?1",
            [raw.as_stored()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()
}

fn count(setup: &Setup, presence: &str) -> i64 {
    setup
        .database
        .connection
        .query_row(
            "SELECT COUNT(*) FROM file WHERE presence = ?1",
            [presence],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn a_scan_of_a_real_folder_follows_what_happens_on_disk() {
    let mut setup = setup(&["A/one.aif", "A/two.flac", "B/three.mp3", "A/cover.jpg"]);
    let first = scan(&mut setup, 100);
    assert_eq!((first.inserted, first.marked_missing), (3, 0));
    assert_eq!(count(&setup, "present"), 3);
    let again = scan(&mut setup, 150);
    assert_eq!((again.unchanged, again.inserted, again.updated), (3, 0, 0));

    let (two_id, _) = presence_of(&setup, &["A", "two.flac"]).unwrap();
    fs::remove_file(setup.root_path.join("A/two.flac")).unwrap();
    assert_eq!(scan(&mut setup, 200).marked_missing, 1);
    assert_eq!(
        presence_of(&setup, &["A", "two.flac"]).unwrap().1,
        "missing"
    );

    write(&setup.root_path.join("A/two.flac"), b"audio");
    assert_eq!(scan(&mut setup, 300).revived, 1);
    assert_eq!(
        presence_of(&setup, &["A", "two.flac"]),
        Some((two_id, "present".to_owned()))
    );

    write(
        &setup.root_path.join("A/one.aif"),
        b"a longer body than before",
    );
    assert_eq!(scan(&mut setup, 400).updated, 1);

    fs::remove_dir_all(setup.root_path.join("B")).unwrap();
    assert_eq!(
        scan(&mut setup, 500).marked_missing,
        1,
        "a whole folder went away"
    );
    assert_eq!(
        presence_of(&setup, &["B", "three.mp3"]).unwrap().1,
        "missing"
    );
    assert_eq!(count(&setup, "present"), 2);
    let rows: i64 = setup
        .database
        .connection
        .query_row("SELECT COUNT(*) FROM file", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 3, "no row is ever deleted");
    drop(setup.library);
}

#[test]
fn a_case_only_rename_follows_the_volume_the_probe_found() {
    let mut setup = setup(&["Artist/Track.aif", "Artist/other.mp3"]);
    scan(&mut setup, 100);
    let profile = list_roots(&setup.database.connection).unwrap()[0].profile;
    let (id, _) = presence_of(&setup, &["Artist", "Track.aif"]).unwrap();
    fs::rename(
        setup.root_path.join("Artist/Track.aif"),
        setup.root_path.join("Artist/TRACK.aif"),
    )
    .unwrap();
    let outcome = scan(&mut setup, 200);
    if profile == NameProfile::Exact {
        assert_eq!(
            (outcome.inserted, outcome.marked_missing),
            (1, 1),
            "a new name"
        );
        assert_eq!(
            presence_of(&setup, &["Artist", "Track.aif"]).unwrap().1,
            "missing"
        );
    } else {
        assert_eq!(
            (outcome.respelled, outcome.inserted, outcome.marked_missing),
            (1, 0, 0)
        );
        assert_eq!(
            presence_of(&setup, &["Artist", "TRACK.aif"]).unwrap().0,
            id,
            "same row"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_read_keeps_its_files_as_they_were() {
    use std::os::unix::fs::PermissionsExt;
    let mut setup = setup(&["Open/a.aif", "Locked/b.aif", "Locked/c.mp3"]);
    scan(&mut setup, 100);
    let locked = setup.root_path.join("Locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read_dir(&locked).is_ok();
    fs::remove_file(setup.root_path.join("Open/a.aif")).unwrap();
    let outcome = scan(&mut setup, 200);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if readable_anyway {
        return; // running as a user for whom permissions do not apply
    }
    assert_eq!(outcome.marked_missing, 1, "only the listed folder's file");
    assert_eq!(
        presence_of(&setup, &["Open", "a.aif"]).unwrap().1,
        "missing"
    );
    assert_eq!(
        presence_of(&setup, &["Locked", "b.aif"]).unwrap().1,
        "present"
    );
    assert_eq!(
        presence_of(&setup, &["Locked", "c.mp3"]).unwrap().1,
        "present"
    );
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_library_is_neither_indexed_nor_followed() {
    use std::os::unix::fs::symlink;
    let outside = tempdir().unwrap();
    write(&outside.path().join("secret.mp3"), b"x");
    let mut setup = setup(&["Real/in.aif"]);
    symlink(outside.path(), setup.root_path.join("Escape")).unwrap();
    symlink(
        outside.path().join("secret.mp3"),
        setup.root_path.join("Real/alias.mp3"),
    )
    .unwrap();
    let outcome = scan(&mut setup, 100);
    assert_eq!(outcome.inserted, 1);
    assert!(presence_of(&setup, &["Escape", "secret.mp3"]).is_none());
    assert!(presence_of(&setup, &["Real", "alias.mp3"]).is_none());
}

#[test]
fn a_vanished_root_marks_nothing_missing() {
    let mut setup = setup(&["A/one.aif", "A/two.aif", "B/three.mp3"]);
    scan(&mut setup, 100);
    fs::remove_dir_all(&setup.root_path).unwrap();
    let outcome = scan(&mut setup, 200);
    assert!(outcome.skipped);
    let root = &list_roots(&setup.database.connection).unwrap()[0];
    assert_eq!(root.availability, Availability::Unavailable);
    assert_eq!(
        root.last_available_ms,
        Some(100),
        "the last time it was seen"
    );
    assert_eq!(count(&setup, "present"), 3);
    assert_eq!(count(&setup, "missing"), 0);
    // It comes back as the same folder: the library resumes where it was.
    fs::create_dir_all(&setup.root_path).unwrap();
}

#[cfg(unix)]
#[test]
fn a_different_empty_folder_at_the_same_path_is_unverified_not_a_mass_deletion() {
    let mut setup = setup(&["A/one.aif", "A/two.aif", "B/three.mp3"]);
    scan(&mut setup, 100);
    // Replace the library by an empty folder of the same name, as an empty mount point would.
    fs::remove_dir_all(&setup.root_path).unwrap();
    fs::create_dir(&setup.root_path).unwrap();
    let outcome = scan(&mut setup, 200);
    assert!(outcome.skipped);
    let root = &list_roots(&setup.database.connection).unwrap()[0];
    assert_eq!(root.availability, Availability::Unverified);
    assert_eq!(count(&setup, "present"), 3);
}

#[test]
fn an_empty_but_verified_library_is_held_not_emptied() {
    let files: Vec<String> = (0..30).map(|n| format!("Lib/{n:02}.aif")).collect();
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    let mut setup = setup(&names);
    scan(&mut setup, 100);
    for file in &files {
        fs::remove_file(setup.root_path.join(file)).unwrap();
    }
    // The folder is the registered one and lists fine, but all 30 files vanished at once.
    let outcome = scan(&mut setup, 200);
    assert_eq!((outcome.marked_missing, outcome.held_missing), (0, 30));
    assert_eq!(count(&setup, "present"), 30);
}
