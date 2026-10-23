//! Reading rekordbox's own library, behind the opt-in setting. Every read works on a backup
//! copy taken while rekordbox is closed; backups are named to the page by folder name only, so
//! the page never supplies a path.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oc_core_db::service::LibraryService;
use oc_rekordbox::db::{self as rekordbox, LibraryLocation, Snapshot};
use serde::{Deserialize, Serialize};

use crate::sync::ImportDto;

/// The setting holds the rekordbox version the owner confirmed the warning for.
const SETTING: &str = "rekordbox_database";
const BACKUPS: &str = "rekordbox-backups";
/// The design keeps the last 20, plus every pinned backup.
const KEEP: usize = 20;
/// Shown when no version can be read (for now, outside macOS).
const UNKNOWN: &str = "unknown";

/// What the host can see of rekordbox. Production asks the computer; tests pass a fixture.
pub struct Probe {
    pub location: Option<LibraryLocation>,
    pub running: bool,
    pub app_version: Option<String>,
}

impl Probe {
    pub fn installed() -> Self {
        Self {
            location: rekordbox::installed_library().ok(),
            running: rekordbox::rekordbox_running(),
            app_version: rekordbox::rekordbox_app_version(),
        }
    }

    fn version(&self) -> &str {
        self.app_version.as_deref().unwrap_or(UNKNOWN)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Confirmation {
    confirmed_version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts the page shows, not a state machine"
)]
pub struct RekordboxDbDto {
    pub installed: bool,
    pub app_version: Option<String>,
    pub running: bool,
    pub enabled: bool,
    pub confirmed_version: Option<String>,
    /// On, but rekordbox has changed version since the warning was confirmed.
    pub needs_confirmation: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    pub name: String,
    pub taken_ms: i64,
    pub reason: String,
    pub pinned: bool,
    pub app_version: Option<String>,
    pub tracks: u32,
    pub playlists: u32,
}

impl From<&Snapshot> for BackupDto {
    fn from(snapshot: &Snapshot) -> Self {
        Self {
            name: snapshot
                .dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            taken_ms: snapshot.taken_ms,
            reason: snapshot.reason.clone(),
            pinned: snapshot.pinned,
            app_version: snapshot.app_version.clone(),
            tracks: snapshot.tracks,
            playlists: snapshot.playlists,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(0)
}

fn backups_dir(service: &LibraryService) -> PathBuf {
    service.data_directory().join(BACKUPS)
}

fn confirmed(service: &LibraryService) -> Result<Option<String>, String> {
    let Some(text) = service
        .setting(SETTING)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<Confirmation>(&text)
        .ok()
        .map(|confirmation| confirmation.confirmed_version))
}

pub fn status(service: &LibraryService, probe: &Probe) -> Result<RekordboxDbDto, String> {
    let confirmed_version = confirmed(service)?;
    Ok(RekordboxDbDto {
        installed: probe.location.is_some(),
        app_version: probe.app_version.clone(),
        running: probe.running,
        enabled: confirmed_version.is_some(),
        needs_confirmation: confirmed_version
            .as_deref()
            .is_some_and(|version| version != probe.version()),
        confirmed_version,
    })
}

/// Turning it on records the version the warning was confirmed for; off clears it.
pub fn set_enabled(
    service: &LibraryService,
    probe: &Probe,
    enabled: bool,
) -> Result<RekordboxDbDto, String> {
    let value = if enabled {
        Some(
            serde_json::to_string(&Confirmation {
                confirmed_version: probe.version().to_owned(),
            })
            .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    service
        .set_setting(SETTING, value)
        .map_err(|error| error.to_string())?;
    status(service, probe)
}

fn installed(probe: &Probe) -> Result<&LibraryLocation, String> {
    probe
        .location
        .as_ref()
        .ok_or_else(|| "rekordbox's library was not found on this computer.".to_owned())
}

/// Back up rekordbox's library, read the copy and make it the rekordbox source.
pub fn read(service: &LibraryService, probe: &Probe) -> Result<ImportDto, String> {
    let state = status(service, probe)?;
    if !state.enabled {
        return Err("Reading rekordbox's library directly is off. Turn it on in Settings.".into());
    }
    if state.needs_confirmation {
        return Err(format!(
            "rekordbox is now version {}. Confirm the warning in Settings again before reading.",
            probe.version()
        ));
    }
    let location = installed(probe)?;
    let backups = backups_dir(service);
    let snapshot = rekordbox::take_snapshot(location, &backups, "read", probe.running, now_ms())
        .map_err(|error| error.to_string())?;
    let collection = rekordbox::read_snapshot(&snapshot.master_db, &location.passphrase)
        .map_err(|error| error.to_string())?;
    let outcome = service
        .import_database(&location.master_db, collection)
        .map_err(|error| error.to_string())?;
    if let Err(error) = rekordbox::prune(&backups, KEEP) {
        eprintln!("Old rekordbox backups could not be removed: {error}");
    }
    Ok(ImportDto::from(&outcome))
}

pub fn backups(service: &LibraryService) -> Vec<BackupDto> {
    rekordbox::list_backups(&backups_dir(service))
        .iter()
        .map(BackupDto::from)
        .collect()
}

/// A backup folder by the name the page was given, never by a path it sends.
fn backup_named(service: &LibraryService, name: &str) -> Result<PathBuf, String> {
    let backups = backups_dir(service);
    rekordbox::list_backups(&backups)
        .into_iter()
        .map(|snapshot| snapshot.dir)
        .find(|dir| {
            dir.file_name()
                .is_some_and(|found| found == Path::new(name).as_os_str())
        })
        .ok_or_else(|| "That backup was not found.".to_owned())
}

pub fn pin(service: &LibraryService, name: &str, pinned: bool) -> Result<Vec<BackupDto>, String> {
    let dir = backup_named(service, name)?;
    rekordbox::set_pinned(&dir, pinned).map_err(|error| error.to_string())?;
    Ok(backups(service))
}

/// Put a backup back. The library it replaces is kept as a pinned backup first.
pub fn restore(service: &LibraryService, probe: &Probe, name: &str) -> Result<BackupDto, String> {
    let dir = backup_named(service, name)?;
    let location = installed(probe)?;
    let saved = rekordbox::restore(
        &dir,
        location,
        &backups_dir(service),
        probe.running,
        now_ms(),
    )
    .map_err(|error| error.to_string())?;
    Ok(BackupDto::from(&saved))
}

#[cfg(test)]
mod tests {
    use rbl_db::fixture::{self, Shape};

    use super::*;

    struct Setup {
        _data: tempfile::TempDir,
        _rekordbox: tempfile::TempDir,
        service: LibraryService,
        probe: Probe,
    }

    fn setup(version: &str) -> Setup {
        let data = tempfile::tempdir().unwrap();
        let rekordbox = tempfile::tempdir().unwrap();
        let location = fixture::build(rekordbox.path(), Shape::default()).unwrap();
        Setup {
            service: LibraryService::open(data.path(), 2).unwrap(),
            probe: Probe {
                location: Some(location),
                running: false,
                app_version: Some(version.to_owned()),
            },
            _data: data,
            _rekordbox: rekordbox,
        }
    }

    #[test]
    fn it_is_off_until_confirmed_and_reading_is_refused() {
        let setup = setup("7.2.19");
        let state = status(&setup.service, &setup.probe).unwrap();
        assert!(state.installed && !state.enabled && !state.needs_confirmation);
        let error = read(&setup.service, &setup.probe).unwrap_err();
        assert!(error.contains("Settings"), "{error}");
        assert!(backups(&setup.service).is_empty());
    }

    #[test]
    fn reading_backs_up_then_fills_the_playlists() {
        let setup = setup("7.2.19");
        let state = set_enabled(&setup.service, &setup.probe, true).unwrap();
        assert_eq!(state.confirmed_version.as_deref(), Some("7.2.19"));
        let outcome = read(&setup.service, &setup.probe).unwrap();
        assert_eq!(outcome.entries, 40);
        let playlists = setup.service.playlists().unwrap();
        assert_eq!(playlists.iter().filter(|row| !row.is_folder).count(), 3);
        let saved = backups(&setup.service);
        assert_eq!(saved.len(), 1);
        assert_eq!((saved[0].reason.as_str(), saved[0].tracks), ("read", 40));
    }

    #[test]
    fn a_new_version_needs_confirmation_again() {
        let mut setup = setup("7.2.19");
        set_enabled(&setup.service, &setup.probe, true).unwrap();
        setup.probe.app_version = Some("7.2.20".into());
        let state = status(&setup.service, &setup.probe).unwrap();
        assert!(state.enabled && state.needs_confirmation);
        let error = read(&setup.service, &setup.probe).unwrap_err();
        assert!(error.contains("7.2.20"), "{error}");
        set_enabled(&setup.service, &setup.probe, true).unwrap();
        assert!(read(&setup.service, &setup.probe).is_ok());
    }

    #[test]
    fn a_running_rekordbox_is_refused() {
        let mut setup = setup("7.2.19");
        set_enabled(&setup.service, &setup.probe, true).unwrap();
        setup.probe.running = true;
        let error = read(&setup.service, &setup.probe).unwrap_err();
        assert!(error.contains("running"), "{error}");
        assert!(backups(&setup.service).is_empty());
    }

    #[test]
    fn restore_and_pin_use_backup_names_only() {
        let setup = setup("7.2.19");
        set_enabled(&setup.service, &setup.probe, true).unwrap();
        read(&setup.service, &setup.probe).unwrap();
        let name = backups(&setup.service)[0].name.clone();
        assert!(pin(&setup.service, "../elsewhere", true).is_err());
        assert!(pin(&setup.service, &name, true).unwrap()[0].pinned);
        assert!(restore(&setup.service, &setup.probe, "/etc").is_err());
        let saved = restore(&setup.service, &setup.probe, &name).unwrap();
        assert_eq!(saved.reason, "before-restore");
        assert!(saved.pinned);
        assert_eq!(backups(&setup.service).len(), 2);
    }

    #[test]
    fn turning_it_off_clears_the_confirmation() {
        let setup = setup("7.2.19");
        set_enabled(&setup.service, &setup.probe, true).unwrap();
        let state = set_enabled(&setup.service, &setup.probe, false).unwrap();
        assert!(!state.enabled && state.confirmed_version.is_none());
    }
}
