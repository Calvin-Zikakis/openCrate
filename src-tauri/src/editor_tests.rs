use std::fs;
use std::path::PathBuf;

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;

use crate::editor::{
    edit_each, edit_tracks, genres, rename_genres, track_tags, FieldChange, TrackEdit,
};

struct Setup {
    _data: tempfile::TempDir,
    _folder: tempfile::TempDir,
    service: LibraryService,
    files: Vec<PathBuf>,
}

fn setup(titles: &[&str]) -> Setup {
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
    for (index, title) in titles.iter().enumerate() {
        let staged = staging.join(format!("{index}.aif"));
        fs::write(&staged, aiff(2)).unwrap();
        write_tags(
            &staged,
            &TagInfo {
                artist: Some("Studio 001".into()),
                title: Some((*title).into()),
                album: Some("Between Spaces".into()),
                genre: Some("House".into()),
                track_number: Some(u32::try_from(index).unwrap() + 1),
                ..TagInfo::default()
            },
        );
        let destination = music.join(format!("Studio 001/Between Spaces/{index:02}.aif"));
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        service.publish_file(staged, destination.clone()).unwrap();
        files.push(destination);
    }
    service.scan_root(root, &ScanOptions::default()).unwrap();
    Setup {
        _data: data,
        _folder: folder,
        service,
        files,
    }
}

fn ids(service: &LibraryService) -> std::collections::HashMap<String, i64> {
    service
        .read_page(
            &oc_core_db::library::LibraryQuery::new(oc_core_db::library::View::Tracks),
            0,
            50,
        )
        .unwrap()
        .items
        .into_iter()
        .map(|item| (item.title, item.id))
        .collect()
}

#[test]
fn the_editor_reads_what_each_file_holds() {
    let setup = setup(&["Night Drive", "Soft Focus"]);
    let ids = ids(&setup.service);
    let found = track_tags(&setup.service, &[ids["Soft Focus"], ids["Night Drive"]]).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].track_id, ids["Soft Focus"]);
    assert_eq!(found[0].title.as_deref(), Some("Soft Focus"));
    assert_eq!(found[0].genre.as_deref(), Some("House"));
    assert_eq!(found[0].track, Some(2));
}

#[test]
fn an_edit_is_written_into_every_chosen_file_and_the_library_follows() {
    let setup = setup(&["Night Drive", "Soft Focus"]);
    let ids = ids(&setup.service);
    let both = [ids["Night Drive"], ids["Soft Focus"]];
    let done = edit_tracks(
        &setup.service,
        &both,
        &[
            FieldChange {
                field: "genre".into(),
                value: "Deep House".into(),
            },
            FieldChange {
                field: "label".into(),
                value: "Test Label".into(),
            },
        ],
    )
    .unwrap();
    assert_eq!((done.files, done.left.len()), (2, 0));
    for file in &setup.files {
        let tags = oc_tag::read_file(file).unwrap().tags;
        assert_eq!(tags.genre.as_deref(), Some("Deep House"));
        assert_eq!(tags.label.as_deref(), Some("Test Label"));
        assert_eq!(
            tags.artist.as_deref(),
            Some("Studio 001"),
            "fields not edited stay"
        );
    }
    edit_tracks(
        &setup.service,
        &[ids["Night Drive"]],
        &[FieldChange {
            field: "title".into(),
            value: "Night Drive (Extended)".into(),
        }],
    )
    .unwrap();
    let after = self::ids(&setup.service);
    assert_eq!(
        after.get("Night Drive (Extended)"),
        Some(&ids["Night Drive"]),
        "the same track, retitled"
    );
}

#[test]
fn a_bad_value_or_field_is_refused_before_any_file_changes() {
    let setup = setup(&["Night Drive"]);
    let ids = ids(&setup.service);
    let before = fs::read(&setup.files[0]).unwrap();
    for change in [
        FieldChange {
            field: "year".into(),
            value: "next year".into(),
        },
        FieldChange {
            field: "bpm".into(),
            value: "128".into(),
        },
    ] {
        assert!(edit_tracks(&setup.service, &[ids["Night Drive"]], &[change]).is_err());
    }
    assert_eq!(fs::read(&setup.files[0]).unwrap(), before);
}

#[test]
fn many_tracks_take_their_own_changes_in_one_pass() {
    let setup = setup(&["01 - Night Drive", "02 - Soft Focus"]);
    let ids = ids(&setup.service);
    let edit = |title: &str, value: &str| TrackEdit {
        track_id: ids[title],
        changes: vec![FieldChange {
            field: "title".into(),
            value: value.into(),
        }],
    };
    let done = edit_each(
        &setup.service,
        &[
            edit("01 - Night Drive", "Night Drive"),
            edit("02 - Soft Focus", "Soft Focus"),
        ],
    )
    .unwrap();
    assert_eq!(done.files, 2);
    let after = self::ids(&setup.service);
    assert!(
        after.contains_key("Night Drive") && after.contains_key("Soft Focus"),
        "{after:?}"
    );
}

#[test]
fn genres_spelled_alike_are_grouped_and_merged_into_one() {
    let setup = setup(&["Night Drive", "Soft Focus", "Late Train"]);
    let ids = ids(&setup.service);
    edit_tracks(
        &setup.service,
        &[ids["Soft Focus"]],
        &[FieldChange {
            field: "genre".into(),
            value: "house".into(),
        }],
    )
    .unwrap();
    edit_tracks(
        &setup.service,
        &[ids["Late Train"]],
        &[FieldChange {
            field: "genre".into(),
            value: "Deep-House".into(),
        }],
    )
    .unwrap();
    let found = genres(&setup.service).unwrap();
    assert_eq!(found.groups.len(), 1, "{:?}", found.groups);
    let group = &found.groups[0];
    assert_eq!(group.suggested, "House", "the commonest spelling");
    assert_eq!(group.members, ["House", "house"]);
    assert_eq!(group.tracks, 2);

    let done = rename_genres(
        &setup.service,
        &["house".to_owned(), "Deep-House".to_owned()],
        "House",
    )
    .unwrap();
    assert_eq!(done.files, 2);
    let found = genres(&setup.service).unwrap();
    assert_eq!(found.genres.len(), 1);
    assert_eq!(
        (found.genres[0].name.as_str(), found.genres[0].tracks),
        ("House", 3)
    );
}
