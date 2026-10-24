use std::fs;
use std::path::PathBuf;

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::{embedded_cover, Cover, TagInfo};

use crate::art_fix::{releases_without_art, remove_release_art, set_release_art, ArtSource};
use crate::artwork::ArtCache;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

struct Setup {
    _data: tempfile::TempDir,
    _folder: tempfile::TempDir,
    service: LibraryService,
    music: PathBuf,
    files: Vec<PathBuf>,
}

/// Two releases of two files each; "Between Spaces" has a cover.png beside it, "No Light" has
/// nothing.
fn setup() -> Setup {
    let folder = tempfile::tempdir().unwrap();
    let music = folder.path().join("Music");
    let staging = folder.path().join("staging");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let music = oc_paths::canonical(&music).unwrap();
    let data = tempfile::tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    let mut files = Vec::new();
    for (album, title) in [
        ("Between Spaces", "Night Drive"),
        ("Between Spaces", "Soft Focus"),
        ("No Light", "Late Train"),
        ("No Light", "Dawn"),
    ] {
        let staged = staging.join(format!("{}.aif", files.len()));
        fs::write(&staged, aiff(2)).unwrap();
        write_tags(
            &staged,
            &TagInfo {
                artist: Some("Studio 001".into()),
                title: Some(title.into()),
                album: Some(album.into()),
                ..TagInfo::default()
            },
        );
        let destination = music.join(format!("Studio 001/{album}/{title}.aif"));
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        service.publish_file(staged, destination.clone()).unwrap();
        files.push(destination);
    }
    fs::write(music.join("Studio 001/Between Spaces/cover.png"), PNG).unwrap();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    Setup {
        _data: data,
        _folder: folder,
        service,
        music,
        files,
    }
}

fn release_named(setup: &Setup, title: &str) -> i64 {
    setup
        .service
        .releases_all()
        .unwrap()
        .into_iter()
        .find(|(_, _, found)| found == title)
        .unwrap()
        .0
}

#[test]
fn releases_whose_files_carry_no_picture_are_listed_with_what_is_beside_them() {
    let setup = setup();
    let found = releases_without_art(&setup.service).unwrap();
    let names: Vec<(&str, bool)> = found
        .iter()
        .map(|release| (release.title.as_str(), release.folder_image))
        .collect();
    assert_eq!(names, [("Between Spaces", true), ("No Light", false)]);
    assert_eq!(found[0].files, 2);
}

#[test]
fn the_folder_image_goes_into_every_file_and_the_release_leaves_the_list() {
    let setup = setup();
    let cache = ArtCache::default();
    let release = release_named(&setup, "Between Spaces");
    let done = set_release_art(&setup.service, &cache, release, &ArtSource::Folder).unwrap();
    assert_eq!((done.files, done.left.len()), (2, 0));
    for file in &setup.files[..2] {
        assert_eq!(embedded_cover(file).unwrap().mime, "image/png");
    }
    assert_eq!(
        embedded_cover(&setup.files[2]),
        None,
        "the other release is untouched"
    );
    let titles: Vec<String> = releases_without_art(&setup.service)
        .unwrap()
        .into_iter()
        .map(|release| release.title)
        .collect();
    assert_eq!(titles, ["No Light"]);
}

#[test]
fn a_chosen_image_is_embedded_and_can_be_removed_again() {
    let setup = setup();
    let cache = ArtCache::default();
    let release = release_named(&setup, "No Light");
    let error = set_release_art(&setup.service, &cache, release, &ArtSource::Folder).unwrap_err();
    assert!(error.contains("no image"), "{error}");
    let picture = setup.music.join("chosen.jpg");
    fs::write(
        &picture,
        [
            0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 1,
        ],
    )
    .unwrap();
    set_release_art(&setup.service, &cache, release, &ArtSource::File(picture)).unwrap();
    assert_eq!(embedded_cover(&setup.files[3]).unwrap().mime, "image/jpeg");
    let bytes = Cover {
        mime: "image/png",
        data: PNG.to_vec(),
    };
    set_release_art(&setup.service, &cache, release, &ArtSource::Bytes(bytes)).unwrap();
    assert_eq!(
        embedded_cover(&setup.files[3]).unwrap().mime,
        "image/png",
        "replaced"
    );
    let done = remove_release_art(&setup.service, &cache, release).unwrap();
    assert_eq!(done.files, 2);
    assert_eq!(embedded_cover(&setup.files[2]), None);
    let not_an_image = setup.music.join("notes.txt");
    fs::write(&not_an_image, b"hello").unwrap();
    assert!(set_release_art(
        &setup.service,
        &cache,
        release,
        &ArtSource::File(not_an_image)
    )
    .is_err());
}
