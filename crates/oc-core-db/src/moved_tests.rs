use std::fs;
use std::path::Path;

use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use tempfile::tempdir;

use crate::service::{LibraryService, ScanOptions};

fn put(path: &Path, title: &str, seconds: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, aiff(seconds)).unwrap();
    write_tags(
        path,
        &TagInfo {
            artist: Some("Studio 001".into()),
            title: Some(title.into()),
            album: Some("Between Spaces".into()),
            ..TagInfo::default()
        },
    );
}

fn track_named(service: &LibraryService, title: &str) -> Vec<i64> {
    service
        .read_page(
            &crate::library::LibraryQuery::new(crate::library::View::Tracks),
            0,
            50,
        )
        .unwrap()
        .items
        .into_iter()
        .filter(|item| item.title == title)
        .map(|item| item.id)
        .collect()
}

#[test]
fn a_file_renamed_outside_opencrate_is_relinked_and_its_track_keeps_its_crates_and_fields() {
    let music = tempdir().unwrap();
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let folder = music.path().join("Studio 001/Between Spaces");
    put(&folder.join("01 Night Drive.aif"), "Night Drive", 2);
    put(&folder.join("02 Soft Focus.aif"), "Soft Focus", 3);
    let root = service.add_root(music.path(), "Music").unwrap();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let [night] = track_named(&service, "Night Drive")[..] else {
        panic!("one Night Drive")
    };
    let crate_id = service.create_crate("Warm-up").unwrap();
    service.add_to_crate(crate_id, vec![night]).unwrap();
    service.set_custom_field(&[night], "Energy", "7").unwrap();
    assert_eq!(service.moved_files().unwrap(), []);

    // Renamed in the Finder, and retitled: the scan sees a missing file and a new track.
    fs::rename(
        folder.join("01 Night Drive.aif"),
        folder.join("Night Drive (renamed).aif"),
    )
    .unwrap();
    put(&folder.join("02 Soft Focus.aif"), "Soft Focus", 3);
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let found = service.moved_files().unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].track_id, found[0].same_file), (night, true));
    let newcomer = service
        .track_of_file(found[0].found_file_id)
        .unwrap()
        .unwrap();
    service.add_to_crate(crate_id, vec![newcomer]).unwrap();
    service
        .set_custom_field(&[newcomer], "Mood", "dark")
        .unwrap();

    service
        .relink(found[0].missing_file_id, found[0].found_file_id)
        .unwrap();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    assert_eq!(
        track_named(&service, "Night Drive"),
        [night],
        "the old track, not a new one"
    );
    assert_eq!(
        service.track_of_file(found[0].missing_file_id).unwrap(),
        Some(night)
    );
    assert!(service
        .track_file(night)
        .unwrap()
        .unwrap()
        .ends_with("Night Drive (renamed).aif"));
    assert_eq!(
        service.crate_track_ids(crate_id).unwrap(),
        [night],
        "one place, kept"
    );
    let mut fields = service.custom_fields(&[night]).unwrap();
    fields.sort();
    assert_eq!(
        fields,
        [
            (night, "Energy".to_owned(), "7".to_owned()),
            (night, "Mood".to_owned(), "dark".to_owned())
        ]
    );
    assert_eq!(service.moved_files().unwrap(), []);
    assert_eq!(service.missing_files().unwrap(), []);
    // Asked again, it is refused: the file is no longer missing.
    assert!(service
        .relink(found[0].missing_file_id, found[0].found_file_id)
        .is_err());
}

#[test]
fn a_copy_that_was_there_before_or_two_equal_candidates_are_never_guessed() {
    let music = tempdir().unwrap();
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    put(&music.path().join("A/Late Train.aif"), "Late Train", 2);
    // An older copy of the same size and name, elsewhere: a duplicate, not the moved file.
    put(&music.path().join("Old/Late Train.aif"), "Late Train", 2);
    let root = service.add_root(music.path(), "Music").unwrap();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    fs::remove_file(music.path().join("A/Late Train.aif")).unwrap();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    assert_eq!(service.missing_files().unwrap().len(), 1);
    assert!(
        service.moved_files().unwrap().is_empty(),
        "the older copy is not offered"
    );

    // Two new files of that name and size: either could be it, so neither is offered.
    put(&music.path().join("B/Late Train.aif"), "Late Train", 2);
    put(&music.path().join("C/Late Train.aif"), "Late Train", 2);
    service.scan_root(root, &ScanOptions::default()).unwrap();
    assert_eq!(service.moved_files().unwrap(), []);
    // Chosen by the owner, one of them is relinked all the same.
    let (missing, _) = service.missing_files().unwrap()[0];
    let chosen = service
        .file_at(root, &[b"B".to_vec(), b"Late Train.aif".to_vec()])
        .unwrap()
        .unwrap();
    service.relink(missing, chosen).unwrap();
    assert_eq!(service.missing_files().unwrap(), []);
    assert_eq!(track_named(&service, "Late Train").len(), 3);
}
