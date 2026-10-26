//! What is in the watch folders right now, read only.
//!
//! Each poll lists every enabled, reachable watch folder and asks `oc-ingest` which
//! releases look finished. The stability trackers live here, in memory: a restart makes every
//! file wait out the quiet period again, which is the safe direction. Nothing is copied,
//! staged or changed; importing is a later slice.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use oc_core_db::downloads::WatchFolder;
use oc_core_db::service::LibraryService;
use oc_ingest::{
    evaluate, list_folder, ReleaseKey, ReleaseState, StabilityConfig, StabilityTracker, Waiting,
};
use serde::Serialize;

#[derive(Default)]
pub struct DownloadWatch {
    config: StabilityConfig,
    trackers: Mutex<HashMap<i64, StabilityTracker<Vec<u8>>>>,
}

impl DownloadWatch {
    #[cfg(test)]
    pub fn with_config(config: StabilityConfig) -> Self {
        Self {
            config,
            trackers: Mutex::default(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDto {
    pub folder_id: i64,
    pub folder_label: String,
    /// For display only; lossy where the name is not text.
    pub name: String,
    /// A file directly in the watch folder rather than a release folder.
    pub loose: bool,
    /// `ready`, `downloading`, `settling` or `nothing`.
    pub state: &'static str,
    pub quiet_seconds: Option<i64>,
    pub audio_files: usize,
    pub archives: usize,
    /// Every file is older than the day the folder was added, so it was there already and a
    /// later import would not pick it up on its own.
    pub already_there: bool,
    /// Imported with openCrate. The originals are still in the watch folder.
    pub imported: bool,
    pub newest_modified_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchStateDto {
    pub folder_id: i64,
    pub label: String,
    /// `watching`, `off`, `offline` or `unreadable`.
    pub state: &'static str,
    /// Subfolders that could not be read and were left out.
    pub unreadable_dirs: usize,
    /// The folder holds more files than are listed.
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsDto {
    pub folders: Vec<WatchStateDto>,
    pub releases: Vec<ReleaseDto>,
}

use oc_core_db::now_ms;

fn describe(
    folder: &WatchFolder,
    baseline: Option<&HashSet<Vec<u8>>>,
    imported: &HashSet<Vec<u8>>,
    status: &oc_ingest::ReleaseStatus,
) -> ReleaseDto {
    let (name, loose) = match &status.key {
        ReleaseKey::Folder(name) => (name, false),
        ReleaseKey::Loose(name) => (name, true),
    };
    let (state, quiet_seconds) = match status.state {
        ReleaseState::Ready => ("ready", None),
        ReleaseState::Waiting(Waiting::Partial) => ("downloading", None),
        ReleaseState::Waiting(Waiting::Changing { shortest_quiet_ms }) => {
            ("settling", Some(shortest_quiet_ms / 1000))
        }
        ReleaseState::Waiting(Waiting::NothingToImport) => ("nothing", None),
    };
    ReleaseDto {
        folder_id: folder.id,
        folder_label: folder.label.clone(),
        name: String::from_utf8_lossy(name).into_owned(),
        loose,
        state,
        quiet_seconds,
        audio_files: status.audio_files,
        archives: status.archives,
        // The list taken when the folder was added decides; dates cannot, because a copy keeps
        // its old modified time. Without a list nothing is assumed old.
        already_there: baseline.is_some_and(|names| names.contains(name)),
        imported: imported.contains(name),
        newest_modified_ms: status.newest_modified_ms,
    }
}

/// List the watch folders and evaluate their releases at `now_ms`. Newest first.
pub fn poll(
    service: &LibraryService,
    watch: &DownloadWatch,
    now_ms: i64,
) -> Result<DownloadsDto, String> {
    let folders = service
        .check_watch_folders()
        .map_err(|error| error.to_string())?;
    let mut trackers = watch
        .trackers
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let known: HashSet<i64> = folders.iter().map(|folder| folder.id).collect();
    trackers.retain(|id, _| known.contains(id));
    let mut states = Vec::new();
    let mut releases = Vec::new();
    for folder in &folders {
        let mut state = WatchStateDto {
            folder_id: folder.id,
            label: folder.label.clone(),
            state: "watching",
            unreadable_dirs: 0,
            truncated: false,
        };
        if !folder.enabled {
            state.state = "off";
        } else if !folder.online {
            state.state = "offline";
        } else {
            let path = oc_paths::bytes_to_os_string(&folder.path_raw)
                .map(PathBuf::from)
                .map_err(|_| "A watch folder path cannot be represented here.".to_owned())?;
            let baseline = service
                .watch_baseline(folder.id)
                .map_err(|error| error.to_string())?;
            let imported = service
                .imported_releases(folder.id)
                .map_err(|error| error.to_string())?;
            match list_folder(&path) {
                Ok(listing) => {
                    state.unreadable_dirs = listing.unreadable_dirs;
                    state.truncated = listing.truncated;
                    let tracker = trackers
                        .entry(folder.id)
                        .or_insert_with(|| StabilityTracker::new(watch.config));
                    releases.extend(
                        evaluate(tracker, &listing.entries, now_ms)
                            .iter()
                            .map(|status| describe(folder, baseline.as_ref(), &imported, status)),
                    );
                }
                Err(_) => state.state = "unreadable",
            }
        }
        states.push(state);
    }
    releases.sort_by(|a, b| {
        b.newest_modified_ms
            .cmp(&a.newest_modified_ms)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(DownloadsDto {
        folders: states,
        releases,
    })
}

/// `poll` at the current time.
pub fn poll_now(service: &LibraryService, watch: &DownloadWatch) -> Result<DownloadsDto, String> {
    poll(service, watch, now_ms())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn quick() -> DownloadWatch {
        DownloadWatch::with_config(StabilityConfig {
            quiet_ms: 0,
            min_sightings: 2,
            trust_old_mtime_ms: None,
        })
    }

    fn setup() -> (tempfile::TempDir, LibraryService, tempfile::TempDir) {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        (data, service, tempfile::tempdir().unwrap())
    }

    fn state_of<'a>(dto: &'a DownloadsDto, name: &str) -> &'a ReleaseDto {
        dto.releases.iter().find(|r| r.name == name).unwrap()
    }

    #[test]
    fn releases_are_ready_only_when_finished_and_unchanged() {
        let (_data, service, area) = setup();
        let downloads = area.path().join("Downloads");
        fs::create_dir_all(downloads.join("Done")).unwrap();
        fs::create_dir_all(downloads.join("Busy")).unwrap();
        fs::write(downloads.join("Done/01.flac"), b"abc").unwrap();
        fs::write(downloads.join("Busy/01.flac.part"), b"a").unwrap();
        fs::write(downloads.join("Busy/00.flac"), b"abc").unwrap();
        service.add_watch_folder(&downloads, "Downloads").unwrap();
        let watch = quick();

        let first = poll(&service, &watch, 1_000).unwrap();
        assert_eq!(state_of(&first, "Done").state, "settling");
        assert_eq!(state_of(&first, "Busy").state, "downloading");
        let second = poll(&service, &watch, 2_000).unwrap();
        assert_eq!(state_of(&second, "Done").state, "ready");
        assert_eq!(state_of(&second, "Done").audio_files, 1);
        assert_eq!(state_of(&second, "Busy").state, "downloading");

        fs::rename(
            downloads.join("Busy/01.flac.part"),
            downloads.join("Busy/01.flac"),
        )
        .unwrap();
        poll(&service, &watch, 3_000).unwrap();
        assert_eq!(
            state_of(&poll(&service, &watch, 4_000).unwrap(), "Busy").state,
            "ready"
        );
        assert!(
            downloads.join("Done/01.flac").exists(),
            "nothing was touched"
        );
    }

    #[test]
    fn what_was_there_when_the_folder_was_added_is_marked_and_new_arrivals_are_not() {
        let (_data, service, area) = setup();
        let downloads = area.path().join("Downloads");
        fs::create_dir_all(downloads.join("Old")).unwrap();
        fs::write(downloads.join("Old/01.flac"), b"abc").unwrap();
        service.add_watch_folder(&downloads, "Downloads").unwrap();
        let watch = quick();
        let before = poll(&service, &watch, 1).unwrap();
        assert!(state_of(&before, "Old").already_there);

        // A copy keeps its old modified time: this file claims to be from 2001.
        fs::create_dir_all(downloads.join("Copied")).unwrap();
        let copied = downloads.join("Copied/01.flac");
        fs::write(&copied, b"abc").unwrap();
        fs::File::options()
            .write(true)
            .open(&copied)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
            .unwrap();
        fs::write(downloads.join("loose.mp3"), b"abc").unwrap();
        let after = poll(&service, &watch, 2).unwrap();
        assert!(state_of(&after, "Old").already_there);
        assert!(
            !state_of(&after, "Copied").already_there,
            "new, whatever its dates say"
        );
        assert!(!state_of(&after, "loose.mp3").already_there);
    }

    #[test]
    fn a_folder_without_a_list_assumes_nothing_is_old_and_the_list_can_be_taken_on_request() {
        let (_data, service, area) = setup();
        let downloads = area.path().join("Downloads");
        fs::create_dir_all(downloads.join("Old")).unwrap();
        fs::write(downloads.join("Old/01.flac"), b"abc").unwrap();
        let id = service.add_watch_folder(&downloads, "Downloads").unwrap();
        // As a folder added before lists were kept looks.
        service.retake_watch_baseline(id).unwrap();
        let watch = quick();
        assert!(state_of(&poll(&service, &watch, 1).unwrap(), "Old").already_there);
        fs::create_dir_all(downloads.join("Copied")).unwrap();
        fs::write(downloads.join("Copied/01.flac"), b"abc").unwrap();
        assert!(!state_of(&poll(&service, &watch, 2).unwrap(), "Copied").already_there);
        // The person says: everything here now is old.
        service.retake_watch_baseline(id).unwrap();
        assert!(state_of(&poll(&service, &watch, 3).unwrap(), "Copied").already_there);
        assert!(
            downloads.join("Copied/01.flac").exists(),
            "nothing on disk changes"
        );
        assert!(service.retake_watch_baseline(999).is_err());
    }

    #[test]
    fn the_list_taken_when_a_folder_is_added_is_kept_and_removed_with_it() {
        let (_data, service, area) = setup();
        let downloads = area.path().join("Downloads");
        fs::create_dir_all(downloads.join("A")).unwrap();
        fs::write(downloads.join("b.mp3"), b"x").unwrap();
        let id = service.add_watch_folder(&downloads, "Downloads").unwrap();
        let names = service.watch_baseline(id).unwrap().unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains(b"A".as_slice()) && names.contains(b"b.mp3".as_slice()));
        service.remove_watch_folder(id).unwrap();
        assert_eq!(service.watch_baseline(id).unwrap(), None);
        // Adding it again takes a new list, so what is there now counts as already there.
        fs::write(downloads.join("c.mp3"), b"x").unwrap();
        let again = service.add_watch_folder(&downloads, "Downloads").unwrap();
        assert_eq!(service.watch_baseline(again).unwrap().unwrap().len(), 3);
    }

    #[test]
    fn off_offline_and_removed_folders_are_reported_not_listed() {
        let (_data, service, area) = setup();
        let downloads = area.path().join("Downloads");
        fs::create_dir_all(downloads.join("R")).unwrap();
        fs::write(downloads.join("R/01.flac"), b"abc").unwrap();
        let id = service.add_watch_folder(&downloads, "Downloads").unwrap();
        let watch = quick();
        assert_eq!(poll(&service, &watch, 0).unwrap().releases.len(), 1);

        service.set_watch_folder_enabled(id, false).unwrap();
        let off = poll(&service, &watch, 1).unwrap();
        assert_eq!((off.folders[0].state, off.releases.len()), ("off", 0));
        service.set_watch_folder_enabled(id, true).unwrap();

        let away = area.path().join("away");
        fs::rename(&downloads, &away).unwrap();
        let offline = poll(&service, &watch, 2).unwrap();
        assert_eq!(
            (offline.folders[0].state, offline.releases.len()),
            ("offline", 0)
        );
        fs::rename(&away, &downloads).unwrap();
        assert_eq!(
            poll(&service, &watch, 3).unwrap().folders[0].state,
            "watching"
        );

        service.remove_watch_folder(id).unwrap();
        let none = poll(&service, &watch, 4).unwrap();
        assert_eq!((none.folders.len(), none.releases.len()), (0, 0));
        assert_eq!(watch.trackers.lock().unwrap().len(), 0);
    }

    /// Timing: `cargo test -p opencrate-desktop -- --ignored --nocapture poll_time`.
    #[test]
    #[ignore = "benchmark"]
    fn poll_time_for_a_big_folder() {
        let (_data, service, area) = setup();
        let downloads = area.path().join("Downloads");
        for release in 0..400 {
            let folder = downloads.join(format!("Release {release}"));
            fs::create_dir_all(&folder).unwrap();
            for track in 0..12 {
                fs::write(folder.join(format!("{track:02}.flac")), b"x").unwrap();
            }
        }
        service.add_watch_folder(&downloads, "Downloads").unwrap();
        let watch = DownloadWatch::default();
        for round in 0..3 {
            let start = std::time::Instant::now();
            let dto = poll(&service, &watch, 1_000 * round).unwrap();
            eprintln!(
                "poll {round}: {} releases in {:?}",
                dto.releases.len(),
                start.elapsed()
            );
        }
    }
}
