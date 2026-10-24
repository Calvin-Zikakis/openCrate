use std::fs;
use std::path::PathBuf;

use oc_core_db::library::{LibraryQuery, View};
use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;

use crate::export::{export_query, render_csv, render_m3u8, ExportFormat, Row};

fn row(title: &str, path: &str) -> Row {
    Row {
        title: title.into(),
        artist: "Studio 001".into(),
        release: "Between \"Spaces\", Vol. 2".into(),
        bpm: Some(124.5),
        key: "8A".into(),
        duration_ms: 225_400,
        format: Some("AIFF".into()),
        path: Some(PathBuf::from(path)),
    }
}

#[test]
fn an_m3u8_names_each_track_and_skips_those_without_a_file() {
    let rows = [
        row("Night Drive", "/Music/A/01.aif"),
        Row {
            path: None,
            ..row("Gone", "")
        },
    ];
    let text = render_m3u8(&rows);
    assert_eq!(
        text,
        "#EXTM3U\n#EXTINF:225,Studio 001 - Night Drive\n/Music/A/01.aif\n"
    );
}

#[test]
fn a_csv_quotes_what_needs_it_and_keeps_every_track() {
    let rows = [
        row("Night Drive", "/Music/A/01.aif"),
        Row {
            path: None,
            bpm: None,
            ..row("Gone", "")
        },
    ];
    let text = render_csv(&rows);
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines[0], "Title,Artist,Release,BPM,Key,Length,Format,Path");
    assert_eq!(
        lines[1],
        "Night Drive,Studio 001,\"Between \"\"Spaces\"\", Vol. 2\",124.5,8A,3:45,AIFF,/Music/A/01.aif"
    );
    assert_eq!(
        lines[2],
        "Gone,Studio 001,\"Between \"\"Spaces\"\", Vol. 2\",,8A,3:45,AIFF,"
    );
    assert!(
        text.starts_with('\u{feff}'),
        "a byte order mark, so spreadsheets read UTF-8"
    );
}

#[test]
fn a_whole_list_is_exported_in_its_order_to_the_chosen_file() {
    let folder = tempfile::tempdir().unwrap();
    let music = folder.path().join("Music");
    let staging = folder.path().join("staging");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let music = oc_paths::canonical(&music).unwrap();
    let data = tempfile::tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    for (index, title) in ["Night Drive", "Soft Focus", "Late Train"]
        .iter()
        .enumerate()
    {
        let staged = staging.join(format!("{index}.aif"));
        fs::write(&staged, aiff(2)).unwrap();
        write_tags(
            &staged,
            &TagInfo {
                title: Some((*title).into()),
                artist: Some("Studio 001".into()),
                album: Some("Between Spaces".into()),
                track_number: Some(u32::try_from(index).unwrap() + 1),
                ..TagInfo::default()
            },
        );
        let destination = music.join(format!("{index:02}.aif"));
        service.publish_file(staged, destination).unwrap();
    }
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let query = LibraryQuery {
        sort: oc_core_db::library::Sort::Title,
        ..LibraryQuery::new(View::Tracks)
    };
    let target = folder.path().join("out/list.m3u8");
    let done = export_query(&service, &query, ExportFormat::M3u8, &target).unwrap();
    assert_eq!((done.tracks, done.without_file), (3, 0));
    let text = fs::read_to_string(&target).unwrap();
    let names: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("#EXTINF"))
        .collect();
    assert_eq!(
        names,
        [
            "#EXTINF:2,Studio 001 - Late Train",
            "#EXTINF:2,Studio 001 - Night Drive",
            "#EXTINF:2,Studio 001 - Soft Focus"
        ]
    );
    assert!(
        text.contains(&music.join("02.aif").to_string_lossy().replace('\\', "/"))
            || text.contains(&music.join("02.aif").to_string_lossy().into_owned())
    );
    let csv = folder.path().join("list.csv");
    let done = export_query(&service, &query, ExportFormat::Csv, &csv).unwrap();
    assert_eq!(done.tracks, 3);
    assert_eq!(fs::read_to_string(&csv).unwrap().lines().count(), 4);
}
