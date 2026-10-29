//! The genre, year and date-added filters, the label list, and finding a file like one about
//! to be imported.

use std::fs;

use tempfile::tempdir;

use crate::library::{LibraryQuery, Sort, View};
use crate::service::{LibraryService, ScanOptions};

fn write(path: &std::path::Path, title: &str, genre: &str, label: &str, year: i32, seconds: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, oc_tag::fixtures::aiff(seconds)).unwrap();
    oc_tag::fixtures::write_tags(
        path,
        &oc_tag::TagInfo {
            title: Some(title.into()),
            artist: Some("Studio 001".into()),
            album: Some("Set".into()),
            genre: Some(genre.into()),
            label: Some(label.into()),
            year: Some(year),
            ..oc_tag::TagInfo::default()
        },
    );
}

#[test]
fn tracks_filter_by_genre_year_and_date_added_and_labels_are_listed() {
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let music = tempdir().unwrap();
    write(
        &music.path().join("a.aif"),
        "One",
        "Techno",
        "Label A",
        2019,
        1,
    );
    write(
        &music.path().join("b.aif"),
        "Two",
        "House",
        "Label A",
        2021,
        2,
    );
    write(
        &music.path().join("c.aif"),
        "Three",
        "Techno",
        "Label B",
        2024,
        3,
    );
    let root = service.add_root(music.path(), "Music").unwrap();
    let before = crate::now_ms();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let after = crate::now_ms();

    let titles = |query: &LibraryQuery| -> Vec<String> {
        service
            .read_page(query, 0, 10)
            .unwrap()
            .items
            .into_iter()
            .map(|item| item.title)
            .collect()
    };
    let mut query = LibraryQuery::new(View::Tracks);
    query.sort = Sort::Title;
    query.genre = Some("Techno".into());
    assert_eq!(titles(&query), ["One", "Three"]);
    query.genre = Some("techno".into());
    assert_eq!(titles(&query), Vec::<String>::new(), "as spelled");
    query.genre = None;
    query.years = Some((2020, 2024));
    assert_eq!(titles(&query), ["Three", "Two"]);
    query.years = None;
    query.added = Some((before, after));
    assert_eq!(titles(&query).len(), 3);
    query.added = Some((after + 60_000, after + 120_000));
    assert_eq!(titles(&query), Vec::<String>::new());
    query.added = None;

    // Releases and artists follow their tracks.
    let mut releases = LibraryQuery::new(View::Releases);
    releases.genre = Some("House".into());
    assert_eq!(service.read_page(&releases, 0, 10).unwrap().total, 1);
    releases.genre = Some("Ambient".into());
    assert_eq!(service.read_page(&releases, 0, 10).unwrap().total, 0);
    let mut artists = LibraryQuery::new(View::Artists);
    artists.years = Some((2019, 2019));
    assert_eq!(service.read_page(&artists, 0, 10).unwrap().total, 1);

    // Rows carry the tags and when they were added.
    let page = service
        .read_page(&LibraryQuery::new(View::Tracks), 0, 10)
        .unwrap();
    let three = page
        .items
        .iter()
        .find(|item| item.title == "Three")
        .unwrap();
    assert_eq!(
        (three.genre.as_str(), three.label.as_str(), three.year),
        ("Techno", "Label B", Some(2024))
    );
    assert!(three.added_ms >= before && three.added_ms <= after);

    assert_eq!(
        service.labels().unwrap(),
        [("Label A".to_owned(), 2), ("Label B".to_owned(), 1)]
    );

    // A file of the same size and length as one in the library is found; another length is not.
    let size = i64::try_from(fs::metadata(music.path().join("b.aif")).unwrap().len()).unwrap();
    let like = service.files_like(size, 2_000).unwrap();
    assert_eq!(like.len(), 1);
    assert_eq!(
        (
            like[0].title.as_str(),
            like[0].folder.as_str(),
            like[0].path.as_str()
        ),
        ("Two", "Music", "b.aif")
    );
    assert_eq!(service.files_like(size, 9_000).unwrap(), Vec::new());
}
