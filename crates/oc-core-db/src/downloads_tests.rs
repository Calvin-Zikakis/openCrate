use std::fs;

use tempfile::tempdir;

use crate::service::LibraryService;
use crate::Error;

fn service() -> (tempfile::TempDir, LibraryService) {
    let data = tempdir().unwrap();
    let service = LibraryService::open(&data.path().join("db"), 2).unwrap();
    (data, service)
}

fn folder(parent: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = parent.join(name);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn download_folders_are_added_toggled_and_removed_without_touching_disk() {
    let (_data, service) = service();
    let area = tempdir().unwrap();
    let downloads = folder(area.path(), "Downloads");
    fs::write(downloads.join("keep.mp3.part"), b"x").unwrap();
    let id = service.add_watch_folder(&downloads, "Downloads").unwrap();
    let list = service.watch_folders().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].enabled && list[0].online);
    service.set_watch_folder_enabled(id, false).unwrap();
    assert!(!service.watch_folders().unwrap()[0].enabled);
    service.remove_watch_folder(id).unwrap();
    assert_eq!(service.watch_folders().unwrap().len(), 0);
    assert!(downloads.join("keep.mp3.part").exists());
    assert!(matches!(
        service.remove_watch_folder(id),
        Err(Error::UnknownWatchFolder(_))
    ));
}

#[test]
fn a_download_folder_may_not_overlap_the_library_another_folder_or_the_app_data() {
    let (data, service) = service();
    let area = tempdir().unwrap();
    let music = folder(area.path(), "Music");
    let downloads = folder(area.path(), "Downloads");
    service.add_root(&music, "Music").unwrap();
    assert!(matches!(
        service.add_watch_folder(&music, "x"),
        Err(Error::WatchOverlapsLibrary)
    ));
    assert!(matches!(
        service.add_watch_folder(&folder(&music, "new"), "x"),
        Err(Error::WatchOverlapsLibrary)
    ));
    assert!(matches!(
        service.add_watch_folder(area.path(), "everything"),
        Err(Error::WatchOverlapsLibrary)
    ));
    service.add_watch_folder(&downloads, "Downloads").unwrap();
    assert!(matches!(
        service.add_watch_folder(&downloads, "again"),
        Err(Error::WatchOverlapsWatch)
    ));
    assert!(matches!(
        service.add_watch_folder(&folder(&downloads, "inner"), "x"),
        Err(Error::WatchOverlapsWatch)
    ));
    assert!(matches!(
        service.add_watch_folder(data.path(), "data"),
        Err(Error::Root(_))
    ));
    assert!(matches!(
        service.add_watch_folder(&area.path().join("missing"), "x"),
        Err(Error::Root(_))
    ));
}

#[test]
fn the_library_may_not_be_added_over_a_download_folder() {
    let (_data, service) = service();
    let area = tempdir().unwrap();
    let downloads = folder(area.path(), "Downloads");
    service.add_watch_folder(&downloads, "Downloads").unwrap();
    assert!(matches!(
        service.add_root(area.path(), "all"),
        Err(Error::RootOverlapsWatch)
    ));
    assert!(matches!(
        service.add_root(&folder(&downloads, "Sub"), "sub"),
        Err(Error::RootOverlapsWatch)
    ));
}

#[test]
fn an_unreachable_folder_stays_listed_and_returns_when_the_drive_does() {
    let (_data, service) = service();
    let area = tempdir().unwrap();
    let downloads = folder(area.path(), "Downloads");
    let id = service.add_watch_folder(&downloads, "Downloads").unwrap();
    let parked = area.path().join("Downloads-away");
    fs::rename(&downloads, &parked).unwrap();
    let gone = service.check_watch_folders().unwrap();
    assert_eq!(gone.len(), 1);
    assert!(!gone[0].online);
    assert!(gone[0].enabled, "going offline is not switching it off");
    fs::rename(&parked, &downloads).unwrap();
    let back = service.check_watch_folders().unwrap();
    assert!(back[0].online);
    assert_eq!(back[0].id, id);
    assert!(back[0].last_seen_ms >= gone[0].last_seen_ms);
}

#[test]
fn imported_releases_are_remembered_and_forgotten_with_their_folder() {
    let (_data, service) = service();
    let area = tempdir().unwrap();
    let downloads = folder(area.path(), "Downloads");
    let id = service.add_watch_folder(&downloads, "Downloads").unwrap();
    assert_eq!(service.imported_releases(id).unwrap().len(), 0);
    service
        .mark_imported(id, b"Between Spaces".to_vec(), 2)
        .unwrap();
    service
        .mark_imported(id, b"Between Spaces".to_vec(), 1)
        .unwrap();
    let names = service.imported_releases(id).unwrap();
    assert_eq!(names.len(), 1, "importing again updates the record");
    assert!(names.contains(b"Between Spaces".as_slice()));
    service.remove_watch_folder(id).unwrap();
    let again = service.add_watch_folder(&downloads, "Downloads").unwrap();
    assert_eq!(service.imported_releases(again).unwrap().len(), 0);
}

#[test]
fn settings_are_kept_replaced_and_removed() {
    let (data, service) = service();
    assert_eq!(service.setting("folder_layout").unwrap(), None);
    service
        .set_setting("folder_layout", Some("{artist}/{title}".into()))
        .unwrap();
    service
        .set_setting("folder_layout", Some("{release}/{title}".into()))
        .unwrap();
    assert_eq!(
        service.setting("folder_layout").unwrap().as_deref(),
        Some("{release}/{title}")
    );
    service.shutdown().unwrap();
    let service = LibraryService::open(&data.path().join("db"), 2).unwrap();
    assert_eq!(
        service.setting("folder_layout").unwrap().as_deref(),
        Some("{release}/{title}"),
        "survives a restart"
    );
    service.set_setting("folder_layout", None).unwrap();
    assert_eq!(service.setting("folder_layout").unwrap(), None);
}
