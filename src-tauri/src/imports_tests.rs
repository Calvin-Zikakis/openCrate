use std::fs;

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;

use crate::imports::{release_titles, ReleaseTitleDto};

/// One present file per release, two releases, one of them by a different artist.
#[test]
fn release_titles_are_listed_with_their_artists() {
    let folder = tempfile::tempdir().unwrap();
    let music = folder.path().join("Music");
    let staging = folder.path().join("staging");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let music = oc_paths::canonical(&music).unwrap();
    let data = tempfile::tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    for (n, (artist, album, title)) in [
        ("Studio 001", "Set", "One"),
        ("Night Drive", "Between Spaces", "Two"),
    ]
    .into_iter()
    .enumerate()
    {
        let staged = staging.join(format!("{n}.aif"));
        fs::write(&staged, aiff(1)).unwrap();
        write_tags(
            &staged,
            &TagInfo {
                artist: Some(artist.into()),
                title: Some(title.into()),
                album: Some(album.into()),
                ..TagInfo::default()
            },
        );
        let destination = music.join(format!("{artist}/{album}/{title}.aif"));
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        service.publish_file(staged, destination).unwrap();
    }
    service.scan_root(root, &ScanOptions::default()).unwrap();

    let titles = release_titles(&service).unwrap();
    assert!(
        titles.contains(&ReleaseTitleDto {
            artist: "Studio 001".into(),
            title: "Set".into()
        }),
        "{titles:?}"
    );
    assert!(
        titles.contains(&ReleaseTitleDto {
            artist: "Night Drive".into(),
            title: "Between Spaces".into()
        }),
        "{titles:?}"
    );
    assert_eq!(titles.len(), 2, "{titles:?}");
}
