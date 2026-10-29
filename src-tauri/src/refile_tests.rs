use std::fs;
use std::path::{Path, PathBuf};

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use rbl_db::fixture::{self, Shape};

use crate::refile::{refile, refile_plan};
use crate::rekordbox_db::{self, Probe};

struct Setup {
    _data: tempfile::TempDir,
    _folder: tempfile::TempDir,
    _rekordbox: tempfile::TempDir,
    service: LibraryService,
    probe: Probe,
    music: PathBuf,
}

fn put(path: &Path, artist: Option<&str>, title: &str, track: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, aiff(2)).unwrap();
    write_tags(
        path,
        &TagInfo {
            artist: artist.map(Into::into),
            title: Some(title.into()),
            album: Some("Between Spaces".into()),
            track_number: Some(track),
            ..TagInfo::default()
        },
    );
}

const NIGHT: &str = "Unsorted/night.aif";

/// Night Drive and Soft Focus filed loosely, one file already in place, one with no artist; rekordbox
/// uses Night Drive.
fn setup() -> Setup {
    let folder = tempfile::tempdir().unwrap();
    let music = folder.path().join("Music");
    fs::create_dir_all(&music).unwrap();
    let music = oc_paths::canonical(&music).unwrap();
    put(&music.join(NIGHT), Some("Studio 001"), "Night Drive", 1);
    put(
        &music.join("Unsorted/Deeper/soft.aif"),
        Some("Studio 001"),
        "Soft Focus",
        2,
    );
    put(
        &music.join("Studio 001/Between Spaces/03 - Studio 001 - Late Train.aif"),
        Some("Studio 001"),
        "Late Train",
        3,
    );
    put(&music.join("Unsorted/nobody.aif"), None, "Nobody", 4);
    let data = tempfile::tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    service.scan_root(root, &ScanOptions::default()).unwrap();
    let rekordbox = tempfile::tempdir().unwrap();
    let location = fixture::build(rekordbox.path(), Shape::default()).unwrap();
    let stored = music.join(NIGHT).to_string_lossy().replace('\\', "/");
    fixture::point_at_audio(&location, 0, &stored, 2).unwrap();
    let probe = Probe::new(Some(location), Some("7.2.19".into())).running_from(|| false);
    Setup {
        _data: data,
        _folder: folder,
        _rekordbox: rekordbox,
        service,
        probe,
        music,
    }
}

fn stored_path(setup: &Setup) -> String {
    let location = setup.probe.location.clone().unwrap();
    rbl_db::Library::open(location, rbl_db::OpenMode::ReadOnly)
        .unwrap()
        .connection()
        .query_row(
            "SELECT FolderPath FROM djmdContent WHERE ID = ?1",
            [fixture::track_id(0)],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn files_are_refiled_by_the_layout_only_when_chosen_and_rekordbox_follows() {
    let setup = setup();
    let plan = refile_plan(&setup.service).unwrap();
    assert_eq!(
        plan.in_place, 1,
        "Late Train is already where the layout puts it"
    );
    let mut moves: Vec<(&str, &str)> = plan
        .moves
        .iter()
        .map(|item| (item.from.as_str(), item.to.as_str()))
        .collect();
    moves.sort_unstable();
    assert_eq!(
        moves,
        [
            (
                "Unsorted/Deeper/soft.aif",
                "Studio 001/Between Spaces/02 - Studio 001 - Soft Focus.aif"
            ),
            (
                "Unsorted/night.aif",
                "Studio 001/Between Spaces/01 - Studio 001 - Night Drive.aif"
            ),
        ]
    );
    assert_eq!(plan.skipped.len(), 1);
    assert_eq!(plan.skipped[0].path, "Music: Unsorted/nobody.aif");
    assert!(
        plan.skipped[0].why.contains("no artist"),
        "{:?}",
        plan.skipped
    );
    let night = plan
        .moves
        .iter()
        .find(|item| item.from == NIGHT)
        .unwrap()
        .clone();
    let soft = plan
        .moves
        .iter()
        .find(|item| item.from != NIGHT)
        .unwrap()
        .clone();
    let track = setup.service.track_of_file(night.file_id).unwrap();

    // Reading rekordbox is off: nothing moves.
    let error = refile(&setup.service, &setup.probe, &[night.file_id], None).unwrap_err();
    assert!(error.contains("reading rekordbox"), "{error}");
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    // rekordbox uses Night Drive and writing is off: nothing moves.
    let error = refile(
        &setup.service,
        &setup.probe,
        &[night.file_id, soft.file_id],
        None,
    )
    .unwrap_err();
    assert!(
        error.contains("rekordbox uses some of these files"),
        "{error}"
    );
    assert!(setup.music.join(NIGHT).is_file());

    // Soft Focus alone is not used by rekordbox; its emptied folders go.
    let done = refile(&setup.service, &setup.probe, &[soft.file_id], None).unwrap();
    assert_eq!((done.moved, done.relocated), (1, 0), "{:?}", done.left);
    assert!(setup.music.join(&soft.to).is_file());
    assert!(!setup.music.join("Unsorted/Deeper").exists());
    assert!(
        setup.music.join("Unsorted").is_dir(),
        "still holds other files"
    );

    crate::rekordbox_write::set_enabled(&setup.service, &setup.probe, true).unwrap();
    let done = refile(&setup.service, &setup.probe, &[night.file_id], None).unwrap();
    assert_eq!((done.moved, done.relocated), (1, 1), "{:?}", done.left);
    let now = setup.music.join(&night.to);
    assert!(now.is_file());
    assert_eq!(
        stored_path(&setup),
        now.to_string_lossy().replace('\\', "/")
    );
    assert_eq!(
        setup.service.track_of_file(night.file_id).unwrap(),
        track,
        "the same track"
    );
    let plan = refile_plan(&setup.service).unwrap();
    assert_eq!((plan.moves.len(), plan.in_place), (0, 3));
}

#[test]
fn two_files_bound_for_one_place_or_a_taken_place_are_left() {
    let setup = setup();
    // A second copy of Night Drive, tagged the same, elsewhere.
    put(
        &setup.music.join("Other/night copy.aif"),
        Some("Studio 001"),
        "Night Drive",
        1,
    );
    let root = setup.service.roots().unwrap()[0].root.id;
    setup
        .service
        .scan_root(root, &ScanOptions::default())
        .unwrap();
    let plan = refile_plan(&setup.service).unwrap();
    assert!(
        plan.moves
            .iter()
            .all(|item| !item.to.contains("Night Drive")),
        "{:?}",
        plan.moves
    );
    let why: Vec<&str> = plan
        .skipped
        .iter()
        .filter(|item| item.path.contains("night"))
        .map(|item| item.why.as_str())
        .collect();
    assert_eq!(why, ["another file would go to the same place"; 2]);
}

#[test]
fn a_sitting_records_its_moves_and_puts_them_back_with_rekordbox_following() {
    use crate::reorganize::{begin, last, undo_sitting};
    let setup = setup();
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    crate::rekordbox_write::set_enabled(&setup.service, &setup.probe, true).unwrap();
    assert_eq!(last(&setup.service).unwrap(), None);
    let plan = refile_plan(&setup.service).unwrap();
    let ids: Vec<i64> = plan.moves.iter().map(|item| item.file_id).collect();
    let night = plan
        .moves
        .iter()
        .find(|item| item.from == NIGHT)
        .unwrap()
        .clone();
    let soft = plan
        .moves
        .iter()
        .find(|item| item.from != NIGHT)
        .unwrap()
        .clone();

    let sitting = begin(&setup.service).unwrap();
    let done = refile(&setup.service, &setup.probe, &ids, Some(sitting)).unwrap();
    assert_eq!((done.moved, done.relocated), (2, 1), "{:?}", done.left);
    let recorded = last(&setup.service).unwrap().unwrap();
    assert_eq!(
        (recorded.id, recorded.moves, recorded.undone),
        (sitting, 2, false)
    );
    assert!(!setup.music.join(NIGHT).exists());

    // Only the last sitting can be put back, and only once.
    assert!(undo_sitting(&setup.service, &setup.probe, sitting + 1).is_err());
    let undone = undo_sitting(&setup.service, &setup.probe, sitting).unwrap();
    assert_eq!(
        (undone.put_back, undone.relocated),
        (2, 1),
        "{:?}",
        undone.left
    );
    assert!(setup.music.join(NIGHT).is_file());
    assert!(setup.music.join("Unsorted/Deeper/soft.aif").is_file());
    assert!(!setup.music.join(&night.to).exists());
    assert!(!setup.music.join(&soft.to).exists());
    assert_eq!(
        stored_path(&setup),
        setup.music.join(NIGHT).to_string_lossy().replace('\\', "/"),
        "rekordbox points at the old place again"
    );
    assert!(last(&setup.service).unwrap().unwrap().undone);
    let error = undo_sitting(&setup.service, &setup.probe, sitting).unwrap_err();
    assert!(error.contains("already"), "{error}");
    // The library knows the files at their old places: the plan offers the same moves again.
    let plan = refile_plan(&setup.service).unwrap();
    assert_eq!(plan.moves.len(), 2, "{:?}", plan.moves);
}

#[test]
fn a_file_moved_elsewhere_since_the_sitting_stays_where_it_is() {
    use crate::reorganize::{begin, undo_sitting};
    let setup = setup();
    rekordbox_db::set_enabled(&setup.service, &setup.probe, true).unwrap();
    let plan = refile_plan(&setup.service).unwrap();
    let soft = plan
        .moves
        .iter()
        .find(|item| item.from != NIGHT)
        .unwrap()
        .clone();
    let sitting = begin(&setup.service).unwrap();
    refile(&setup.service, &setup.probe, &[soft.file_id], Some(sitting)).unwrap();
    // Someone moves it again by hand, and the library catches up.
    let elsewhere = setup.music.join("Elsewhere/soft.aif");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::rename(setup.music.join(&soft.to), &elsewhere).unwrap();
    let root = setup.service.roots().unwrap()[0].root.id;
    setup
        .service
        .scan_root(root, &ScanOptions::default())
        .unwrap();
    let undone = undo_sitting(&setup.service, &setup.probe, sitting).unwrap();
    assert_eq!(undone.put_back, 0);
    assert_eq!(undone.left.len(), 1, "{:?}", undone.left);
    assert!(elsewhere.is_file());
}
