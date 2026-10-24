//! The owner's own crates: named, ordered lists of library tracks. openCrate keeps them; they are
//! never written to rekordbox. A track appears at most once in a crate.

use rusqlite::{params, Connection, TransactionBehavior};

use crate::{Database, Error, Result};

/// Longest crate name kept, in characters.
pub const MAX_NAME_CHARS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateRow {
    pub id: i64,
    pub name: String,
    pub tracks: i64,
}

pub fn list_crates(connection: &Connection) -> Result<Vec<CrateRow>> {
    let mut statement = connection.prepare(
        "SELECT c.id, c.name, (SELECT COUNT(*) FROM crate_item i WHERE i.crate_id = c.id) \
         FROM crate c ORDER BY c.position, c.id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(CrateRow {
                id: row.get(0)?,
                name: row.get(1)?,
                tracks: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The track ids of a crate in order.
pub fn crate_tracks(connection: &Connection, crate_id: i64) -> Result<Vec<i64>> {
    let mut statement = connection
        .prepare("SELECT track_id FROM crate_item WHERE crate_id = ?1 ORDER BY position")?;
    let ids = statement
        .query_map([crate_id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

fn clean_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_NAME_CHARS
        || trimmed.chars().any(char::is_control)
    {
        return Err(Error::InvalidCrateName);
    }
    Ok(trimmed.to_owned())
}

fn exists(connection: &Connection, crate_id: i64) -> Result<()> {
    let found: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM crate WHERE id = ?1)",
        [crate_id],
        |row| row.get(0),
    )?;
    if found {
        Ok(())
    } else {
        Err(Error::UnknownCrate(crate_id))
    }
}

/// Write `order` as the crate's positions 0, 1, 2...
fn write_order(connection: &Connection, crate_id: i64, order: &[i64]) -> Result<()> {
    let mut update = connection.prepare_cached(
        "UPDATE crate_item SET position = ?3 WHERE crate_id = ?1 AND track_id = ?2",
    )?;
    for (position, track) in order.iter().enumerate() {
        update.execute(params![
            crate_id,
            track,
            i64::try_from(position).unwrap_or(i64::MAX)
        ])?;
    }
    Ok(())
}

impl Database {
    /// Arrange the whole track list by hand. The order shown by `from` is the starting point,
    /// so the first drag keeps everything else where it was; tracks move to just before
    /// `before` (or the end). Afterwards `Sort::Custom` shows this order.
    pub fn move_in_library(
        &mut self,
        track_ids: &[i64],
        before: Option<i64>,
        from: crate::library::Sort,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = crate::library::library_order_ids(&transaction, from)?;
        let order = moved(&current, track_ids, before);
        transaction.execute("DELETE FROM library_order", [])?;
        {
            let mut insert = transaction
                .prepare("INSERT INTO library_order (track_id, position) VALUES (?1, ?2)")?;
            for (position, track) in order.iter().enumerate() {
                insert.execute(params![track, i64::try_from(position).unwrap_or(i64::MAX)])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn create_crate(&mut self, name: &str, now_ms: i64) -> Result<i64> {
        let name = clean_name(name)?;
        self.connection.execute(
            "INSERT INTO crate (name, position, created_ms) \
             VALUES (?1, (SELECT COALESCE(MAX(position) + 1, 0) FROM crate), ?2)",
            params![name, now_ms],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn rename_crate(&mut self, crate_id: i64, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        let changed = self.connection.execute(
            "UPDATE crate SET name = ?2 WHERE id = ?1",
            params![crate_id, name],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(Error::UnknownCrate(crate_id))
        }
    }

    /// Delete a crate. Its tracks stay in the library.
    pub fn delete_crate(&mut self, crate_id: i64) -> Result<()> {
        let changed = self
            .connection
            .execute("DELETE FROM crate WHERE id = ?1", [crate_id])?;
        if changed == 1 {
            Ok(())
        } else {
            Err(Error::UnknownCrate(crate_id))
        }
    }

    /// Append tracks in the given order, skipping ones already in the crate and ids that are not
    /// library tracks. Returns how many were added.
    pub fn add_to_crate(&mut self, crate_id: i64, track_ids: &[i64], now_ms: i64) -> Result<usize> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        exists(&transaction, crate_id)?;
        let mut next: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM crate_item WHERE crate_id = ?1",
            [crate_id],
            |row| row.get(0),
        )?;
        let mut added = 0;
        {
            let mut insert = transaction.prepare(
                "INSERT OR IGNORE INTO crate_item (crate_id, track_id, position, added_ms) \
                 SELECT ?1, id, ?3, ?4 FROM track WHERE id = ?2",
            )?;
            for track in track_ids {
                let inserted = insert.execute(params![crate_id, track, next, now_ms])?;
                added += inserted;
                next += i64::try_from(inserted).unwrap_or(0);
            }
        }
        transaction.commit()?;
        Ok(added)
    }

    /// Take tracks out of a crate (not out of the library). Returns how many were removed.
    pub fn remove_from_crate(&mut self, crate_id: i64, track_ids: &[i64]) -> Result<usize> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        exists(&transaction, crate_id)?;
        let mut removed = 0;
        {
            let mut delete = transaction
                .prepare("DELETE FROM crate_item WHERE crate_id = ?1 AND track_id = ?2")?;
            for track in track_ids {
                removed += delete.execute(params![crate_id, track])?;
            }
        }
        let order = crate_tracks(&transaction, crate_id)?;
        write_order(&transaction, crate_id, &order)?;
        transaction.commit()?;
        Ok(removed)
    }

    /// Move tracks so they sit, in their current relative order, just before `before` (or at the
    /// end when `before` is `None` or is itself being moved past). Ids not in the crate are
    /// ignored. Returns the new order.
    pub fn move_in_crate(
        &mut self,
        crate_id: i64,
        track_ids: &[i64],
        before: Option<i64>,
    ) -> Result<Vec<i64>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        exists(&transaction, crate_id)?;
        let current = crate_tracks(&transaction, crate_id)?;
        let order = moved(&current, track_ids, before);
        write_order(&transaction, crate_id, &order)?;
        transaction.commit()?;
        Ok(order)
    }
}

impl Database {
    /// Put a crate in `order`. Ids not in the crate are ignored; members not in `order` keep
    /// their relative order at the end.
    pub fn set_crate_order(&mut self, crate_id: i64, order: &[i64]) -> Result<Vec<i64>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        exists(&transaction, crate_id)?;
        let current = crate_tracks(&transaction, crate_id)?;
        let mut arranged: Vec<i64> = order
            .iter()
            .copied()
            .filter(|id| current.contains(id))
            .collect();
        let rest: Vec<i64> = current
            .iter()
            .copied()
            .filter(|id| !arranged.contains(id))
            .collect();
        arranged.extend(rest);
        write_order(&transaction, crate_id, &arranged)?;
        transaction.commit()?;
        Ok(arranged)
    }
}

/// The order after moving `moving` before `before`. Pure, so the rule is easy to test.
#[must_use]
pub fn moved(current: &[i64], moving: &[i64], before: Option<i64>) -> Vec<i64> {
    let picked: Vec<i64> = current
        .iter()
        .copied()
        .filter(|id| moving.contains(id))
        .collect();
    let mut rest: Vec<i64> = current
        .iter()
        .copied()
        .filter(|id| !moving.contains(id))
        .collect();
    let at = before
        .and_then(|anchor| rest.iter().position(|id| *id == anchor))
        .unwrap_or(rest.len());
    rest.splice(at..at, picked);
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_keeps_the_relative_order_and_lands_before_the_anchor() {
        let current = [1, 2, 3, 4, 5];
        assert_eq!(moved(&current, &[4], Some(2)), [1, 4, 2, 3, 5]);
        assert_eq!(moved(&current, &[1], None), [2, 3, 4, 5, 1]);
        assert_eq!(
            moved(&current, &[5, 2], Some(1)),
            [2, 5, 1, 3, 4],
            "crate order, not click order"
        );
        assert_eq!(
            moved(&current, &[3], Some(3)),
            [1, 2, 4, 5, 3],
            "an anchor that moves means the end"
        );
        assert_eq!(
            moved(&current, &[9], Some(1)),
            current,
            "unknown ids change nothing"
        );
        assert_eq!(moved(&current, &[2], Some(3)), current, "already in place");
        assert_eq!(moved(&[], &[1], None), Vec::<i64>::new());
    }
}
