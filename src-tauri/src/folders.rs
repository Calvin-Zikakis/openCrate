//! Library folders for the UI: list them, register one, and scan in the background.
//!
//! A folder's path comes only from the native dialog, never from the page, so a compromised
//! page cannot point the scanner at an arbitrary directory. Scans run on their own thread
//! and the UI polls their status; that needs no event permissions.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use oc_core_db::downloads::WatchFolder;
use oc_core_db::service::{
    LibraryService, RootInfo, ScanOptions, ScanPhase, ScanProgress, ScanSummary,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeDto {
    pub mode: &'static str,
    /// Folders can be added and scanned.
    pub can_manage_folders: bool,
    /// The database survives a restart.
    pub persistent: bool,
    /// Why the library the owner asked for could not be opened, if that happened.
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootDto {
    pub id: i64,
    pub label: String,
    /// For display only; lossy where the name is not text.
    pub path: String,
    pub availability: String,
    pub reason: String,
    pub last_available_ms: Option<i64>,
    pub files: i64,
    pub missing: i64,
}

impl From<RootInfo> for RootDto {
    fn from(info: RootInfo) -> Self {
        Self {
            id: info.root.id,
            label: info.root.label,
            path: String::from_utf8_lossy(&info.root.path_raw).into_owned(),
            availability: info.root.availability.as_str().to_owned(),
            reason: info.root.availability_reason,
            last_available_ms: info.root.last_available_ms,
            files: info.files,
            missing: info.missing,
        }
    }
}

pub fn roots(service: &LibraryService) -> Result<Vec<RootDto>, String> {
    service
        .roots()
        .map(|roots| roots.into_iter().map(RootDto::from).collect())
        .map_err(|error| error.to_string())
}

/// Look at every folder read-only, remember what was found, and list them.
pub fn check_roots(service: &LibraryService) -> Result<Vec<RootDto>, String> {
    service
        .check_roots()
        .map(|roots| roots.into_iter().map(RootDto::from).collect())
        .map_err(|error| error.to_string())
}

/// The owner says the folder now at this path is the library.
pub fn confirm_root(service: &LibraryService, root_id: i64) -> Result<(), String> {
    service
        .confirm_root(root_id)
        .map_err(|error| error.to_string())
}

/// Register a folder the owner picked. It is checked read-only first: it must exist, be a
/// directory and not overlap another folder.
pub fn add_root(service: &LibraryService, path: &Path) -> Result<RootDto, String> {
    let label = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let id = service
        .add_root(path, &label)
        .map_err(|error| error.to_string())?;
    roots(service)?
        .into_iter()
        .find(|root| root.id == id)
        .ok_or_else(|| "The folder was added but could not be listed.".to_owned())
}

/// Remove a folder from the library after the owner confirmed. Refused while a scan runs.
pub fn remove_root(
    service: &LibraryService,
    scan: &ScanControl,
    root_id: i64,
) -> Result<usize, String> {
    if scan.is_running() {
        return Err("A scan is running. Cancel it or wait for it to finish first.".to_owned());
    }
    service
        .remove_root(root_id)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchFolderDto {
    pub id: i64,
    pub label: String,
    /// For display only; lossy where the name is not text.
    pub path: String,
    pub enabled: bool,
    pub online: bool,
    pub last_seen_ms: Option<i64>,
}

impl From<WatchFolder> for WatchFolderDto {
    fn from(folder: WatchFolder) -> Self {
        Self {
            id: folder.id,
            label: folder.label,
            path: String::from_utf8_lossy(&folder.path_raw).into_owned(),
            enabled: folder.enabled,
            online: folder.online,
            last_seen_ms: folder.last_seen_ms,
        }
    }
}

pub fn watch_folders(service: &LibraryService) -> Result<Vec<WatchFolderDto>, String> {
    service
        .watch_folders()
        .map(|folders| folders.into_iter().map(WatchFolderDto::from).collect())
        .map_err(|error| error.to_string())
}

/// Look at every download folder read-only and list them.
pub fn check_watch_folders(service: &LibraryService) -> Result<Vec<WatchFolderDto>, String> {
    service
        .check_watch_folders()
        .map(|folders| folders.into_iter().map(WatchFolderDto::from).collect())
        .map_err(|error| error.to_string())
}

/// Add a download folder the owner picked. Nothing in it is staged.
pub fn add_watch_folder(service: &LibraryService, path: &Path) -> Result<WatchFolderDto, String> {
    let label = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let id = service
        .add_watch_folder(path, &label)
        .map_err(|error| error.to_string())?;
    watch_folders(service)?
        .into_iter()
        .find(|folder| folder.id == id)
        .ok_or_else(|| "The folder was added but could not be listed.".to_owned())
}

pub fn set_watch_folder_enabled(
    service: &LibraryService,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    service
        .set_watch_folder_enabled(id, enabled)
        .map_err(|error| error.to_string())
}

/// Count everything in a download folder now as already there.
pub fn retake_watch_baseline(service: &LibraryService, id: i64) -> Result<(), String> {
    service
        .retake_watch_baseline(id)
        .map_err(|error| error.to_string())
}

pub fn remove_watch_folder(service: &LibraryService, id: i64) -> Result<(), String> {
    service
        .remove_watch_folder(id)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummaryDto {
    pub availability: String,
    pub reason: String,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub revived: usize,
    pub marked_missing: usize,
    /// Files held back from "missing" until confirmed (a large disappearance).
    pub held_missing: usize,
    pub tracks_created: usize,
    pub tracks_updated: usize,
    pub tag_errors: usize,
    pub cancelled: bool,
}

impl From<&ScanSummary> for ScanSummaryDto {
    fn from(summary: &ScanSummary) -> Self {
        Self {
            availability: summary.availability.as_str().to_owned(),
            reason: summary.reason.clone(),
            inserted: summary.scan.inserted,
            updated: summary.scan.updated,
            unchanged: summary.scan.unchanged,
            revived: summary.scan.revived,
            marked_missing: summary.scan.marked_missing,
            held_missing: summary.scan.held_missing,
            tracks_created: summary.catalog.tracks_created,
            tracks_updated: summary.catalog.tracks_updated,
            tag_errors: summary.catalog.tag_errors,
            cancelled: summary.cancelled,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatusDto {
    /// `idle`, `running`, `finished` or `failed`.
    pub state: &'static str,
    pub root_id: Option<i64>,
    pub phase: Option<&'static str>,
    pub files_seen: usize,
    pub tags_read: usize,
    pub tags_total: usize,
    pub summary: Option<ScanSummaryDto>,
    pub error: Option<String>,
}

impl ScanStatusDto {
    fn new(state: &'static str, root_id: Option<i64>) -> Self {
        Self {
            state,
            root_id,
            phase: None,
            files_seen: 0,
            tags_read: 0,
            tags_total: 0,
            summary: None,
            error: None,
        }
    }
}

const fn phase_name(phase: ScanPhase) -> &'static str {
    match phase {
        ScanPhase::Checking => "checking",
        ScanPhase::Walking => "walking",
        ScanPhase::Applying => "applying",
        ScanPhase::ReadingTags => "reading-tags",
        ScanPhase::Finished => "finished",
    }
}

enum ScanState {
    Idle,
    Running {
        root_id: i64,
        progress: ScanProgress,
    },
    Finished {
        root_id: i64,
        outcome: Result<ScanSummary, String>,
    },
}

/// One scan at a time, with its status and a cancel flag.
pub struct ScanControl {
    state: Mutex<ScanState>,
    cancel: AtomicBool,
}

impl Default for ScanControl {
    fn default() -> Self {
        Self {
            state: Mutex::new(ScanState::Idle),
            cancel: AtomicBool::new(false),
        }
    }
}

impl ScanControl {
    fn lock(&self) -> MutexGuard<'_, ScanState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Start scanning a folder in the background. Fails if a scan is running or the folder
    /// is unknown. The handle is for tests; the app lets the thread run on.
    pub fn start(
        self: &Arc<Self>,
        service: &Arc<LibraryService>,
        root_id: i64,
        confirm_missing: bool,
    ) -> Result<JoinHandle<()>, String> {
        if !roots(service)?.iter().any(|root| root.id == root_id) {
            return Err("That folder is not in the library.".to_owned());
        }
        {
            let mut state = self.lock();
            if matches!(*state, ScanState::Running { .. }) {
                return Err("A scan is already running.".to_owned());
            }
            self.cancel.store(false, Ordering::Relaxed);
            *state = ScanState::Running {
                root_id,
                progress: ScanProgress {
                    phase: ScanPhase::Checking,
                    files_seen: 0,
                    tags_read: 0,
                    tags_total: 0,
                },
            };
        }
        let (control, service) = (Arc::clone(self), Arc::clone(service));
        Ok(thread::spawn(move || {
            let record = |progress: &ScanProgress| {
                *control.lock() = ScanState::Running {
                    root_id,
                    progress: *progress,
                };
            };
            let run = catch_unwind(AssertUnwindSafe(|| {
                service.scan_root(
                    root_id,
                    &ScanOptions {
                        cancel: Some(&control.cancel),
                        progress: Some(&record),
                        confirm_mass_missing: confirm_missing,
                        ..ScanOptions::default()
                    },
                )
            }));
            let outcome = match run {
                Ok(result) => result.map_err(|error| error.to_string()),
                Err(_) => Err("The scan stopped unexpectedly.".to_owned()),
            };
            *control.lock() = ScanState::Finished { root_id, outcome };
        }))
    }

    /// Test support: pretend a scan of this folder is in progress.
    #[cfg(test)]
    pub fn force_running_for_test(&self, root_id: i64) {
        *self.lock() = ScanState::Running {
            root_id,
            progress: ScanProgress {
                phase: ScanPhase::Walking,
                files_seen: 0,
                tags_read: 0,
                tags_total: 0,
            },
        };
    }

    pub fn is_running(&self) -> bool {
        matches!(*self.lock(), ScanState::Running { .. })
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn status(&self) -> ScanStatusDto {
        match &*self.lock() {
            ScanState::Idle => ScanStatusDto::new("idle", None),
            ScanState::Running { root_id, progress } => ScanStatusDto {
                phase: Some(phase_name(progress.phase)),
                files_seen: progress.files_seen,
                tags_read: progress.tags_read,
                tags_total: progress.tags_total,
                ..ScanStatusDto::new("running", Some(*root_id))
            },
            ScanState::Finished {
                root_id,
                outcome: Ok(summary),
            } => ScanStatusDto {
                tags_read: summary.tags_read,
                summary: Some(ScanSummaryDto::from(summary)),
                ..ScanStatusDto::new("finished", Some(*root_id))
            },
            ScanState::Finished {
                root_id,
                outcome: Err(message),
            } => ScanStatusDto {
                error: Some(message.clone()),
                ..ScanStatusDto::new("failed", Some(*root_id))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oc_core_db::library::{LibraryQuery, View};
    use oc_tag::fixtures::{aiff, wav, write_tags};
    use oc_tag::TagInfo;
    use tempfile::tempdir;

    use super::*;

    fn library_folder() -> tempfile::TempDir {
        let folder = tempdir().unwrap();
        for (relative, bytes, title) in [
            ("Artist/Release/01 - A - One.aif", aiff(1), "One"),
            ("Artist/Release/02 - A - Two.wav", wav(1), "Two"),
        ] {
            let path = folder.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            write_tags(
                &path,
                &TagInfo {
                    title: Some(title.into()),
                    artist: Some("A".into()),
                    album: Some("Release".into()),
                    ..TagInfo::default()
                },
            );
        }
        folder
    }

    fn service() -> (tempfile::TempDir, Arc<LibraryService>) {
        let database = tempdir().unwrap();
        let service = Arc::new(LibraryService::open(database.path(), 2).unwrap());
        (database, service)
    }

    #[test]
    fn a_folder_is_added_listed_and_scanned_in_the_background() {
        let (_database, service) = service();
        assert!(roots(&service).unwrap().is_empty());
        let folder = library_folder();
        let added = add_root(&service, folder.path()).unwrap();
        assert_eq!(added.availability, "available");
        assert_eq!((added.files, added.missing), (0, 0));
        assert_eq!(
            added.label,
            folder.path().file_name().unwrap().to_string_lossy()
        );

        let control = Arc::new(ScanControl::default());
        assert_eq!(control.status().state, "idle");
        control
            .start(&service, added.id, false)
            .unwrap()
            .join()
            .unwrap();
        let status = control.status();
        assert_eq!((status.state, status.root_id), ("finished", Some(added.id)));
        let summary = status.summary.unwrap();
        assert_eq!(
            (summary.inserted, summary.tracks_created, summary.tag_errors),
            (2, 2, 0)
        );
        assert_eq!(summary.availability, "available");

        assert_eq!(roots(&service).unwrap()[0].files, 2);
        let page = service
            .read_page(&LibraryQuery::new(View::Tracks), 0, 10)
            .unwrap();
        assert_eq!(page.total, 2);
    }

    #[test]
    fn only_one_scan_runs_at_a_time_and_unknown_folders_are_refused() {
        let (_database, service) = service();
        let folder = library_folder();
        let id = add_root(&service, folder.path()).unwrap().id;
        let control = Arc::new(ScanControl::default());
        *control.lock() = ScanState::Running {
            root_id: id,
            progress: ScanProgress {
                phase: ScanPhase::Walking,
                files_seen: 0,
                tags_read: 0,
                tags_total: 0,
            },
        };
        assert_eq!(control.status().phase, Some("walking"));
        assert!(control
            .start(&service, id, false)
            .unwrap_err()
            .contains("already running"));
        *control.lock() = ScanState::Idle;
        assert!(control
            .start(&service, 999, false)
            .unwrap_err()
            .contains("not in the library"));
        assert_eq!(
            control.status().state,
            "idle",
            "a refused start changes nothing"
        );
    }

    #[test]
    fn adding_a_bad_folder_says_why_and_a_vanished_one_fails_cleanly() {
        let (_database, service) = service();
        let folder = library_folder();
        add_root(&service, folder.path()).unwrap();
        let nested = folder.path().join("Artist");
        assert!(add_root(&service, &nested)
            .unwrap_err()
            .contains("already in the library"));
        assert!(add_root(&service, &folder.path().join("nope")).is_err());

        let other = library_folder();
        let id = add_root(&service, other.path()).unwrap().id;
        fs::remove_dir_all(other.path()).unwrap();
        let control = Arc::new(ScanControl::default());
        control.start(&service, id, false).unwrap().join().unwrap();
        let status = control.status();
        assert_eq!(status.state, "finished");
        assert_eq!(status.summary.unwrap().availability, "unavailable");
        let listed = roots(&service).unwrap();
        assert!(listed
            .iter()
            .any(|root| root.id == id && root.availability == "unavailable"));
        fs::create_dir_all(other.path()).unwrap();
    }

    #[test]
    fn a_new_scan_clears_an_earlier_cancel() {
        let (_database, service) = service();
        let folder = library_folder();
        let id = add_root(&service, folder.path()).unwrap().id;
        let control = Arc::new(ScanControl::default());
        control.cancel();
        control.start(&service, id, false).unwrap().join().unwrap();
        let summary = control.status().summary.unwrap();
        assert!(!summary.cancelled, "the cancel belonged to an earlier scan");
        assert_eq!(summary.inserted, 2);
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UntidyFolderDto {
    pub root_id: i64,
    pub root_label: String,
    pub folder: String,
    pub suggested: String,
    pub files: usize,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateDto {
    pub artist: String,
    pub title: String,
    pub files: Vec<String>,
    pub file_ids: Vec<i64>,
    /// The music folder of each file, in the same order.
    pub root_labels: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrokenFileDto {
    pub root_label: String,
    pub path: String,
    pub error: String,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HealthDto {
    pub files: usize,
    pub untidy: Vec<UntidyFolderDto>,
    pub duplicates: Vec<DuplicateDto>,
    pub broken: Vec<BrokenFileDto>,
}

/// Folders named from untidy tags and recordings filed twice. Reads the database only.
pub fn check_health(service: &LibraryService) -> Result<HealthDto, String> {
    let health = service.check_health().map_err(|error| error.to_string())?;
    let labels: std::collections::HashMap<i64, String> = service
        .roots()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|info| (info.root.id, info.root.label))
        .collect();
    Ok(HealthDto {
        files: health.files,
        broken: health
            .broken
            .into_iter()
            .map(|file| BrokenFileDto {
                root_label: labels.get(&file.root_id).cloned().unwrap_or_default(),
                path: file.path,
                error: file.error,
            })
            .collect(),
        untidy: health
            .untidy
            .into_iter()
            .map(|folder| UntidyFolderDto {
                root_id: folder.root_id,
                root_label: labels.get(&folder.root_id).cloned().unwrap_or_default(),
                folder: folder.folder,
                suggested: folder.suggested,
                files: folder.files,
            })
            .collect(),
        duplicates: health
            .duplicates
            .into_iter()
            .map(|duplicate| DuplicateDto {
                artist: duplicate.artist,
                file_ids: duplicate.file_ids,
                title: duplicate.title,
                files: duplicate.files,
                root_labels: duplicate
                    .root_ids
                    .iter()
                    .map(|id| labels.get(id).cloned().unwrap_or_default())
                    .collect(),
            })
            .collect(),
    })
}
