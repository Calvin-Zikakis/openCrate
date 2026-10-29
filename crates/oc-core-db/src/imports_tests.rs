//! Import batches: files filed close together share one batch, a gap starts another, and the
//! tracks behind a batch appear once the folder has been scanned.

use std::fs;

use tempfile::tempdir;

use crate::imports::BATCH_GAP_MS;
use crate::library::{LibraryQuery, Sort, View};
use crate::service::{LibraryService, ScanOptions};

fn components(path: &str) -> Vec<Vec<u8>> {
    path.split('/')
        .map(|part| part.as_bytes().to_vec())
        .collect()
}

#[test]
fn imports_close_together_share_a_batch_and_a_gap_starts_a_new_one() {
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let music = tempdir().unwrap();
    let root = service.add_root(music.path(), "Music").unwrap();
    let first = service
        .record_import(root, vec![components("A/one.aif"), components("A/two.aif")])
        .unwrap();
    assert_eq!(first.file_ids.len(), 2);
    let second = service
        .record_import(root, vec![components("B/three.aif")])
        .unwrap();
    assert_eq!(
        second.batch_id, first.batch_id,
        "a minute apart is one batch"
    );
    let batches = service.import_batches().unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        (batches[0].files, batches[0].tracks),
        (3, 0),
        "not scanned yet"
    );

    // An import after a long quiet spell starts a batch of its own.
    let later = crate::now_ms() + BATCH_GAP_MS + 1;
    let third = service
        .record_import_at(root, vec![components("C/four.aif")], later)
        .unwrap();
    assert_ne!(third.batch_id, first.batch_id);
    let batches = service.import_batches().unwrap();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].id, third.batch_id, "newest first");
}

#[test]
fn a_scanned_batch_lists_its_tracks_and_the_query_filters_by_it() {
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let music = tempdir().unwrap();
    for (name, title) in [
        ("A/one.aif", "One"),
        ("A/two.aif", "Two"),
        ("B/three.aif", "Three"),
    ] {
        let path = music.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, oc_tag::fixtures::aiff(1)).unwrap();
        oc_tag::fixtures::write_tags(
            &path,
            &oc_tag::TagInfo {
                title: Some(title.into()),
                artist: Some("Studio 001".into()),
                album: Some("Set".into()),
                ..oc_tag::TagInfo::default()
            },
        );
    }
    let root = service.add_root(music.path(), "Music").unwrap();
    let recorded = service
        .record_import(root, vec![components("A/one.aif"), components("A/two.aif")])
        .unwrap();
    assert_eq!(
        service.imported_tracks(&recorded.file_ids).unwrap(),
        Vec::<i64>::new(),
        "nothing until the scan"
    );
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let tracks = service.imported_tracks(&recorded.file_ids).unwrap();
    assert_eq!(tracks.len(), 2);
    let batches = service.import_batches().unwrap();
    assert_eq!((batches[0].files, batches[0].tracks), (2, 2));

    let mut query = LibraryQuery::new(View::Tracks);
    query.batch_id = Some(recorded.batch_id);
    query.sort = Sort::Title;
    let page = service.read_page(&query, 0, 10).unwrap();
    let titles: Vec<&str> = page.items.iter().map(|item| item.title.as_str()).collect();
    assert_eq!(
        titles,
        ["One", "Two"],
        "the third file was not imported by openCrate"
    );
    assert_eq!(
        service.formats().unwrap(),
        [("AIFF".to_owned(), 3)],
        "every container in the library, with its tracks"
    );
}
