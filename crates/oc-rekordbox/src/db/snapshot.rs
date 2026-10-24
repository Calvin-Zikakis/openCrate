use std::fs;
use std::path::{Path, PathBuf};

use rbl_db::{DbError, Library, LibraryLocation, OpenMode, SchemaSupport};
use serde::{Deserialize, Serialize};

/// A checked copy of rekordbox's library in its own backup folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub dir: PathBuf,
    pub master_db: PathBuf,
    pub taken_ms: i64,
    pub reason: String,
    pub db_version: Option<i64>,
    pub app_version: Option<String>,
    pub tracks: u32,
    pub playlists: u32,
    #[serde(default)]
    pub pinned: bool,
}

pub(super) const RECORD: &str = "snapshot.json";

/// Copy `master.db` (with its write-ahead log) into a new folder under `backups`, fold the log
/// into the copy and check it. rekordbox must be closed: `running` is the caller's process check.
/// On any failure the new folder is removed.
pub fn take_snapshot(
    location: &LibraryLocation,
    backups: &Path,
    reason: &str,
    running: bool,
    now_ms: i64,
) -> Result<Snapshot, DbError> {
    if running {
        return Err(DbError::WriteRefused(
            "rekordbox is running. Quit it (and wait for its agent to stop) first.".into(),
        ));
    }
    fs::create_dir_all(backups)?;
    let dir = new_folder(backups, now_ms, reason)?;
    let result = copy_and_check(location, &dir, reason, now_ms);
    if result.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    result
}

fn copy_and_check(
    location: &LibraryLocation,
    dir: &Path,
    reason: &str,
    now_ms: i64,
) -> Result<Snapshot, DbError> {
    let master_db = dir.join("master.db");
    fs::copy(&location.master_db, &master_db)?;
    // The log holds committed changes not yet in master.db. The shared-memory index is not
    // copied: SQLite rebuilds it from the log, and a stale one could mislead it.
    let wal = sibling(&location.master_db, "-wal");
    if wal.is_file() {
        fs::copy(&wal, sibling(&master_db, "-wal"))?;
    }
    let (db_version, tracks, playlists) = check(location, &master_db)?;
    let snapshot = Snapshot {
        dir: dir.to_path_buf(),
        master_db,
        taken_ms: now_ms,
        reason: reason.to_owned(),
        db_version,
        app_version: rekordbox_app_version(),
        tracks,
        playlists,
        pinned: false,
    };
    write_record(&snapshot)?;
    Ok(snapshot)
}

/// Keep the library exactly as it is, unchecked: for a library too damaged to snapshot, so a
/// restore can still save what it replaces.
pub(super) fn keep_raw(
    location: &LibraryLocation,
    backups: &Path,
    reason: &str,
    now_ms: i64,
) -> Result<Snapshot, DbError> {
    fs::create_dir_all(backups)?;
    let dir = new_folder(backups, now_ms, reason)?;
    let master_db = dir.join("master.db");
    let copied = (|| -> Result<(), DbError> {
        fs::copy(&location.master_db, &master_db)?;
        for suffix in ["-wal", "-shm"] {
            let from = sibling(&location.master_db, suffix);
            if from.is_file() {
                fs::copy(&from, sibling(&master_db, suffix))?;
            }
        }
        Ok(())
    })();
    if let Err(error) = copied {
        let _ = fs::remove_dir_all(&dir);
        return Err(error);
    }
    let snapshot = Snapshot {
        dir,
        master_db,
        taken_ms: now_ms,
        reason: reason.to_owned(),
        db_version: None,
        app_version: rekordbox_app_version(),
        tracks: 0,
        playlists: 0,
        pinned: true,
    };
    write_record(&snapshot)?;
    Ok(snapshot)
}

/// Open a copy, fold its log in, and prove it whole. Returns the schema version and counts.
pub(super) fn check(
    location: &LibraryLocation,
    copy: &Path,
) -> Result<(Option<i64>, u32, u32), DbError> {
    let library = Library::open(
        LibraryLocation {
            master_db: copy.to_path_buf(),
            share_root: location.share_root.clone(),
            passphrase: location.passphrase.clone(),
            is_real_install: false,
        },
        OpenMode::ReadWrite,
    )?;
    let connection = library.connection();
    connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    let verdict: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if verdict != "ok" {
        return Err(DbError::Schema(format!("the copy is damaged: {verdict}")));
    }
    if let SchemaSupport::Degraded { missing } = &library.schema().support {
        return Err(DbError::Schema(format!(
            "this rekordbox library is missing {}",
            missing.join(", ")
        )));
    }
    let counts = (
        library.schema().db_version,
        library.live_track_count()?,
        library.live_playlist_count()?,
    );
    drop(library);
    // The last connection closing in WAL mode removes the now-empty log.
    let _ = fs::remove_file(sibling(copy, "-shm"));
    let _ = fs::remove_file(sibling(copy, "-wal"));
    Ok(counts)
}

pub(super) fn write_record(snapshot: &Snapshot) -> Result<(), DbError> {
    let text = serde_json::to_string_pretty(snapshot)
        .map_err(|error| DbError::Schema(error.to_string()))?;
    fs::write(snapshot.dir.join(RECORD), text)?;
    Ok(())
}

pub(super) fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn new_folder(backups: &Path, now_ms: i64, reason: &str) -> Result<PathBuf, DbError> {
    let reason: String = reason
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let base = format!("{}-{reason}", folder_stamp(now_ms));
    for attempt in 1..1000 {
        let name = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let dir = backups.join(name);
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(DbError::Open("too many backups with the same time".into()))
}

/// `YYYYMMDD-HHMMSS` in UTC.
pub(super) fn folder_stamp(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The installed rekordbox's version: from its app bundle on macOS, from its install folder's
/// name on Windows.
pub fn rekordbox_app_version() -> Option<String> {
    if cfg!(windows) {
        let bases: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|name| std::env::var_os(name).map(PathBuf::from))
            .collect();
        return windows_install_version(&bases);
    }
    if !cfg!(target_os = "macos") {
        return None;
    }
    let versions = fs::read_dir("/Applications")
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("rekordbox"))
        })
        .filter_map(|folder| {
            let plist = folder.join("rekordbox.app/Contents/Info.plist");
            plist_version(&fs::read_to_string(plist).ok()?)
        });
    newest_version(versions)
}

/// rekordbox installs on Windows into a folder named for its version ("rekordbox 7.2.19"),
/// under Program Files itself or inside a "rekordbox" or "Pioneer" folder there. The newest
/// such folder holding the program wins.
pub(super) fn windows_install_version(bases: &[PathBuf]) -> Option<String> {
    let versions = bases
        .iter()
        .flat_map(|base| [base.clone(), base.join("rekordbox"), base.join("Pioneer")])
        .filter_map(|parent| fs::read_dir(parent).ok())
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter(|entry| entry.path().join("rekordbox.exe").is_file())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version = name.strip_prefix("rekordbox ")?.trim().to_owned();
            version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
                .then_some(version)
        });
    newest_version(versions)
}

/// The highest version, compared part by part as numbers ("10.0" is newer than "7.2").
pub(super) fn newest_version(versions: impl Iterator<Item = String>) -> Option<String> {
    let parts = |version: &str| -> Vec<u64> {
        version
            .split('.')
            .map(|part| part.trim().parse().unwrap_or(0))
            .collect()
    };
    versions.max_by(|a, b| parts(a).cmp(&parts(b)))
}

pub(super) fn plist_version(text: &str) -> Option<String> {
    let after = &text[text.find("<key>CFBundleShortVersionString</key>")?..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")?;
    Some(after[start..start + end].trim().to_owned()).filter(|version| !version.is_empty())
}
