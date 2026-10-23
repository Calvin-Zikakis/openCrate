//! Tracks openCrate filed that rekordbox does not have yet, and which were offered to it.

use std::collections::HashSet;

use oc_rekordbox::location::{decode_location, encode_location};
use rusqlite::{Connection, OptionalExtension};

use crate::matching::Matcher;
use crate::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub file_id: i64,
    /// Where it was filed, as recorded when it was published.
    pub path: Vec<u8>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    pub duration_ms: i64,
    pub size: i64,
    pub container: String,
    pub filed_ms: i64,
    pub offered_ms: Option<i64>,
}

/// Files openCrate published into the library that are still there and that no rekordbox entry
/// in the mirror matches, newest filing first.
pub fn candidates(connection: &Connection) -> Result<Vec<Candidate>> {
    let matcher = Matcher::load(connection)?;
    let mut filed = connection.prepare(
        "SELECT destination_raw, MAX(updated_ms) FROM publish_operation \
         WHERE phase IN ('recorded', 'cleaned') GROUP BY destination_raw",
    )?;
    let filed: Vec<(Vec<u8>, i64)> = filed
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut details = connection.prepare_cached(
        "SELECT t.title, a.name, r.title, r.year, COALESCE(f.duration_ms, t.duration_ms, 0), \
                f.size, f.container, o.offered_ms \
         FROM file f \
         LEFT JOIN track t ON t.id = f.track_id \
         LEFT JOIN artist a ON a.id = t.artist_id \
         LEFT JOIN release r ON r.id = t.release_id \
         LEFT JOIN rekordbox_offer o ON o.file_id = f.id \
         WHERE f.id = ?1 AND f.lifecycle = 'active' AND f.presence = 'present' \
           AND f.id NOT IN (SELECT file_id FROM rb_track WHERE file_id IS NOT NULL)",
    )?;
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for (destination, filed_ms) in filed {
        // The same form an XML Location decodes to, so Windows paths match too.
        let Ok(decoded) = decode_location(&encode_location(&destination)) else {
            continue;
        };
        let Some(file_id) = matcher.match_location(connection, &decoded)?.file_id else {
            continue;
        };
        if !seen.insert(file_id) {
            continue;
        }
        let row = details
            .query_row([file_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i32>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })
            .optional()?;
        let Some((title, artist, album, year, duration_ms, size, container, offered_ms)) = row
        else {
            continue;
        };
        found.push(Candidate {
            file_id,
            title: title.unwrap_or_else(|| file_stem(&destination)),
            path: destination,
            artist: artist.unwrap_or_default(),
            album: album.unwrap_or_default(),
            year,
            duration_ms,
            size,
            container,
            filed_ms,
            offered_ms,
        });
    }
    found.sort_by(|a, b| b.filed_ms.cmp(&a.filed_ms).then(b.file_id.cmp(&a.file_id)));
    Ok(found)
}

/// A name to show for a file whose tags were not read.
fn file_stem(path: &[u8]) -> String {
    let name = path
        .rsplit(|byte| *byte == b'/' || *byte == b'\\')
        .next()
        .unwrap_or(path);
    let stem = name
        .iter()
        .rposition(|byte| *byte == b'.')
        .map_or(name, |dot| &name[..dot]);
    String::from_utf8_lossy(stem).into_owned()
}
