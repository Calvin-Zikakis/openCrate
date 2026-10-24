use std::fs;
use std::path::Path;

use oc_rekordbox::xml;
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use tempfile::{tempdir, TempDir};

use crate::merge::{classify, Direction, Field};
use crate::service::{LibraryService, ScanOptions};

#[test]
fn a_change_on_one_side_flows_to_the_other_and_both_is_a_conflict() {
    use Direction::{Conflict, ToOpenCrate, ToRekordbox};
    // In step: nothing to do, whatever the base says.
    assert_eq!(classify(Some("A"), "B", "B"), None);
    assert_eq!(classify(None, "A", "A"), None);
    // Changed only in rekordbox, only in openCrate, or in both to different values.
    assert_eq!(classify(Some("A"), "A", "B"), Some(ToOpenCrate));
    assert_eq!(classify(Some("A"), "B", "A"), Some(ToRekordbox));
    assert_eq!(classify(Some("A"), "B", "C"), Some(Conflict));
    // The first sync takes rekordbox as the starting point; openCrate fills only what it lacks.
    assert_eq!(classify(None, "A", "B"), Some(ToOpenCrate));
    assert_eq!(classify(None, "A", ""), Some(ToRekordbox));
    assert_eq!(classify(None, "", "B"), Some(ToOpenCrate));
}

struct Library {
    _data: TempDir,
    _folder: TempDir,
    service: LibraryService,
    night: std::path::PathBuf,
}

/// A library with one filed track, and a direct read of rekordbox that has it as track 777
/// with `title`, `year` and `label`.
fn library() -> Library {
    let folder = tempdir().unwrap();
    let music = folder.path().join("Music");
    let staging = folder.path().join("staging");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let music = music.canonicalize().unwrap();
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    let staged = staging.join("night.aif");
    fs::write(&staged, aiff(2)).unwrap();
    write_tags(
        &staged,
        &TagInfo {
            title: Some("Night Drive".into()),
            artist: Some("Studio 001".into()),
            album: Some("Between Spaces".into()),
            ..TagInfo::default()
        },
    );
    let night = music.join("Studio 001/Between Spaces/01 - Night Drive.aif");
    fs::create_dir_all(night.parent().unwrap()).unwrap();
    service.publish_file(staged, night.clone()).unwrap();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    Library {
        _data: data,
        _folder: folder,
        service,
        night,
    }
}

fn rekordbox_has(library: &Library, title: &str, year: &str, label: &str) {
    let mut attributes = xml::Attributes::from([
        ("Name".to_owned(), title.to_owned()),
        ("Artist".to_owned(), "Studio 001".to_owned()),
        ("Album".to_owned(), "Between Spaces".to_owned()),
    ]);
    if !year.is_empty() {
        attributes.insert("Year".into(), year.into());
    }
    if !label.is_empty() {
        attributes.insert("Label".into(), label.into());
    }
    let location = library.night.to_string_lossy().into_owned();
    let collection = xml::Collection {
        attributes: xml::Attributes::new(),
        product: None,
        collection_attributes: xml::Attributes::new(),
        tracks: vec![xml::Track {
            id: 777,
            location_bytes: location.clone().into_bytes(),
            location,
            attributes,
            cues: Vec::new(),
            tempos: Vec::new(),
        }],
        playlists: Vec::new(),
        unresolved_track_ids: Vec::new(),
        ignored_elements: Vec::new(),
    };
    library
        .service
        .import_database(Path::new("/rekordbox/master.db"), collection)
        .unwrap();
}

fn found(library: &Library) -> Vec<(Field, String, String, Option<String>, Direction)> {
    library
        .service
        .merge_review()
        .unwrap()
        .into_iter()
        .map(|change| {
            (
                change.field,
                change.ours,
                change.theirs,
                change.base,
                change.direction,
            )
        })
        .collect()
}

#[test]
fn the_first_review_takes_rekordbox_as_the_starting_point() {
    let library = library();
    rekordbox_has(&library, "Night Drive (Original Mix)", "2019", "");
    let changes = library.service.merge_review().unwrap();
    assert_eq!(changes.len(), 2);
    assert!(changes
        .iter()
        .all(|change| change.rb_key == 777 && change.title == "Night Drive"));
    assert_eq!(
        found(&library),
        [
            (
                Field::Title,
                "Night Drive".into(),
                "Night Drive (Original Mix)".into(),
                None,
                Direction::ToOpenCrate
            ),
            (
                Field::Year,
                String::new(),
                "2019".into(),
                None,
                Direction::ToOpenCrate
            ),
        ]
    );
}

#[test]
fn after_a_sync_only_what_changed_since_is_offered() {
    let library = library();
    rekordbox_has(&library, "Night Drive", "", "");
    // Everything agrees: nothing to offer, and that agreement becomes the base.
    assert_eq!(found(&library), []);

    // rekordbox's title changes; openCrate's does not: it flows to openCrate.
    rekordbox_has(&library, "Night Drive (Extended)", "", "");
    assert_eq!(
        found(&library),
        [(
            Field::Title,
            "Night Drive".into(),
            "Night Drive (Extended)".into(),
            Some("Night Drive".into()),
            Direction::ToOpenCrate
        )]
    );

    // The owner keeps openCrate's, recorded as the new base: now it flows to rekordbox.
    library
        .service
        .record_merge_base(vec![(777, Field::Title, "Night Drive (Club)".into())])
        .unwrap();
    let changes = found(&library);
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].4,
        Direction::Conflict,
        "both now differ from the base"
    );
}

#[test]
fn a_label_added_in_rekordbox_flows_to_opencrate() {
    let library = library();
    rekordbox_has(&library, "Night Drive", "", "");
    assert_eq!(found(&library), []);
    // rekordbox later gains a label while openCrate has none: that is a rekordbox change.
    rekordbox_has(&library, "Night Drive", "", "Test Label");
    assert_eq!(
        found(&library),
        [(
            Field::Label,
            String::new(),
            "Test Label".into(),
            Some(String::new()),
            Direction::ToOpenCrate
        )]
    );
}
