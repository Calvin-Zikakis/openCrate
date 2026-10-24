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

/// The id of the newest save, as the page would offer it.
fn newest(service: &LibraryService) -> i64 {
    crate::editor::edit_history(service)
        .unwrap()
        .first()
        .map_or(0, |batch| batch.id)
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
        FieldChange {
            field: "year".into(),
            value: "0".into(),
        },
        FieldChange {
            field: "track".into(),
            value: "+12".into(),
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

/// An undo that cannot put a file back keeps the save, so it can be tried again.
#[cfg(unix)]
#[test]
fn an_undo_that_fails_keeps_the_save() {
    use std::os::unix::fs::PermissionsExt;

    use crate::editor::{edit_history, undo_last};
    let setup = setup(&["Night Drive"]);
    let ids = ids(&setup.service);
    edit_tracks(
        &setup.service,
        &[ids["Night Drive"]],
        &[FieldChange {
            field: "title".into(),
            value: "Night Drive (Extended)".into(),
        }],
    )
    .unwrap();
    // A file is rewritten through a copy beside it; a locked folder refuses the copy.
    let folder = setup.files[0].parent().unwrap().to_path_buf();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    let undone = undo_last(&setup.service, newest(&setup.service)).unwrap();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!((undone.files, undone.left.len()), (0, 1));
    assert_eq!(
        edit_history(&setup.service).unwrap().len(),
        1,
        "still there"
    );

    let undone = undo_last(&setup.service, newest(&setup.service)).unwrap();
    assert_eq!((undone.files, undone.left.len()), (1, 0));
    assert_eq!(edit_history(&setup.service).unwrap().len(), 0);
    assert_eq!(
        oc_tag::read_file(&setup.files[0])
            .unwrap()
            .tags
            .title
            .as_deref(),
        Some("Night Drive")
    );
}

#[test]
fn every_edit_is_remembered_and_the_last_one_can_be_undone() {
    use crate::editor::{edit_history, undo_last};
    let setup = setup(&["Night Drive", "Soft Focus"]);
    let ids = ids(&setup.service);
    assert_eq!(edit_history(&setup.service).unwrap().len(), 0);
    edit_tracks(
        &setup.service,
        &[ids["Night Drive"], ids["Soft Focus"]],
        &[FieldChange {
            field: "genre".into(),
            value: "Deep House".into(),
        }],
    )
    .unwrap();
    edit_tracks(
        &setup.service,
        &[ids["Night Drive"]],
        &[FieldChange {
            field: "title".into(),
            value: "Night Drive (Extended)".into(),
        }],
    )
    .unwrap();
    let history = edit_history(&setup.service).unwrap();
    assert_eq!(history.len(), 2, "one entry per save, newest first");
    assert_eq!(
        (history[0].tracks, history[0].fields.as_slice()),
        (1, &["title".to_owned()][..])
    );
    assert_eq!(
        (history[1].tracks, history[1].fields.as_slice()),
        (2, &["genre".to_owned()][..])
    );

    let undone = undo_last(&setup.service, newest(&setup.service)).unwrap();
    assert_eq!(undone.files, 1);
    let tags = oc_tag::read_file(&setup.files[0]).unwrap().tags;
    assert_eq!(
        tags.title.as_deref(),
        Some("Night Drive"),
        "the title is back"
    );
    assert_eq!(
        tags.genre.as_deref(),
        Some("Deep House"),
        "the earlier edit stays"
    );
    assert_eq!(edit_history(&setup.service).unwrap().len(), 1);

    let undone = undo_last(&setup.service, newest(&setup.service)).unwrap();
    assert_eq!(undone.files, 2);
    for file in &setup.files {
        assert_eq!(
            oc_tag::read_file(file).unwrap().tags.genre.as_deref(),
            Some("House"),
            "the genre is back"
        );
    }
    assert_eq!(edit_history(&setup.service).unwrap().len(), 0);
    assert!(
        undo_last(&setup.service, 1).is_err(),
        "nothing left to undo"
    );
}

fn retitle(setup: &Setup, title: &str, value: &str) {
    let ids = ids(&setup.service);
    edit_tracks(
        &setup.service,
        &[ids[title]],
        &[FieldChange {
            field: "title".into(),
            value: value.into(),
        }],
    )
    .unwrap();
}

/// A field changed again after a save (by a rekordbox sync, say) keeps its newer value.
#[test]
fn an_undo_keeps_a_value_changed_after_the_save() {
    use crate::editor::{edit_history, undo_last};
    let setup = setup(&["Night Drive"]);
    let ids = ids(&setup.service);
    edit_tracks(
        &setup.service,
        &[ids["Night Drive"]],
        &[
            FieldChange {
                field: "title".into(),
                value: "Night Drive (Extended)".into(),
            },
            FieldChange {
                field: "track".into(),
                value: "07".into(),
            },
        ],
    )
    .unwrap();
    oc_tag::rewrite_tags(
        &setup.files[0],
        &[(oc_tag::TextTag::Title, "Night Drive (Sync)".into())],
    )
    .unwrap();
    let undone = undo_last(&setup.service, newest(&setup.service)).unwrap();
    assert_eq!(undone.files, 1, "the track number still goes back");
    assert_eq!(undone.left.len(), 1, "{:?}", undone.left);
    assert!(
        undone.left[0].contains("title was changed after that save (now \"Night Drive (Sync)\")"),
        "{:?}",
        undone.left
    );
    let tags = oc_tag::read_file(&setup.files[0]).unwrap().tags;
    assert_eq!(
        (tags.title.as_deref(), tags.track_number),
        (Some("Night Drive (Sync)"), Some(1))
    );
    assert!(
        edit_history(&setup.service).unwrap().is_empty(),
        "nothing is left to try again"
    );
}

/// The page names the save it offers; a newer one is not undone in its place.
#[test]
fn an_undo_of_a_save_that_is_no_longer_the_newest_is_refused() {
    use crate::editor::{edit_history, forget_edit, undo_last};
    let setup = setup(&["Night Drive"]);
    retitle(&setup, "Night Drive", "One");
    let offered = newest(&setup.service);
    retitle(&setup, "One", "Two");
    let refused = undo_last(&setup.service, offered).unwrap_err();
    assert!(refused.contains("nothing was undone"), "{refused}");
    let title = || {
        oc_tag::read_file(&setup.files[0])
            .unwrap()
            .tags
            .title
            .unwrap_or_default()
    };
    assert_eq!(title(), "Two");

    // A save given up on goes, and the one before it can be undone.
    forget_edit(&setup.service, newest(&setup.service)).unwrap();
    assert_eq!(edit_history(&setup.service).unwrap().len(), 1);
    oc_tag::rewrite_tags(&setup.files[0], &[(oc_tag::TextTag::Title, "One".into())]).unwrap();
    undo_last(&setup.service, offered).unwrap();
    assert_eq!(title(), "Night Drive");
}
