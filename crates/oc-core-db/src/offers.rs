//! Tracks openCrate filed that rekordbox does not have yet, and which were offered to it.

use std::collections::HashSet;

use oc_rekordbox::location::{decode_location, encode_location};
use oc_rekordbox::xml::{Attributes, Cue, Tempo, Track};
use rusqlite::{Connection, OptionalExtension};

use crate::matching::Matcher;
use crate::{Error, Result};

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

/// One track of a crate, as it can go into openCrate's rekordbox file.
#[derive(Clone, Debug, PartialEq)]
pub enum CrateTrack {
    /// rekordbox has it: its own exported entry, written back exactly.
    Echo(oc_rekordbox::xml::Track),
    /// rekordbox does not have it.
    New(oc_rekordbox::xml::NewTrack),
    /// rekordbox has it, but only a direct database read says so, which has no exact entry to
    /// write back. Reading an XML export first makes it an echo.
    DatabaseOnly { title: String },
    /// None of its files can be reached.
    NoFile { title: String },
}

fn json_attributes(text: &str) -> Result<Attributes> {
    serde_json::from_str(text).map_err(|_| Error::InvalidStoredValue("attributes"))
}

/// rekordbox's own entry for a track, from an XML export in the mirror.
fn echo(connection: &Connection, track_id: i64) -> Result<Option<Track>> {
    let found: Option<(i64, i64, Vec<u8>, String)> = connection
        .query_row(
            "SELECT rt.id, rt.rb_key, rt.location_raw, rt.attributes FROM rb_track rt \
             JOIN rb_source s ON s.id = rt.rb_source_id JOIN file f ON f.id = rt.file_id \
             WHERE f.track_id = ?1 AND s.kind = 'xml' ORDER BY rt.id LIMIT 1",
            [track_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((row, key, location_bytes, attributes)) = found else {
        return Ok(None);
    };
    let attributes = json_attributes(&attributes)?;
    let mut cues = Vec::new();
    let mut statement = connection.prepare_cached(
        "SELECT kind, number, start_seconds, end_seconds, attributes FROM rb_cue \
         WHERE rb_track_id = ?1 ORDER BY position",
    )?;
    let rows = statement.query_map([row], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i32>>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, Option<f64>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for cue in rows {
        let (kind, number, start_seconds, end_seconds, attributes) = cue?;
        cues.push(Cue {
            kind: u8::try_from(kind).map_err(|_| Error::InvalidStoredValue("cue kind"))?,
            number,
            start_seconds,
            end_seconds,
            attributes: json_attributes(&attributes)?,
        });
    }
    let mut tempos = Vec::new();
    let mut statement = connection.prepare_cached(
        "SELECT start_seconds, bpm, meter, beat, attributes FROM rb_tempo \
         WHERE rb_track_id = ?1 ORDER BY position",
    )?;
    let rows = statement.query_map([row], |row| {
        Ok((
            row.get::<_, f64>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<u32>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for tempo in rows {
        let (start_seconds, bpm, meter, beat, attributes) = tempo?;
        tempos.push(Tempo {
            start_seconds,
            bpm,
            meter,
            beat,
            attributes: json_attributes(&attributes)?,
        });
    }
    Ok(Some(Track {
        id: u64::try_from(key).map_err(|_| Error::InvalidStoredValue("TrackID"))?,
        location: attributes.get("Location").cloned().unwrap_or_default(),
        location_bytes,
        attributes,
        cues,
        tempos,
    }))
}

struct CrateItem {
    track_id: i64,
    title: String,
    artist: Option<String>,
    album: Option<String>,
    year: Option<i32>,
}

struct FileRow {
    root: Vec<u8>,
    relative: Vec<u8>,
    size: i64,
    container: String,
    duration_ms: i64,
}

/// A crate's name and its tracks in order, each as it can go into openCrate's rekordbox file.
pub fn crate_tracks(connection: &Connection, crate_id: i64) -> Result<(String, Vec<CrateTrack>)> {
    let name: String = connection
        .query_row("SELECT name FROM crate WHERE id = ?1", [crate_id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or(Error::UnknownCrate(crate_id))?;
    let mut items = connection.prepare(
        "SELECT t.id, t.title, a.name, r.title, r.year FROM crate_item ci \
         JOIN track t ON t.id = ci.track_id \
         LEFT JOIN artist a ON a.id = t.artist_id LEFT JOIN release r ON r.id = t.release_id \
         WHERE ci.crate_id = ?1 ORDER BY ci.position, ci.track_id",
    )?;
    let items: Vec<CrateItem> = items
        .query_map([crate_id], |row| {
            Ok(CrateItem {
                track_id: row.get(0)?,
                title: row.get(1)?,
                artist: row.get(2)?,
                album: row.get(3)?,
                year: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut file = connection.prepare_cached(
        "SELECT r.path_raw, f.path_raw, f.size, f.container, COALESCE(f.duration_ms, 0) \
         FROM file f JOIN library_root r ON r.id = f.root_id \
         WHERE f.track_id = ?1 AND f.presence = 'present' AND f.lifecycle = 'active' \
         AND r.availability = 'available' \
         ORDER BY CASE f.container WHEN 'AIFF' THEN 0 WHEN 'WAV' THEN 0 WHEN 'FLAC' THEN 0 ELSE 1 END, f.id \
         LIMIT 1",
    )?;
    let mut tracks = Vec::with_capacity(items.len());
    for CrateItem {
        track_id,
        title,
        artist,
        album,
        year,
    } in items
    {
        if let Some(entry) = echo(connection, track_id)? {
            tracks.push(CrateTrack::Echo(entry));
            continue;
        }
        let in_database: bool = connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM rb_track rt JOIN rb_source s ON s.id = rt.rb_source_id \
             JOIN file f ON f.id = rt.file_id WHERE f.track_id = ?1 AND s.kind = 'master_db')",
            [track_id],
            |row| row.get(0),
        )?;
        if in_database {
            tracks.push(CrateTrack::DatabaseOnly { title });
            continue;
        }
        let found = file
            .query_row([track_id], |row| {
                Ok(FileRow {
                    root: row.get(0)?,
                    relative: row.get(1)?,
                    size: row.get(2)?,
                    container: row.get(3)?,
                    duration_ms: row.get(4)?,
                })
            })
            .optional()?;
        let Some((row, path)) = found.and_then(|row| {
            let path = crate::catalog::full_path(&row.root, &row.relative)?;
            Some((row, path))
        }) else {
            tracks.push(CrateTrack::NoFile { title });
            continue;
        };
        tracks.push(CrateTrack::New(oc_rekordbox::xml::NewTrack {
            path: path.into_os_string().into_encoded_bytes(),
            name: title,
            artist: artist.unwrap_or_default(),
            album: album.unwrap_or_default(),
            year,
            total_seconds: u32::try_from(row.duration_ms / 1000).unwrap_or(0),
            size: u64::try_from(row.size).unwrap_or(0),
            kind: row.container,
        }));
    }
    Ok((name, tracks))
}
