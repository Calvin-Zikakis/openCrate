use std::fs;
use std::path::{Path, PathBuf};

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use rbl_db::fixture::{self, Shape};

use crate::rekordbox_db::{self, Probe};
use crate::rekordbox_merge::{self, ChoiceDto, Keep};
use crate::rekordbox_write::{add_to_playlist, send_new, set_enabled, IMPORTED};

/// A path as rekordbox stores it: forward slashes, on Windows too.
fn rekordbox_form(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

struct Setup {
    _data: tempfile::TempDir,
    _folder: tempfile::TempDir,
    _rekordbox: tempfile::TempDir,
    service: LibraryService,
    probe: Probe,
    files: Vec<PathBuf>,
}

/// A library with `titles` filed the way an import files them, and a synthetic rekordbox whose
/// first track is the library's first file.
fn setup(titles: &[&str], version: &str) -> Setup {
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
                ..TagInfo::default()
            },
        );
        let destination = music.join(format!(
            "Studio 001/Between Spaces/{index:02} - {title}.aif"
        ));
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        service.publish_file(staged, destination.clone()).unwrap();
        files.push(destination);
    }
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let rekordbox = tempfile::tempdir().unwrap();
    let location = fixture::build(rekordbox.path(), Shape::default()).unwrap();
    if let Some(first) = files.first() {
        fixture::point_at_audio(&location, 0, &rekordbox_form(first), 2).unwrap();
    }
    Setup {
        service,
        probe: Probe::new(Some(location), Some(version.to_owned())).running_from(|| false),
        files,
        _data: data,
        _folder: folder,
        _rekordbox: rekordbox,
    }
}

fn track_ids(service: &LibraryService) -> std::collections::HashMap<String, i64> {
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

fn playlist(service: &LibraryService, name: &str) -> Option<oc_core_db::library::PlaylistRow> {
    service
        .playlists()
        .unwrap()
        .into_iter()
        .find(|row| row.name == name)
}

/// Reading on and read once, then writing on.
fn writing_on(setup: &Setup) {
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    rekordbox_db::read(&setup.service, &setup.probe).unwrap();
    set_enabled(&setup.service, &setup.probe, true).unwrap();
}

#[test]
fn writing_is_off_until_turned_on_and_needs_reading_on_first() {
    let setup = setup(&["Night Drive"], "7.2.19");
    let state = rekordbox_db::status(&setup.service, &setup.probe).unwrap();
    assert!(!state.write_enabled && state.write_tested);
    let error = send_new(&setup.service, &setup.probe, None).unwrap_err();
    assert!(error.contains("Settings"), "{error}");
    let error = set_enabled(&setup.service, &setup.probe, true).unwrap_err();
    assert!(error.contains("reading"), "{error}");
    writing_on(&setup);
    let state = rekordbox_db::status(&setup.service, &setup.probe).unwrap();
    assert!(state.write_enabled && !state.write_needs_confirmation);
    assert_eq!(state.write_confirmed_version.as_deref(), Some("7.2.19"));
    // Turning reading off turns writing off with it.
    rekordbox_db::set_enabled(&setup.service, &setup.probe, false).unwrap();
    let state = rekordbox_db::status(&setup.service, &setup.probe).unwrap();
    assert!(!state.write_enabled);
}

#[test]
fn an_untested_version_cannot_be_written_to() {
    let setup = setup(&["Night Drive"], "7.3.0");
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    let state = rekordbox_db::status(&setup.service, &setup.probe).unwrap();
    assert!(!state.write_tested);
    let error = set_enabled(&setup.service, &setup.probe, true).unwrap_err();
    assert!(error.contains("7.3.0"), "{error}");

    // Confirmed for one version, then rekordbox updates: writing waits for a new confirmation.
    let mut setup = self::setup(&["Night Drive"], "7.2.19");
    writing_on(&setup);
    setup.probe.app_version = Some("7.2.20".into());
    let state = rekordbox_db::status(&setup.service, &setup.probe).unwrap();
    assert!(state.write_enabled && state.write_needs_confirmation);
    let error = send_new(&setup.service, &setup.probe, None).unwrap_err();
    assert!(error.contains("7.2.20"), "{error}");
}

#[test]
fn a_track_dropped_on_a_rekordbox_playlist_goes_into_it_after_a_backup() {
    let setup = setup(&["Night Drive", "Soft Focus"], "7.2.19");
    writing_on(&setup);
    let target = playlist(&setup.service, "Playlist 1").unwrap();
    let ids = track_ids(&setup.service);
    let done = add_to_playlist(
        &setup.service,
        &setup.probe,
        target.id,
        &[ids["Night Drive"], ids["Soft Focus"]],
    )
    .unwrap();
    assert_eq!(done.playlist, "Playlist 1");
    // Night Drive is rekordbox's track 0 already; Soft Focus is new to rekordbox.
    assert_eq!((done.added, done.imported), (2, 1));
    assert_eq!(done.left_out, Vec::<String>::new());
    // openCrate read rekordbox again, so the sidebar shows the change.
    let after = playlist(&setup.service, "Playlist 1").unwrap();
    assert_eq!(after.entries, target.entries + 2);
    assert_eq!(after.in_library, 2);
    let reasons: Vec<String> = rekordbox_db::backups(&setup.service)
        .into_iter()
        .map(|backup| backup.reason)
        .collect();
    assert!(reasons.contains(&"before-write".to_owned()), "{reasons:?}");
}

#[test]
fn new_tracks_go_to_rekordbox_in_the_imported_playlist() {
    let setup = setup(&["Night Drive", "Soft Focus", "Late Train"], "7.2.19");
    writing_on(&setup);
    // Night Drive is rekordbox's track 0, so only the other two are new.
    let listed = crate::rekordbox_send::new_tracks(&setup.service).unwrap();
    assert_eq!(listed.tracks.len(), 2);
    let done = send_new(&setup.service, &setup.probe, None).unwrap();
    assert_eq!(done.playlist, IMPORTED);
    assert_eq!((done.added, done.imported), (2, 2));
    let imported = playlist(&setup.service, IMPORTED).unwrap();
    assert_eq!((imported.entries, imported.in_library), (2, 2));
    // Now rekordbox has them, so nothing is left to send.
    let listed = crate::rekordbox_send::new_tracks(&setup.service).unwrap();
    assert!(listed.tracks.is_empty());
    let done = send_new(&setup.service, &setup.probe, None).unwrap();
    assert_eq!((done.added, done.imported), (0, 0));
}

#[test]
fn only_the_ticked_tracks_are_sent_when_some_are_named() {
    let setup = setup(&["Night Drive", "Soft Focus", "Late Train"], "7.2.19");
    writing_on(&setup);
    let listed = crate::rekordbox_send::new_tracks(&setup.service).unwrap();
    let soft = listed
        .tracks
        .iter()
        .find(|track| track.title == "Soft Focus")
        .unwrap();
    let done = send_new(&setup.service, &setup.probe, Some(&[soft.file_id])).unwrap();
    assert_eq!(done.added, 1);
    let left: Vec<String> = crate::rekordbox_send::new_tracks(&setup.service)
        .unwrap()
        .tracks
        .into_iter()
        .map(|track| track.title)
        .collect();
    assert_eq!(left, ["Late Train"]);
}

#[test]
fn a_hidden_track_is_not_sent() {
    let setup = setup(&["Night Drive", "Soft Focus", "Late Train"], "7.2.19");
    writing_on(&setup);
    let listed = crate::rekordbox_send::new_tracks(&setup.service).unwrap();
    let late = listed
        .tracks
        .iter()
        .find(|track| track.title == "Late Train")
        .unwrap();
    crate::rekordbox_send::set_hidden(&setup.service, &[late.file_id], true, 0).unwrap();
    let done = send_new(&setup.service, &setup.probe, None).unwrap();
    assert_eq!(done.added, 1);
}

#[test]
fn a_running_rekordbox_is_refused_and_nothing_is_backed_up() {
    let setup = setup(&["Night Drive", "Soft Focus"], "7.2.19");
    writing_on(&setup);
    let before = rekordbox_db::backups(&setup.service).len();
    setup.probe.set_running(true);
    let error = send_new(&setup.service, &setup.probe, None).unwrap_err();
    assert!(error.contains("running"), "{error}");
    assert_eq!(rekordbox_db::backups(&setup.service).len(), before);
}

#[test]
fn a_playlist_from_an_xml_export_is_explained() {
    let setup = setup(&["Night Drive"], "7.2.19");
    writing_on(&setup);
    let export = setup.files[0].parent().unwrap().join("export.xml");
    let track = oc_rekordbox::xml::NewTrack {
        path: setup.files[0].as_os_str().as_encoded_bytes().to_vec(),
        name: "Night Drive".into(),
        artist: String::new(),
        album: String::new(),
        year: None,
        total_seconds: 0,
        size: 0,
        kind: "AIFF".into(),
    };
    fs::write(
        &export,
        oc_rekordbox::xml::write_offer(&[track], "From export", "1"),
    )
    .unwrap();
    setup.service.import_export(Path::new(&export)).unwrap();
    let target = playlist(&setup.service, "From export").unwrap();
    let ids = track_ids(&setup.service);
    let error = add_to_playlist(
        &setup.service,
        &setup.probe,
        target.id,
        &[ids["Night Drive"]],
    )
    .unwrap_err();
    assert!(error.contains("export"), "{error}");
}

/// rekordbox's track 0 (the library's first file) gets these values, as an edit made in rekordbox.
fn rekordbox_edits(setup: &Setup, title: &str) {
    let location = setup.probe.location.clone().unwrap();
    let library = rbl_db::Library::open(location, rbl_db::OpenMode::ReadWrite).unwrap();
    library
        .connection()
        .execute(
            "UPDATE djmdContent SET Title = ?1 WHERE ID = ?2",
            [title, fixture::track_id(0).as_str()],
        )
        .unwrap();
}

fn title_change(review: &rekordbox_merge::ReviewDto) -> Option<&rekordbox_merge::ChangeDto> {
    review.changes.iter().find(|change| change.field == "title")
}

fn library_title(service: &LibraryService) -> String {
    track_ids(service).into_keys().next().unwrap()
}

#[test]
fn a_title_changed_in_rekordbox_comes_into_the_file_when_ticked() {
    let setup = setup(&["Night Drive"], "7.2.19");
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    rekordbox_edits(&setup, "Night Drive (Original Mix)");
    let review = rekordbox_merge::review(&setup.service, &setup.probe).unwrap();
    let change = title_change(&review).unwrap();
    assert_eq!(
        (
            change.ours.as_str(),
            change.theirs.as_str(),
            change.direction
        ),
        ("Night Drive", "Night Drive (Original Mix)", "toOpenCrate")
    );
    let choice = ChoiceDto {
        rb_key: change.rb_key,
        field: "title".into(),
        keep: Keep::Rekordbox,
        ours: change.ours.clone(),
        theirs: change.theirs.clone(),
    };
    let done = rekordbox_merge::apply(&setup.service, &setup.probe, &[choice]).unwrap();
    assert_eq!((done.to_opencrate, done.to_rekordbox), (1, 0));
    // The file holds the new title, and the library shows it.
    let tags = oc_tag::read_file(&setup.files[0]).unwrap().tags;
    assert_eq!(tags.title.as_deref(), Some("Night Drive (Original Mix)"));
    assert_eq!(library_title(&setup.service), "Night Drive (Original Mix)");
    let review = rekordbox_merge::review(&setup.service, &setup.probe).unwrap();
    assert!(title_change(&review).is_none(), "in step now");
}

#[test]
fn keeping_opencrates_title_writes_it_to_rekordbox_with_writing_on() {
    let setup = setup(&["Night Drive"], "7.2.19");
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    rekordbox_edits(&setup, "Night Drive (Original Mix)");
    let review = rekordbox_merge::review(&setup.service, &setup.probe).unwrap();
    let change = title_change(&review).unwrap().clone();
    let choice = || ChoiceDto {
        rb_key: change.rb_key,
        field: "title".into(),
        keep: Keep::OpenCrate,
        ours: change.ours.clone(),
        theirs: change.theirs.clone(),
    };
    // Writing to rekordbox is off: nothing is written, and it says so.
    let error = rekordbox_merge::apply(&setup.service, &setup.probe, &[choice()]).unwrap_err();
    assert!(error.contains("Settings"), "{error}");

    set_enabled(&setup.service, &setup.probe, true).unwrap();
    let done = rekordbox_merge::apply(&setup.service, &setup.probe, &[choice()]).unwrap();
    assert_eq!((done.to_opencrate, done.to_rekordbox), (0, 1));
    let review = rekordbox_merge::review(&setup.service, &setup.probe).unwrap();
    assert!(
        title_change(&review).is_none(),
        "rekordbox has openCrate's title now"
    );
    assert_eq!(
        library_title(&setup.service),
        "Night Drive",
        "the file is untouched"
    );
}

#[test]
fn a_choice_made_on_values_that_have_since_changed_is_skipped() {
    let setup = setup(&["Night Drive"], "7.2.19");
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    rekordbox_edits(&setup, "Night Drive (Original Mix)");
    let review = rekordbox_merge::review(&setup.service, &setup.probe).unwrap();
    let change = title_change(&review).unwrap();
    let stale = ChoiceDto {
        rb_key: change.rb_key,
        field: "title".into(),
        keep: Keep::Rekordbox,
        ours: change.ours.clone(),
        theirs: "Something else".into(),
    };
    let done = rekordbox_merge::apply(&setup.service, &setup.probe, &[stale]).unwrap();
    assert_eq!(
        (done.to_opencrate, done.to_rekordbox, done.skipped.len()),
        (0, 0, 1)
    );
    assert_eq!(library_title(&setup.service), "Night Drive");
}
