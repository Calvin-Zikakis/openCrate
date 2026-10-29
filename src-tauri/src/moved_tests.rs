use std::fs;
use std::path::PathBuf;

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use rbl_db::fixture::{self, Shape};

use crate::moved::{file_at_path, moved_files, relink};
use crate::rekordbox_db::{self, Probe};

struct Setup {
    _data: tempfile::TempDir,
    folder: tempfile::TempDir,
    _rekordbox: tempfile::TempDir,
    service: LibraryService,
    probe: Probe,
    music: PathBuf,
    root: i64,
}

const NIGHT: &str = "Studio 001/Between Spaces/01 - Night Drive.aif";

/// A library with Night Drive and Soft Focus; rekordbox's first track is Night Drive.
fn setup() -> Setup {
    let folder = tempfile::tempdir().unwrap();
    let music = folder.path().join("Music");
    fs::create_dir_all(&music).unwrap();
    let music = oc_paths::canonical(&music).unwrap();
    let data = tempfile::tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    for (relative, title) in [
        (NIGHT, "Night Drive"),
        (
            "Studio 001/Between Spaces/02 - Soft Focus.aif",
            "Soft Focus",
        ),
    ] {
        let path = music.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, aiff(2)).unwrap();
        write_tags(
            &path,
            &TagInfo {
                artist: Some("Studio 001".into()),
                title: Some(title.into()),
                album: Some("Between Spaces".into()),
                ..TagInfo::default()
            },
        );
    }
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let rekordbox = tempfile::tempdir().unwrap();
    let location = fixture::build(rekordbox.path(), Shape::default()).unwrap();
    let stored = music.join(NIGHT).to_string_lossy().replace('\\', "/");
    fixture::point_at_audio(&location, 0, &stored, 2).unwrap();
    let probe = Probe::new(Some(location), Some("7.2.19".into())).running_from(|| false);
    rekordbox_db::set_enabled(&service, &probe, true).unwrap();
    Setup {
        _data: data,
        folder,
        _rekordbox: rekordbox,
        service,
        probe,
        music,
        root,
    }
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

#[test]
fn a_file_moved_in_the_finder_is_found_and_relinked_and_rekordbox_follows_it() {
    let setup = setup();
    rekordbox_db::read(&setup.service, &setup.probe).unwrap();
    let crate_id = setup.service.create_crate("Warm-up").unwrap();
    let night = setup
        .service
        .file_at(
            setup.root,
            &NIGHT
                .split('/')
                .map(|part| part.as_bytes().to_vec())
                .collect::<Vec<_>>(),
        )
        .unwrap()
        .unwrap();
    let track = setup.service.track_of_file(night).unwrap().unwrap();
    setup.service.add_to_crate(crate_id, vec![track]).unwrap();

    let moved_to = setup.music.join("Sorted/Night Drive.aif");
    fs::create_dir_all(moved_to.parent().unwrap()).unwrap();
    fs::rename(setup.music.join(NIGHT), &moved_to).unwrap();
    setup
        .service
        .scan_root(setup.root, &ScanOptions::default())
        .unwrap();
    let listed = moved_files(&setup.service).unwrap();
    assert_eq!(listed.moved.len(), 1, "{listed:?}");
    assert_eq!(listed.unmatched, []);
    let item = &listed.moved[0];
    assert_eq!(
        (item.missing.title.as_str(), item.missing.path.as_str()),
        (
            "Night Drive",
            "Music: Studio 001/Between Spaces/01 - Night Drive.aif"
        )
    );
    assert_eq!(item.found.path, "Music: Sorted/Night Drive.aif");
    assert!(
        item.same_file,
        "renamed and moved, but the same file on disk"
    );
    let pair = [(item.missing.file_id, item.found.file_id)];

    // rekordbox uses the file and writing is off: nothing is relinked, and it says why.
    let before = stored_path(&setup);
    let done = relink(&setup.service, &setup.probe, &pair).unwrap();
    assert_eq!((done.relinked, done.relocated), (0, 0));
    assert!(
        done.left[0].contains("rekordbox uses this file"),
        "{:?}",
        done.left
    );
    assert_eq!(stored_path(&setup), before);

    crate::rekordbox_write::set_enabled(&setup.service, &setup.probe, true).unwrap();
    let done = relink(&setup.service, &setup.probe, &pair).unwrap();
    assert_eq!((done.relinked, done.relocated), (1, 1), "{:?}", done.left);
    assert_eq!(
        stored_path(&setup),
        moved_to.to_string_lossy().replace('\\', "/")
    );
    assert_eq!(setup.service.track_of_file(night).unwrap(), Some(track));
    assert_eq!(setup.service.crate_track_ids(crate_id).unwrap(), [track]);
    assert_eq!(
        setup.service.file_location(night).unwrap().unwrap().1,
        moved_to
    );
    let listed = moved_files(&setup.service).unwrap();
    assert_eq!((listed.moved.len(), listed.unmatched.len()), (0, 0));
}

#[test]
fn a_chosen_file_must_be_in_a_music_folder_and_is_read_in_when_new() {
    let setup = setup();
    let outside = setup.folder.path().join("elsewhere.aif");
    fs::write(&outside, aiff(1)).unwrap();
    let error = file_at_path(&setup.service, &outside).unwrap_err();
    assert!(
        error.contains("not in one of your music folders"),
        "{error}"
    );
    let inside = setup.music.join("New/Fresh.aif");
    fs::create_dir_all(inside.parent().unwrap()).unwrap();
    fs::write(&inside, aiff(1)).unwrap();
    let file = file_at_path(&setup.service, &inside).unwrap();
    assert_eq!(
        setup.service.file_location(file).unwrap().unwrap().1,
        inside
    );
}

#[test]
fn a_missing_file_found_in_another_folder_is_copied_back_into_place() {
    let setup = setup();
    // Keep a copy somewhere else (a backup drive, say), then lose the library's file.
    let backup = tempfile::tempdir().unwrap();
    let kept = backup.path().join("Old library/01 - Night Drive.aif");
    fs::create_dir_all(kept.parent().unwrap()).unwrap();
    fs::copy(setup.music.join(NIGHT), &kept).unwrap();
    // A file of the same name but another size is the weaker find.
    let other = backup.path().join("Singles/01 - Night Drive.aif");
    fs::create_dir_all(other.parent().unwrap()).unwrap();
    fs::write(&other, aiff(1)).unwrap();
    fs::remove_file(setup.music.join(NIGHT)).unwrap();
    setup
        .service
        .scan_root(setup.root, &ScanOptions::default())
        .unwrap();
    assert_eq!(moved_files(&setup.service).unwrap().unmatched.len(), 1);

    let search = crate::moved::find_in_folder(&setup.service, backup.path()).unwrap();
    assert_eq!(
        (search.searched, search.not_found, search.found.len()),
        (2, 0, 1)
    );
    let found = &search.found[0];
    assert_eq!(found.missing.title, "Night Drive");
    assert_eq!(
        PathBuf::from(&found.path),
        kept,
        "the copy of the same size is offered"
    );
    assert!(found.same_size);
    assert_eq!(
        found.others, 0,
        "the other size does not count as another of the same"
    );

    let done = crate::moved::restore_from_folder(
        &setup.service,
        &[(found.missing.file_id, PathBuf::from(&found.path))],
    )
    .unwrap();
    assert_eq!(
        done,
        crate::moved::RestoredDto {
            restored: 1,
            left: vec![]
        }
    );
    assert_eq!(
        fs::read(setup.music.join(NIGHT)).unwrap(),
        fs::read(&kept).unwrap(),
        "back where it was, byte for byte"
    );
    assert!(kept.exists(), "the found copy stays");
    let after = moved_files(&setup.service).unwrap();
    assert!(
        after.moved.is_empty() && after.unmatched.is_empty(),
        "{after:?}"
    );
    // Nothing is left to find: a second look says so, and a copy over a file back in place is refused.
    assert_eq!(
        crate::moved::find_in_folder(&setup.service, backup.path())
            .unwrap()
            .found
            .len(),
        0
    );
    let again = crate::moved::restore_from_folder(&setup.service, &[(found.missing.file_id, kept)])
        .unwrap();
    assert_eq!(again.restored, 0);
    assert!(
        again.left[0].contains("no longer in the library")
            || again.left[0].contains("back at its place"),
        "{:?}",
        again.left
    );
}
