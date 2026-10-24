//! Custom fields: the owner's own named values on tracks, kept in the database.

use rusqlite::{params, Connection};

use crate::{Database, Error, Result};

const MAX_NAME_CHARS: usize = 60;
const MAX_VALUE_CHARS: usize = 1000;

/// Every custom field on the given tracks: (track, name, value), by track then name.
pub fn fields_of(connection: &Connection, track_ids: &[i64]) -> Result<Vec<(i64, String, String)>> {
    let mut statement = connection
        .prepare("SELECT name, value FROM track_field WHERE track_id = ?1 ORDER BY name")?;
    let mut found = Vec::new();
    for &track_id in track_ids {
        for row in statement.query_map([track_id], |row| Ok((row.get(0)?, row.get(1)?)))? {
            let (name, value): (String, String) = row?;
            found.push((track_id, name, value));
        }
    }
    Ok(found)
}

/// Every field name in use, with the most used first.
pub fn names(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT name FROM track_field GROUP BY name ORDER BY COUNT(*) DESC, name")?;
    let found = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(found)
}

impl Database {
    /// Set one field on tracks; an empty value removes it. Names and values are trimmed.
    pub fn set_field(&mut self, track_ids: &[i64], name: &str, value: &str) -> Result<()> {
        let name = name.trim();
        let value = value.trim();
        if name.is_empty()
            || name.chars().count() > MAX_NAME_CHARS
            || name.chars().any(char::is_control)
        {
            return Err(Error::InvalidStoredValue(
                "a custom field needs a short, plain name",
            ));
        }
        if value.chars().count() > MAX_VALUE_CHARS {
            return Err(Error::InvalidStoredValue(
                "a custom field's value is too long",
            ));
        }
        let transaction = self.connection.transaction()?;
        for &track_id in track_ids {
            if value.is_empty() {
                transaction.execute(
                    "DELETE FROM track_field WHERE track_id = ?1 AND name = ?2",
                    params![track_id, name],
                )?;
            } else {
                transaction.execute(
                    "INSERT INTO track_field (track_id, name, value) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (track_id, name) DO UPDATE SET value = excluded.value",
                    params![track_id, name, value],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}
