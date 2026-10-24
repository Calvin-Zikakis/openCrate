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

fn track_named(setup: &Setup, title: &str) -> i64 {
    setup
        .service
        .read_page(
            &oc_core_db::library::LibraryQuery::new(oc_core_db::library::View::Tracks),
            0,
            50,
        )
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.title == title)
        .unwrap()
        .id
}

#[test]
fn a_cover_goes_into_the_chosen_tracks_only_and_their_state_follows() {
    use crate::art_fix::{set_tracks_art, track_covers, TrackArt};
    let setup = setup();
    let cache = ArtCache::default();
    let (night, soft, late) = (
        track_named(&setup, "Night Drive"),
        track_named(&setup, "Soft Focus"),
        track_named(&setup, "Late Train"),
    );
    let covers = track_covers(&setup.service, &[night, late]).unwrap();
    assert_eq!((covers.state, covers.folder_image), ("none", true));

    let picked = Cover {
        mime: "image/png",
        data: PNG.to_vec(),
    };
    let done = set_tracks_art(&setup.service, &cache, &[late], &TrackArt::Picked(&picked)).unwrap();
    assert_eq!((done.files, done.left.len()), (1, 0));
    assert_eq!(embedded_cover(&setup.files[2]).unwrap().data, PNG);
    assert_eq!(
        embedded_cover(&setup.files[3]),
        None,
        "the other track of the release is left"
    );
    assert_eq!(track_covers(&setup.service, &[late]).unwrap().state, "same");
    assert_eq!(
        track_covers(&setup.service, &[late, night]).unwrap().state,
        "mixed"
    );

    // From the image beside the files: a track with none there is left, and says so.
    let done = set_tracks_art(&setup.service, &cache, &[night, late], &TrackArt::Folder).unwrap();
    assert_eq!(done.files, 1);
    assert!(
        done.left[0].contains("Late Train.aif: there is no image beside it"),
        "{:?}",
        done.left
    );
    assert_eq!(embedded_cover(&setup.files[0]).unwrap().data, PNG);
    assert_eq!(embedded_cover(&setup.files[1]), None);
    assert_eq!(
        track_covers(&setup.service, &[night, late]).unwrap().state,
        "same"
    );

    let done = set_tracks_art(&setup.service, &cache, &[night, soft], &TrackArt::Remove).unwrap();
    assert_eq!(done.files, 2);
    assert_eq!(embedded_cover(&setup.files[0]), None);
    assert_eq!(
        embedded_cover(&setup.files[2]).unwrap().data,
        PNG,
        "not chosen, not touched"
    );
}

#[test]
fn the_editor_sees_a_track_picture_and_a_picked_one_never_kept() {
    use tauri::http::{header, StatusCode};
    let setup = setup();
    let cache = ArtCache::default();
    let (night, late) = (
        track_named(&setup, "Night Drive"),
        track_named(&setup, "Late Train"),
    );
    let service = Some(&setup.service);
    // A track's own picture: the image beside it when its file has none; none at all is a 404.
    let served = crate::artwork::respond(service, &cache, &format!("/track-{night}"));
    assert_eq!(
        (served.status(), served.body().as_slice()),
        (StatusCode::OK, PNG)
    );
    assert_eq!(served.headers()[header::CACHE_CONTROL], "private, no-cache");
    let path = format!("/track-{late}");
    assert_eq!(
        crate::artwork::respond(service, &cache, &path).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        crate::artwork::respond(service, &cache, "/picked").status(),
        StatusCode::NOT_FOUND
    );
    // A cover saved from the editor shows at once, though the miss was remembered.
    let jpeg = Cover {
        mime: "image/jpeg",
        data: vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 1,
        ],
    };
    crate::art_fix::set_tracks_art(
        &setup.service,
        &cache,
        &[late],
        &crate::art_fix::TrackArt::Picked(&jpeg),
    )
    .unwrap();
    let served = crate::artwork::respond(service, &cache, &path);
    assert_eq!(
        (served.status(), served.body()),
        (StatusCode::OK, &jpeg.data)
    );
    // The other track of that release keeps having none.
    let dawn = track_named(&setup, "Dawn");
    assert_eq!(
        crate::artwork::respond(service, &cache, &format!("/track-{dawn}")).status(),
        StatusCode::NOT_FOUND
    );
    cache.pick(Cover {
        mime: "image/png",
        data: PNG.to_vec(),
    });
    let served = crate::artwork::respond(service, &cache, "/picked");
    assert_eq!(served.body(), PNG);
    assert_eq!(served.headers()[header::CACHE_CONTROL], "no-store");
    for bad in ["/track-", "/track-x", "/track-1/2", "/picked/1", "/track/1"] {
        assert_eq!(
            crate::artwork::respond(service, &cache, bad).status(),
            StatusCode::NOT_FOUND,
            "{bad}"
        );
    }
}
