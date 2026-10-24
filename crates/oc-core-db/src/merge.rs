//! The three-way merge with rekordbox. rekordbox does not record when a field was edited, so,
//! like Git, openCrate keeps a base: each synced field as both sides last agreed on it. A side
//! that differs from the base has changed; when both have, to different values, the owner
//! chooses. Only tracks a direct read of rekordbox's library matched to a library file take part.

use rusqlite::{params, Connection};

use crate::{Database, Result};

/// The fields both sides hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Title,
    Artist,
    Album,
    Year,
    Label,
}

impl Field {
    pub const ALL: [Self; 5] = [
        Self::Title,
        Self::Artist,
        Self::Album,
        Self::Year,
        Self::Label,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Year => "year",
            Self::Label => "label",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.as_str() == name)
    }
}

/// Which way a change goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Changed in openCrate only.
    ToRekordbox,
    /// Changed in rekordbox only.
    ToOpenCrate,
    /// Changed on both sides to different values: the owner chooses.
    Conflict,
}

/// Where one field stands. `None` when the two sides agree. With no base yet (the first sync),
/// rekordbox's value is the starting point: openCrate's is offered only where rekordbox has none.
#[must_use]
pub fn classify(base: Option<&str>, ours: &str, theirs: &str) -> Option<Direction> {
    if ours == theirs {
        return None;
    }
    Some(match base {
        None if theirs.is_empty() => Direction::ToRekordbox,
        None => Direction::ToOpenCrate,
        Some(base) if ours == base => Direction::ToOpenCrate,
        Some(base) if theirs == base => Direction::ToRekordbox,
        Some(_) => Direction::Conflict,
    })
}

/// One field of one track that differs between the two sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// rekordbox's id for the track.
    pub rb_key: i64,
    pub track_id: i64,
    /// The library file rekordbox's entry points at.
    pub file_id: i64,
    /// openCrate's title, to name the track.
    pub title: String,
    pub artist: String,
    pub field: Field,
    pub base: Option<String>,
    pub ours: String,
    pub theirs: String,
    pub direction: Direction,
}

/// A text value as compared: trimmed, and a year of 0 is no year.
fn clean(value: Option<String>) -> String {
    let value = value.unwrap_or_default().trim().to_owned();
    if value == "0" {
        String::new()
    } else {
        value
    }
}

struct Pair {
    rb_key: i64,
    track_id: i64,
    file_id: i64,
    ours: [String; 5],
    theirs: [String; 5],
}

/// Every library track a direct read of rekordbox matched, with both sides' values.
fn pairs(connection: &Connection) -> Result<Vec<Pair>> {
    let mut statement = connection.prepare(
        "SELECT t.rb_key, tr.id, f.id,
                tr.title, a.name, r.title, r.year, r.label,
                t.name, t.artist, json_extract(t.attributes, '$.Album'),
                json_extract(t.attributes, '$.Year'), json_extract(t.attributes, '$.Label')
         FROM rb_track t
         JOIN rb_source s ON s.id = t.rb_source_id AND s.kind = 'master_db'
         JOIN file f ON f.id = t.file_id AND f.presence = 'present' AND f.lifecycle = 'active'
         JOIN track tr ON tr.id = f.track_id
         JOIN artist a ON a.id = tr.artist_id
         JOIN release r ON r.id = tr.release_id
         WHERE t.match_status = 'matched'
         ORDER BY t.rb_key",
    )?;
    let rows = statement.query_map([], |row| {
        let text = |index: usize| -> rusqlite::Result<String> {
            Ok(clean(row.get::<_, Option<String>>(index)?))
        };
        let year = row.get::<_, Option<i64>>(6)?.map(|year| year.to_string());
        Ok(Pair {
            rb_key: row.get(0)?,
            track_id: row.get(1)?,
            file_id: row.get(2)?,
            ours: [text(3)?, text(4)?, text(5)?, clean(year), text(7)?],
            theirs: [text(8)?, text(9)?, text(10)?, text(11)?, text(12)?],
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn base(connection: &Connection, rb_key: i64, field: Field) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row(
            "SELECT value FROM merge_base WHERE rb_key = ?1 AND field = ?2",
            params![rb_key, field.as_str()],
            |row| row.get(0),
        )
        .optional()?)
}

/// Record `value` as what both sides agree on for one field.
pub fn agree(
    connection: &Connection,
    rb_key: i64,
    field: Field,
    value: &str,
    now_ms: i64,
) -> Result<()> {
    connection.execute(
        "INSERT INTO merge_base (rb_key, field, value, agreed_ms) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (rb_key, field) DO UPDATE SET value = excluded.value, agreed_ms = excluded.agreed_ms
         WHERE value <> excluded.value",
        params![rb_key, field.as_str(), value, now_ms],
    )?;
    Ok(())
}

/// What differs between openCrate and rekordbox, by track and field. Fields that agree are
/// recorded as the base on the way, so a later edit on one side is recognised as one-sided.
pub fn review(connection: &Connection, now_ms: i64) -> Result<Vec<Change>> {
    let mut changes = Vec::new();
    for pair in pairs(connection)? {
        for (index, field) in Field::ALL.into_iter().enumerate() {
            let (ours, theirs) = (&pair.ours[index], &pair.theirs[index]);
            if ours == theirs {
                agree(connection, pair.rb_key, field, ours, now_ms)?;
                continue;
            }
            let base = base(connection, pair.rb_key, field)?;
            if let Some(direction) = classify(base.as_deref(), ours, theirs) {
                changes.push(Change {
                    rb_key: pair.rb_key,
                    track_id: pair.track_id,
                    file_id: pair.file_id,
                    title: pair.ours[0].clone(),
                    artist: pair.ours[1].clone(),
                    field,
                    base,
                    ours: ours.clone(),
                    theirs: theirs.clone(),
                    direction,
                });
            }
        }
    }
    Ok(changes)
}

impl Database {
    pub fn merge_review(&mut self, now_ms: i64) -> Result<Vec<Change>> {
        let transaction = self.connection.transaction()?;
        let changes = review(&transaction, now_ms)?;
        transaction.commit()?;
        Ok(changes)
    }

    pub fn record_merge_base(
        &mut self,
        agreed: &[(i64, Field, String)],
        now_ms: i64,
    ) -> Result<()> {
        let transaction = self.connection.transaction()?;
        for (rb_key, field, value) in agreed {
            agree(&transaction, *rb_key, *field, value, now_ms)?;
        }
        transaction.commit()?;
        Ok(())
    }
}
