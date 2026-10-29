//! What openCrate has imported, file by file, grouped into batches. An import that starts
//! within [`BATCH_GAP_MS`] of the previous one joins its batch, so "Recently imported" shows one
//! sitting's worth without the page having to say where a batch starts or ends. A file is tied
//! to its track through the `file` table by path, once the music folder has been scanned.

use oc_paths::{Flavor, NameProfile, RelPath};
use rusqlite::{params, Connection, TransactionBehavior};

use crate::{Database, Error, Result};

/// Imports further apart than this start a new batch: a quarter of an hour.
pub const BATCH_GAP_MS: i64 = 15 * 60_000;

/// Batches listed, newest first.
pub const BATCHES_SHOWN: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportBatchRow {
    pub id: i64,
    pub started_ms: i64,
    pub last_ms: i64,
    /// Files filed in this batch.
    pub files: i64,
    /// Of those, the ones the catalog knows as tracks (scanned since).
    pub tracks: i64,
}

/// What recording an import gives back: the batch it joined and the row of each file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    pub batch_id: i64,
    pub file_ids: Vec<i64>,
}

const TRACK_OF_FILE: &str = "(SELECT f.track_id FROM file f WHERE f.root_id = b.root_id \
     AND f.path_key = b.path_key AND f.lifecycle = 'active' AND f.track_id IS NOT NULL \
     ORDER BY f.id LIMIT 1)";

pub fn list_batches(connection: &Connection) -> Result<Vec<ImportBatchRow>> {
    let mut statement = connection.prepare(&format!(
        "SELECT i.id, i.started_ms, i.last_ms, \
         (SELECT COUNT(*) FROM import_file b WHERE b.batch_id = i.id), \
         (SELECT COUNT(*) FROM import_file b WHERE b.batch_id = i.id \
          AND {TRACK_OF_FILE} IS NOT NULL) \
         FROM import_batch i ORDER BY i.last_ms DESC, i.id DESC LIMIT ?1"
    ))?;
    let rows = statement
        .query_map([i64::try_from(BATCHES_SHOWN).unwrap_or(i64::MAX)], |row| {
            Ok(ImportBatchRow {
                id: row.get(0)?,
                started_ms: row.get(1)?,
                last_ms: row.get(2)?,
                files: row.get(3)?,
                tracks: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The track ids behind these import rows, in the rows' order, leaving out files the catalog
/// does not know yet. Unknown ids are skipped.
pub fn tracks_of_files(connection: &Connection, file_ids: &[i64]) -> Result<Vec<i64>> {
    let mut statement = connection.prepare_cached(&format!(
        "SELECT {TRACK_OF_FILE} FROM import_file b WHERE b.id = ?1"
    ))?;
    let mut tracks = Vec::with_capacity(file_ids.len());
    for id in file_ids {
        let found: Option<Option<i64>> = statement
            .query_row([id], |row| row.get(0))
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        if let Some(Some(track)) = found {
            if !tracks.contains(&track) {
                tracks.push(track);
            }
        }
    }
    Ok(tracks)
}

/// The path key of a file below a music folder, as the scan stores it, so the two meet.
fn key_for(connection: &Connection, root_id: i64, components: &[Vec<u8>]) -> Result<Vec<u8>> {
    let profile = connection.query_row(
        "SELECT name_profile FROM library_root WHERE id = ?1",
        [root_id],
        |row| row.get::<_, String>(0),
    )?;
    let profile = NameProfile::parse(&profile).ok_or(Error::InvalidStoredValue("name profile"))?;
    let path = RelPath::from_components(components, Flavor::Posix)
        .map_err(|_| Error::InvalidStoredValue("path"))?;
    Ok(profile.key(&path).as_bytes().to_vec())
}

impl Database {
    /// Remember files just filed under a music folder, each given by its components below the
    /// folder. They join the newest batch when it is recent enough, else a new one.
    pub fn record_import(
        &mut self,
        root_id: i64,
        files: &[Vec<Vec<u8>>],
        now_ms: i64,
    ) -> Result<Recorded> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let newest: Option<(i64, i64)> = {
            use rusqlite::OptionalExtension;
            transaction
                .query_row(
                    "SELECT id, last_ms FROM import_batch ORDER BY last_ms DESC, id DESC LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
        };
        let batch_id = match newest {
            Some((id, last_ms)) if now_ms - last_ms <= BATCH_GAP_MS && now_ms >= last_ms => {
                transaction.execute(
                    "UPDATE import_batch SET last_ms = ?2 WHERE id = ?1",
                    params![id, now_ms],
                )?;
                id
            }
            _ => {
                transaction.execute(
                    "INSERT INTO import_batch (started_ms, last_ms) VALUES (?1, ?1)",
                    [now_ms],
                )?;
                transaction.last_insert_rowid()
            }
        };
        let mut file_ids = Vec::with_capacity(files.len());
        for components in files {
            let key = key_for(&transaction, root_id, components)?;
            transaction.execute(
                "INSERT INTO import_file (batch_id, root_id, path_key, imported_ms) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![batch_id, root_id, key, now_ms],
            )?;
            file_ids.push(transaction.last_insert_rowid());
        }
        transaction.commit()?;
        Ok(Recorded { batch_id, file_ids })
    }
}
