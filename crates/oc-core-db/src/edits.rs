//! The history of edits made in the track editor: what each save changed, and the values the
//! files held before, so the last save can be undone.

use rusqlite::{params, Connection};

use crate::{Database, Result};

/// One field of one track as a save changed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditItem {
    pub track_id: i64,
    pub field: String,
    pub before: String,
    pub after: String,
}

/// One save, as listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditBatch {
    pub id: i64,
    pub saved_ms: i64,
    pub tracks: usize,
    /// The fields changed, each once, in the order first met.
    pub fields: Vec<String>,
}

/// Every save, newest first.
pub fn history(connection: &Connection) -> Result<Vec<EditBatch>> {
    let mut statement = connection.prepare(
        "SELECT h.id, h.saved_ms, i.track_id, i.field FROM edit_history h \
         JOIN edit_history_item i ON i.history_id = h.id ORDER BY h.id DESC, i.track_id, i.field",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut batches: Vec<EditBatch> = Vec::new();
    let mut tracks: Vec<i64> = Vec::new();
    for row in rows {
        let (id, saved_ms, track_id, field) = row?;
        if batches.last().is_none_or(|batch| batch.id != id) {
            batches.push(EditBatch {
                id,
                saved_ms,
                tracks: 0,
                fields: Vec::new(),
            });
            tracks.clear();
        }
        // The newest batch is the one this row belongs to; one was pushed above if needed.
        let Some(batch) = batches.last_mut() else {
            continue;
        };
        if !tracks.contains(&track_id) {
            tracks.push(track_id);
            batch.tracks += 1;
        }
        if !batch.fields.contains(&field) {
            batch.fields.push(field);
        }
    }
    Ok(batches)
}

impl Database {
    /// Remember one save. Returns its id.
    pub fn record_edits(&mut self, items: &[EditItem], now_ms: i64) -> Result<i64> {
        let transaction = self.connection.transaction()?;
        transaction.execute("INSERT INTO edit_history (saved_ms) VALUES (?1)", [now_ms])?;
        let id = transaction.last_insert_rowid();
        for item in items {
            transaction.execute(
                "INSERT OR REPLACE INTO edit_history_item (history_id, track_id, field, before, after) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, item.track_id, item.field, item.before, item.after],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    /// The newest save and its items; `None` when the history is empty. It stays in the history
    /// until [`Database::forget_edits`], so an undo that fails can be tried again.
    pub fn last_edits(&mut self) -> Result<Option<(i64, Vec<EditItem>)>> {
        let transaction = self.connection.transaction()?;
        let Some(id) = transaction.query_row("SELECT MAX(id) FROM edit_history", [], |row| {
            row.get::<_, Option<i64>>(0)
        })?
        else {
            return Ok(None);
        };
        let items = {
            let mut statement = transaction.prepare(
                "SELECT track_id, field, before, after FROM edit_history_item WHERE history_id = ?1 \
                 ORDER BY track_id, field",
            )?;
            let found = statement
                .query_map([id], |row| {
                    Ok(EditItem {
                        track_id: row.get(0)?,
                        field: row.get(1)?,
                        before: row.get(2)?,
                        after: row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            found
        };
        transaction.commit()?;
        Ok(Some((id, items)))
    }

    /// Drop a save from the history, once it has been undone.
    pub fn forget_edits(&mut self, id: i64) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM edit_history_item WHERE history_id = ?1", [id])?;
        transaction.execute("DELETE FROM edit_history WHERE id = ?1", [id])?;
        transaction.commit()?;
        Ok(())
    }
}

/// A track that is gone takes its part of the undo history with it: track ids are reused, so an
/// undo would otherwise write its old values into another track's files. Saves left empty go too.
pub(crate) fn forget_track(connection: &rusqlite::Connection, track_id: i64) -> Result<()> {
    connection.execute(
        "DELETE FROM edit_history_item WHERE track_id = ?1",
        [track_id],
    )?;
    connection.execute(
        "DELETE FROM edit_history WHERE NOT EXISTS \
         (SELECT 1 FROM edit_history_item i WHERE i.history_id = edit_history.id)",
        [],
    )?;
    Ok(())
}
