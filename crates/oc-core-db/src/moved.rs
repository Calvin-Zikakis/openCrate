//! Files moved or renamed outside openCrate. The scan sees a missing file and, at the same time or
//! later, a new file with a track of its own; relinking makes the old track take the new file, so
//! its crate places, own fields, date added and undo history stay, and the new track goes.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::{Database, Error, Result};

/// A missing file and the file it most likely is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub missing_file_id: i64,
    pub track_id: i64,
    pub found_file_id: i64,
    /// The same file on disk (the same inode in the same music folder), not only the same name.
    pub same_file: bool,
}

/// Every missing file with a track, as `(file_id, track_id)`.
pub fn missing(connection: &Connection) -> Result<Vec<(i64, i64)>> {
    let mut statement = connection.prepare(
        "SELECT id, track_id FROM file WHERE presence = 'missing' AND lifecycle = 'active' \
         AND track_id IS NOT NULL ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A missing file as it was: enough to look for it somewhere else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingFile {
    pub file_id: i64,
    pub root_id: i64,
    /// Inside its music folder, as stored.
    pub path_raw: Vec<u8>,
    pub size: i64,
    /// The last part of the path.
    pub name: String,
}

/// Every missing file with its size and name, oldest row first.
pub fn missing_details(connection: &Connection) -> Result<Vec<MissingFile>> {
    let mut statement = connection.prepare(
        "SELECT id, root_id, path_raw, size FROM file WHERE presence = 'missing' \
         AND lifecycle = 'active' AND track_id IS NOT NULL ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        let path_raw: Vec<u8> = row.get(2)?;
        Ok(MissingFile {
            file_id: row.get(0)?,
            root_id: row.get(1)?,
            name: file_name(&path_raw),
            path_raw,
            size: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn file_name(path_raw: &[u8]) -> String {
    let last = path_raw
        .rsplit(|byte| *byte == 0)
        .next()
        .unwrap_or(path_raw);
    String::from_utf8_lossy(last).to_lowercase()
}

/// Missing files and where each most likely is now. A candidate is a present file of the same
/// size that appeared when the old one went missing or later, alone on a track of its own, and
/// either the same file on disk or of the same name. A missing file with two equally good
/// candidates, or a candidate two missing files point at, is left out: a guess there could mix
/// two tracks up.
pub fn moved(connection: &Connection) -> Result<Vec<Moved>> {
    let mut statement = connection.prepare(
        "SELECT m.id, m.track_id, m.root_id, m.path_raw, m.file_token, \
                n.id, n.root_id, n.path_raw, n.file_token \
         FROM file m JOIN file n ON n.size = m.size AND n.id != m.id \
         WHERE m.presence = 'missing' AND m.lifecycle = 'active' AND m.track_id IS NOT NULL \
           AND n.presence = 'present' AND n.lifecycle = 'active' AND n.track_id IS NOT NULL \
           AND n.track_id != m.track_id AND n.created_ms >= m.missing_since_ms \
           AND (SELECT COUNT(*) FROM file o WHERE o.track_id = n.track_id) = 1 \
         ORDER BY m.id, n.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Vec<u8>>(3)?,
            row.get::<_, Option<Vec<u8>>>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, Vec<u8>>(7)?,
            row.get::<_, Option<Vec<u8>>>(8)?,
        ))
    })?;
    let mut best: HashMap<i64, Vec<Moved>> = HashMap::new();
    for row in rows {
        let (missing, track, m_root, m_path, m_token, found, n_root, n_path, n_token) = row?;
        let same_file = m_root == n_root
            && m_token.as_deref().is_some_and(|token| !token.is_empty())
            && m_token == n_token;
        if !same_file && file_name(&m_path) != file_name(&n_path) {
            continue;
        }
        let candidates = best.entry(missing).or_default();
        // The same file on disk beats a file that only has the same name.
        if same_file && candidates.iter().any(|other| !other.same_file) {
            candidates.clear();
        }
        if !same_file && candidates.iter().any(|other| other.same_file) {
            continue;
        }
        candidates.push(Moved {
            missing_file_id: missing,
            track_id: track,
            found_file_id: found,
            same_file,
        });
    }
    let mut found: Vec<Moved> = best
        .into_values()
        .filter(|candidates| candidates.len() == 1)
        .flatten()
        .collect();
    let mut claims: HashMap<i64, usize> = HashMap::new();
    for item in &found {
        *claims.entry(item.found_file_id).or_default() += 1;
    }
    found.retain(|item| claims[&item.found_file_id] == 1);
    found.sort_by_key(|item| item.missing_file_id);
    Ok(found)
}

impl Database {
    /// The missing file `missing` takes the place of the present file `found`: its row takes the
    /// path, and `found`'s row goes. `found`'s track goes too, after its crate places and own
    /// fields pass to the missing file's track where that track does not have them already. The
    /// tags are read again on the next scan. Refused unless `missing` is missing and `found` is
    /// present and alone on a track other than `missing`'s.
    pub fn relink(&mut self, missing: i64, found: i64) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<(String, Option<i64>)> = transaction
            .query_row(
                "SELECT presence, track_id FROM file WHERE id = ?1 AND lifecycle = 'active'",
                [missing],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some(("missing", Some(kept_track))) = old.as_ref().map(|(p, t)| (p.as_str(), *t))
        else {
            return Err(Error::Refused(
                "That track's file is no longer missing.".into(),
            ));
        };
        #[allow(clippy::type_complexity)]
        let new: Option<(
            String,
            Option<i64>,
            i64,
            Vec<u8>,
            Vec<u8>,
            i64,
            i64,
            String,
            Option<Vec<u8>>,
        )> = transaction
            .query_row(
                "SELECT presence, track_id, root_id, path_raw, path_key, size, mtime_ms, \
                     container, file_token FROM file WHERE id = ?1 AND lifecycle = 'active'",
                [found],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .optional()?;
        let Some((presence, new_track, root, path_raw, path_key, size, mtime, container, token)) =
            new
        else {
            return Err(Error::Refused(
                "That file is no longer in the library.".into(),
            ));
        };
        if presence != "present" || new_track == Some(kept_track) {
            return Err(Error::Refused(
                "That file is missing too, or already this track's.".into(),
            ));
        }
        if let Some(track) = new_track {
            let files: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM file WHERE track_id = ?1",
                [track],
                |row| row.get(0),
            )?;
            if files > 1 {
                return Err(Error::Refused(
                    "That file belongs to a track with other files, so it is left as it is.".into(),
                ));
            }
        }

        // What hangs on the found file moves to the kept row first; anything the kept row has
        // already stays.
        for table in ["waveform", "rekordbox_offer", "rekordbox_hidden"] {
            transaction.execute(
                &format!("UPDATE OR IGNORE {table} SET file_id = ?2 WHERE file_id = ?1"),
                params![found, missing],
            )?;
        }
        transaction.execute(
            "UPDATE rb_track SET file_id = ?2, root_id = ?3 WHERE file_id = ?1",
            params![found, missing, root],
        )?;
        transaction.execute("DELETE FROM file WHERE id = ?1", [found])?;
        transaction.execute(
            "UPDATE file SET root_id = ?2, path_raw = ?3, path_key = ?4, size = ?5, mtime_ms = ?6, \
             container = ?7, file_token = ?8, presence = 'present', missing_since_ms = NULL, \
             tags_mtime_ms = NULL WHERE id = ?1",
            params![missing, root, path_raw, path_key, size, mtime, container, token],
        )?;
        if let Some(track) = new_track {
            transaction.execute(
                "UPDATE crate_item SET track_id = ?2 WHERE track_id = ?1 AND NOT EXISTS \
                 (SELECT 1 FROM crate_item other WHERE other.crate_id = crate_item.crate_id \
                  AND other.track_id = ?2)",
                [track, kept_track],
            )?;
            transaction.execute(
                "UPDATE track_field SET track_id = ?2 WHERE track_id = ?1 AND NOT EXISTS \
                 (SELECT 1 FROM track_field other WHERE other.track_id = ?2 \
                  AND other.name = track_field.name)",
                [track, kept_track],
            )?;
            transaction.execute("DELETE FROM track_fts WHERE rowid = ?1", [track])?;
            transaction.execute("DELETE FROM track WHERE id = ?1", [track])?;
            // Its undo history names a track id that a later track may be given.
            crate::edits::forget_track(&transaction, track)?;
        }
        transaction.commit()?;
        Ok(())
    }
}

/// A library file as the owner knows it, present or missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Described {
    pub file_id: i64,
    pub title: String,
    pub artist: String,
    /// The music folder's name.
    pub folder: String,
    /// Inside the music folder, with "/" between the parts.
    pub path: String,
}

/// What the owner needs to tell files apart: track, music folder and path.
pub fn describe(connection: &Connection, file_ids: &[i64]) -> Result<Vec<Described>> {
    let mut statement = connection.prepare(
        "SELECT f.id, COALESCE(t.title, ''), COALESCE(a.name, ''), r.label, f.path_raw \
         FROM file f JOIN library_root r ON r.id = f.root_id \
         LEFT JOIN track t ON t.id = f.track_id LEFT JOIN artist a ON a.id = t.artist_id \
         WHERE f.id = ?1",
    )?;
    let mut found = Vec::with_capacity(file_ids.len());
    for file_id in file_ids {
        let row = statement
            .query_row([file_id], |row| {
                Ok(Described {
                    file_id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    folder: row.get(3)?,
                    path: row
                        .get::<_, Vec<u8>>(4)?
                        .split(|byte| *byte == 0)
                        .map(String::from_utf8_lossy)
                        .collect::<Vec<_>>()
                        .join("/"),
                })
            })
            .optional()?;
        found.extend(row);
    }
    Ok(found)
}
