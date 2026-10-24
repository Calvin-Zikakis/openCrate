use std::fs;
use std::path::PathBuf;

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use rbl_db::fixture::{self, Shape};

use crate::library_fix::tidy_folder;
use crate::rekordbox_db::{self, Probe};

struct Setup {
    _data: tempfile::TempDir,
    _folder: tempfile::TempDir,
    _rekordbox: tempfile::TempDir,
    service: LibraryService,
    probe: Probe,
    music: PathBuf,
    root: i64,
}

/// A library with a file at each of `paths` (inside the music folder), and a synthetic rekordbox
/// whose first track is the first file when `rekordbox_has_first` is set.
fn setup(paths: &[&str], rekordbox_has_first: bool) -> Setup {
    let folder = tempfile::tempdir().unwrap();
    let music = folder.path().join("Music");
    let staging = folder.path().join("staging");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let music = music.canonicalize().unwrap();
    let data = tempfile::tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    for (index, relative) in paths.iter().enumerate() {
        let staged = staging.join(format!("{index}.aif"));
        fs::write(&staged, aiff(2)).unwrap();
        write_tags(
            &staged,
            &TagInfo {
                artist: Some("Studio 001".into()),
                title: Some(format!("Track {index}")),
                album: Some("Between Spaces".into()),
                ..TagInfo::default()
            },
        );
        let destination = music.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        service.publish_file(staged, destination).unwrap();
    }
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let rekordbox = tempfile::tempdir().unwrap();
    let location = fixture::build(rekordbox.path(), Shape::default()).unwrap();
    if rekordbox_has_first {
        fixture::point_at_audio(&location, 0, &music.join(paths[0]).to_string_lossy(), 2).unwrap();
    }
    let probe = Probe::new(Some(location), Some("7.2.19".into())).running_from(|| false);
    rekordbox_db::set_enabled(&service, &probe, true).unwrap();
    Setup {
        service,
        probe,
        music,
        root,
        _data: data,
        _folder: folder,
        _rekordbox: rekordbox,
    }
}

fn untidy(setup: &Setup) -> Vec<String> {
    setup
        .service
        .check_health()
        .unwrap()
        .untidy
        .into_iter()
        .map(|folder| folder.folder)
        .collect()
}

fn stored_path(setup: &Setup) -> String {
    let location = setup.probe.location.clone().unwrap();
    rbl_db::Library::open(location, rbl_db::OpenMode::ReadOnly)
        .unwrap()
        .connection()
        .query_row(
            "SELECT FolderPath FROM djmdContent WHERE ID = ?1",
            [fixture::track_id(0)],
            |row| row.get(0),
        )
        .unwrap()
}

const SOFT: &str = "Studio 001;/Between Spaces/01 - Soft Focus.aif";

#[test]
fn a_folder_rekordbox_uses_is_tidied_and_rekordbox_follows_the_files() {
    let setup = setup(&[SOFT], true);
    crate::rekordbox_write::set_enabled(&setup.service, &setup.probe, true).unwrap();
    assert_eq!(untidy(&setup), ["Studio 001;"]);
    let done = tidy_folder(&setup.service, &setup.probe, setup.root, "Studio 001;").unwrap();
    assert_eq!((done.moved, done.relocated), (1, 1));
    assert_eq!(done.left, Vec::<String>::new());
    let moved = setup
        .music
        .join("Studio 001/Between Spaces/01 - Soft Focus.aif");
    assert!(moved.is_file());
    assert!(
        !setup.music.join("Studio 001;").exists(),
        "the emptied folder is removed"
    );
    assert_eq!(stored_path(&setup), moved.to_string_lossy());
    assert_eq!(untidy(&setup), Vec::<String>::new());
}

#[test]
fn with_writing_off_a_folder_rekordbox_uses_is_left_alone() {
    let setup = setup(&[SOFT], true);
    let before = stored_path(&setup);
    let error = tidy_folder(&setup.service, &setup.probe, setup.root, "Studio 001;").unwrap_err();
    assert!(error.contains("Settings"), "{error}");
    assert!(setup.music.join(SOFT).is_file());
    assert_eq!(stored_path(&setup), before);
}

#[test]
fn a_folder_rekordbox_does_not_use_is_tidied_without_writing_to_it() {
    let setup = setup(&[SOFT], false);
    let done = tidy_folder(&setup.service, &setup.probe, setup.root, "Studio 001;").unwrap();
    assert_eq!((done.moved, done.relocated), (1, 0));
    assert!(setup
        .music
        .join("Studio 001/Between Spaces/01 - Soft Focus.aif")
        .is_file());
}

#[test]
fn a_file_whose_tidy_place_is_taken_stays_where_it_is() {
    let setup = setup(
        &[
            "Studio 001;Guest/Between Spaces/02 - Soft Focus.aif",
            "Studio 001, Guest/Between Spaces/02 - Soft Focus.aif",
            "Studio 001;Guest/Between Spaces/03 - Late Train.aif",
        ],
        false,
    );
    let done = tidy_folder(&setup.service, &setup.probe, setup.root, "Studio 001;Guest").unwrap();
    assert_eq!(done.moved, 1);
    assert_eq!(done.left.len(), 1);
    assert!(done.left[0].contains("02 - Soft Focus"), "{:?}", done.left);
    assert!(setup
        .music
        .join("Studio 001;Guest/Between Spaces/02 - Soft Focus.aif")
        .is_file());
    assert!(setup
        .music
        .join("Studio 001, Guest/Between Spaces/03 - Late Train.aif")
        .is_file());
}
