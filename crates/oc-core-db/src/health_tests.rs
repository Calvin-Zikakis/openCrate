use std::fs;
use std::path::Path;

use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use tempfile::tempdir;

use crate::service::{LibraryService, ScanOptions};

fn put(root: &Path, relative: &str, artist: &str, title: &str, seconds: u32) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, aiff(seconds)).unwrap();
    write_tags(
        &path,
        &TagInfo {
            artist: Some(artist.into()),
            title: Some(title.into()),
            album: Some("Between Spaces".into()),
            ..TagInfo::default()
        },
    );
}

#[test]
fn untidy_folders_and_the_same_recording_twice_are_reported_without_changing_anything() {
    let music = tempdir().unwrap();
    let root = music.path();
    put(
        root,
        "Studio 001;/Between Spaces/01 - Night Drive.aif",
        "Studio 001",
        "Night Drive",
        2,
    );
    put(
        root,
        "Studio 001;Guest/Between Spaces/02 - Soft Focus.aif",
        "Studio 001",
        "Soft Focus",
        3,
    );
    put(
        root,
        "Studio 001/Between Spaces/02 - Soft Focus.aif",
        "Studio 001",
        "Soft Focus",
        3,
    );
    // Same name, very different length: an extended mix, not a copy.
    put(
        root,
        "Studio 001/Extended/Soft Focus.aif",
        "Studio 001",
        "Soft Focus",
        9,
    );
    put(
        root,
        "Clean/Between Spaces/03 - Late Train.aif",
        "Clean",
        "Late Train",
        2,
    );
    let before: Vec<_> = walk(root);

    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let id = service.add_root(root, "Music").unwrap();
    service.scan_root(id, &ScanOptions::default()).unwrap();
    let health = service.check_health().unwrap();

    assert_eq!(health.files, 5);
    let untidy: Vec<_> = health
        .untidy
        .iter()
        .map(|folder| {
            (
                folder.folder.as_str(),
                folder.suggested.as_str(),
                folder.files,
            )
        })
        .collect();
    assert_eq!(
        untidy,
        [
            ("Studio 001;", "Studio 001", 1),
            ("Studio 001;Guest", "Studio 001, Guest", 1),
        ]
    );
    assert_eq!(health.duplicates.len(), 1, "{:?}", health.duplicates);
    assert_eq!(
        health.duplicates[0].files,
        [
            "Studio 001/Between Spaces/02 - Soft Focus.aif",
            "Studio 001;Guest/Between Spaces/02 - Soft Focus.aif"
        ]
    );
    assert_eq!(walk(root), before, "nothing on disk changed");
}

fn walk(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push((
                path.to_string_lossy().into_owned(),
                fs::read(&path).unwrap(),
            ));
        }
    }
    found.sort();
    found
}

#[test]
fn a_track_plays_from_its_lossless_file_and_a_missing_one_gives_nothing() {
    let music = tempdir().unwrap();
    put(
        music.path(),
        "A/R/01 - Night Drive.aif",
        "Studio 001",
        "Night Drive",
        1,
    );
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let id = service.add_root(music.path(), "Music").unwrap();
    service.scan_root(id, &ScanOptions::default()).unwrap();
    let page = service
        .read_page(
            &crate::library::LibraryQuery::new(crate::library::View::Tracks),
            0,
            10,
        )
        .unwrap();
    let track = page.items[0].id;
    let file = service.track_file(track).unwrap().unwrap();
    assert!(
        file.ends_with("A/R/01 - Night Drive.aif"),
        "{}",
        file.display()
    );
    assert_eq!(service.track_file(999_999).unwrap(), None);
    fs::remove_file(&file).unwrap();
    service.scan_root(id, &ScanOptions::default()).unwrap();
    assert_eq!(service.track_file(track).unwrap(), None, "gone from disk");
}

#[test]
fn a_waveform_is_computed_once_and_again_only_when_the_file_or_its_format_changes() {
    let music = tempdir().unwrap();
    put(
        music.path(),
        "A/R/01 - Night Drive.aif",
        "Studio 001",
        "Night Drive",
        1,
    );
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let id = service.add_root(music.path(), "Music").unwrap();
    service.scan_root(id, &ScanOptions::default()).unwrap();
    let track = service
        .read_page(
            &crate::library::LibraryQuery::new(crate::library::View::Tracks),
            0,
            10,
        )
        .unwrap()
        .items[0]
        .id;
    let computed = std::cell::Cell::new(0);
    let compute = |_: &Path| {
        computed.set(computed.get() + 1);
        Some(vec![1, 2, 3])
    };
    assert_eq!(
        service.waveform(track, 1, compute).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(
        service.waveform(track, 1, compute).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(computed.get(), 1, "stored after the first time");
    // The file changes: a scan notices, and the waveform is made again.
    put(
        music.path(),
        "A/R/01 - Night Drive.aif",
        "Studio 001",
        "Night Drive",
        2,
    );
    service.scan_root(id, &ScanOptions::default()).unwrap();
    service.waveform(track, 1, compute).unwrap();
    assert_eq!(computed.get(), 2);
    assert_eq!(service.waveform(999_999, 1, compute).unwrap(), None);
    assert_eq!(
        service.waveform(track, 1, |_| None).unwrap(),
        Some(vec![1, 2, 3]),
        "still stored"
    );
    // A waveform stored in an older format is made again in the asked one, even at the same
    // length.
    let bands = |_: &Path| Some(vec![9, 9, 9]);
    assert_eq!(
        service.waveform(track, 2, bands).unwrap(),
        Some(vec![9, 9, 9])
    );
    assert_eq!(
        service.waveform(track, 2, |_| None).unwrap(),
        Some(vec![9, 9, 9])
    );
    assert_eq!(
        service.waveform(track, 1, |_| None).unwrap(),
        None,
        "format 1 is gone"
    );
}
