use std::fs;
use std::path::{Path, PathBuf};

use rbl_db::{DbError, LibraryLocation};

use super::snapshot::{check, keep_raw, sibling, write_record, RECORD};
use super::{take_snapshot, Snapshot};

/// Every backup folder under `backups` that holds a readable record, newest first.
pub fn list_backups(backups: &Path) -> Vec<Snapshot> {
    let Ok(entries) = fs::read_dir(backups) else {
        return Vec::new();
    };
    let mut found: Vec<Snapshot> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| read_record(&entry.path()))
        .collect();
    found.sort_by(|a, b| b.taken_ms.cmp(&a.taken_ms).then_with(|| b.dir.cmp(&a.dir)));
    found
}

fn read_record(dir: &Path) -> Option<Snapshot> {
    let text = fs::read_to_string(dir.join(RECORD)).ok()?;
    let mut snapshot: Snapshot = serde_json::from_str(&text).ok()?;
    // The folder may have been moved with the app data; trust where it is now.
    snapshot.master_db = dir.join("master.db");
    snapshot.dir = dir.to_path_buf();
    Some(snapshot)
}

/// Remove the oldest unpinned backups beyond `keep`. Returns how many were removed.
pub fn prune(backups: &Path, keep: usize) -> std::io::Result<usize> {
    let mut removed = 0;
    for old in list_backups(backups)
        .into_iter()
        .filter(|backup| !backup.pinned)
        .skip(keep)
    {
        fs::remove_dir_all(&old.dir)?;
        removed += 1;
    }
    Ok(removed)
}

pub fn set_pinned(dir: &Path, pinned: bool) -> std::io::Result<()> {
    let mut snapshot = read_record(dir).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "not an openCrate backup")
    })?;
    snapshot.pinned = pinned;
    write_record(&snapshot).map_err(std::io::Error::other)
}

/// Put a backup back as rekordbox's library. rekordbox must be closed. The backup is checked
/// first, and the library it replaces is saved as a pinned "before-restore" backup, which is
/// returned.
pub fn restore(
    backup: &Path,
    location: &LibraryLocation,
    backups: &Path,
    running: bool,
    now_ms: i64,
) -> Result<Snapshot, DbError> {
    if running {
        return Err(DbError::WriteRefused(
            "rekordbox is running. Quit it (and wait for its agent to stop) before restoring."
                .into(),
        ));
    }
    let incoming = sibling(&location.master_db, ".opencrate-restore");
    let result = restore_through(backup, location, backups, now_ms, &incoming);
    for leftover in [
        incoming.clone(),
        sibling(&incoming, "-wal"),
        sibling(&incoming, "-shm"),
    ] {
        let _ = fs::remove_file(leftover);
    }
    result
}

fn restore_through(
    backup: &Path,
    location: &LibraryLocation,
    backups: &Path,
    now_ms: i64,
    incoming: &PathBuf,
) -> Result<Snapshot, DbError> {
    fs::copy(backup.join("master.db"), incoming)?;
    // A raw backup of a damaged library keeps its log beside it; `check` folds it in.
    let log = backup.join("master.db-wal");
    if log.is_file() {
        fs::copy(&log, sibling(incoming, "-wal"))?;
    }
    check(location, incoming)?;
    // A library too damaged to check is still kept, byte for byte, before it is replaced.
    let mut saved = match take_snapshot(location, backups, "before-restore", false, now_ms) {
        Ok(saved) => saved,
        Err(_) => keep_raw(location, backups, "before-restore-damaged", now_ms)?,
    };
    saved.pinned = true;
    write_record(&saved)?;
    // The old log belongs to the old file: remove it before the restored file takes its place,
    // so it can never be replayed onto the restored one.
    for stale in ["-wal", "-shm"] {
        let path = sibling(&location.master_db, stale);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    fs::rename(incoming, &location.master_db)?;
    Ok(saved)
}
