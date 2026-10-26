//! Watch folders: where finished downloads appear. They are read only and never part of
//! the library, so they live in their own table and no file row points into one.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use oc_paths::{bytes_to_os_string, os_str_to_bytes};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::{Database, Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchFolder {
    pub id: i64,
    pub path_raw: Vec<u8>,
    pub label: String,
    pub enabled: bool,
    pub online: bool,
    pub checked_ms: Option<i64>,
    pub last_seen_ms: Option<i64>,
    pub added_ms: i64,
}

pub fn list_watch_folders(connection: &Connection) -> Result<Vec<WatchFolder>> {
    let mut statement = connection.prepare(
        "SELECT id, path_raw, label, enabled, online, checked_ms, last_seen_ms, added_ms \
         FROM watch_folder ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(WatchFolder {
            id: row.get(0)?,
            path_raw: row.get(1)?,
            label: row.get(2)?,
            enabled: row.get(3)?,
            online: row.get(4)?,
            checked_ms: row.get(5)?,
            last_seen_ms: row.get(6)?,
            added_ms: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// The top-level names a folder held when it was added, or `None` for a folder added before
/// such lists were kept.
pub fn watch_baseline(connection: &Connection, id: i64) -> Result<Option<HashSet<Vec<u8>>>> {
    let taken: Option<bool> = connection
        .query_row(
            "SELECT baseline_taken FROM watch_folder WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    if taken != Some(true) {
        return Ok(None);
    }
    let mut statement =
        connection.prepare("SELECT name FROM watch_baseline WHERE watch_folder_id = ?1")?;
    let names = statement
        .query_map([id], |row| row.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(Some(names))
}

/// The names directly inside `path`, read only. Links and all are listed by name; nothing is
/// opened.
pub fn top_level_names(path: &Path) -> std::io::Result<Vec<Vec<u8>>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(path)? {
        names.push(os_str_to_bytes(&entry?.file_name()));
    }
    Ok(names)
}

/// Whether the folder can be listed right now. Read only.
#[must_use]
pub fn is_reachable(folder: &WatchFolder) -> bool {
    bytes_to_os_string(&folder.path_raw).is_ok_and(|path| fs::read_dir(Path::new(&path)).is_ok())
}

impl Database {
    /// Record a folder already validated against the file system and the other folders,
    /// together with the top-level names it holds now (see [`watch_baseline`]).
    pub fn add_watch_folder(
        &mut self,
        path: &Path,
        label: &str,
        baseline: &[Vec<u8>],
        now_ms: i64,
    ) -> Result<i64> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO watch_folder (path_raw, label, enabled, online, checked_ms, \
             last_seen_ms, added_ms, baseline_taken) VALUES (?1, ?2, 1, 1, ?3, ?3, ?3, 1)",
            params![os_str_to_bytes(path.as_os_str()), label, now_ms],
        )?;
        let id = transaction.last_insert_rowid();
        {
            let mut insert = transaction.prepare(
                "INSERT OR IGNORE INTO watch_baseline (watch_folder_id, name) VALUES (?1, ?2)",
            )?;
            for name in baseline {
                insert.execute(params![id, name])?;
            }
        }
        transaction.commit()?;
        Ok(id)
    }

    /// Replace what counts as "already there" with `names`, the folder's contents now.
    pub fn retake_watch_baseline(&mut self, id: i64, names: &[Vec<u8>]) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE watch_folder SET baseline_taken = 1 WHERE id = ?1",
            [id],
        )?;
        if changed != 1 {
            return Err(Error::UnknownWatchFolder(id));
        }
        transaction.execute(
            "DELETE FROM watch_baseline WHERE watch_folder_id = ?1",
            [id],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT OR IGNORE INTO watch_baseline (watch_folder_id, name) VALUES (?1, ?2)",
            )?;
            for name in names {
                insert.execute(params![id, name])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn set_watch_enabled(&mut self, id: i64, enabled: bool) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE watch_folder SET enabled = ?2 WHERE id = ?1",
            params![id, enabled],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(Error::UnknownWatchFolder(id))
        }
    }

    /// Remove a folder from the list. Nothing on disk is touched.
    pub fn remove_watch_folder(&mut self, id: i64) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM watch_baseline WHERE watch_folder_id = ?1",
            [id],
        )?;
        transaction.execute(
            "DELETE FROM imported_release WHERE watch_folder_id = ?1",
            [id],
        )?;
        let changed = transaction.execute("DELETE FROM watch_folder WHERE id = ?1", [id])?;
        transaction.commit()?;
        if changed == 1 {
            Ok(())
        } else {
            Err(Error::UnknownWatchFolder(id))
        }
    }

    pub fn set_watch_online(&mut self, id: i64, online: bool, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE watch_folder SET online = ?2, checked_ms = ?3, \
             last_seen_ms = CASE WHEN ?2 = 1 THEN ?3 ELSE last_seen_ms END WHERE id = ?1",
            params![id, online, now_ms],
        )?;
        Ok(())
    }

    /// Remember that a release was imported, so it leaves the "new" list. Importing it again
    /// updates the record.
    pub fn mark_imported(&mut self, id: i64, name: &[u8], files: usize, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO imported_release (watch_folder_id, name, files, imported_ms) \
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(watch_folder_id, name) DO UPDATE SET \
             files = files + excluded.files, imported_ms = excluded.imported_ms",
            params![id, name, i64::try_from(files).unwrap_or(i64::MAX), now_ms],
        )?;
        Ok(())
    }
}

/// The top-level names imported from a watch folder.
pub fn imported_names(connection: &Connection, id: i64) -> Result<HashSet<Vec<u8>>> {
    let mut statement =
        connection.prepare("SELECT name FROM imported_release WHERE watch_folder_id = ?1")?;
    let names = statement
        .query_map([id], |row| row.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<HashSet<_>>>()?;
    Ok(names)
}
