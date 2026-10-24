//! Tracks openCrate filed that rekordbox does not have yet, and which were offered to it.

use std::collections::{HashMap, HashSet};

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
    /// The owner chose to keep it out of rekordbox.
    pub hidden: bool,
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
                f.size, f.container, o.offered_ms, h.file_id IS NOT NULL \
         FROM file f \
         LEFT JOIN track t ON t.id = f.track_id \
         LEFT JOIN artist a ON a.id = t.artist_id \
         LEFT JOIN release r ON r.id = t.release_id \
         LEFT JOIN rekordbox_offer o ON o.file_id = f.id \
         LEFT JOIN rekordbox_hidden h ON h.file_id = f.id \
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
                    row.get::<_, bool>(8)?,
                ))
            })
            .optional()?;
        let Some((title, artist, album, year, duration_ms, size, container, offered_ms, hidden)) =
            row
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
            hidden,
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

/// Cues and tempos by mirrored row.
type Analysis = (HashMap<i64, Vec<Cue>>, HashMap<i64, Vec<Tempo>>);

/// The decoded cues and tempos of every mirrored entry in a crate.
fn analysis(connection: &Connection, crate_id: i64) -> Result<Analysis> {
    // Every mirrored row (from an XML export) for a track in the crate.
    const ROWS: &str = "SELECT rt.id FROM rb_track rt \
         JOIN rb_source s ON s.id = rt.rb_source_id AND s.kind = 'xml' \
         JOIN file f ON f.id = rt.file_id \
         JOIN crate_item ci ON ci.track_id = f.track_id WHERE ci.crate_id = ?1";
    let mut cues: HashMap<i64, Vec<Cue>> = HashMap::new();
    let mut statement = connection.prepare_cached(&format!(
        "SELECT rb_track_id, kind, number, start_seconds, end_seconds, attributes FROM rb_cue \
         WHERE rb_track_id IN ({ROWS}) ORDER BY rb_track_id, position"
    ))?;
    let rows = statement.query_map([crate_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<i32>>(2)?,
            row.get::<_, f64>(3)?,
            row.get::<_, Option<f64>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (track_row, kind, number, start_seconds, end_seconds, attributes) = row?;
        cues.entry(track_row).or_default().push(Cue {
            kind: u8::try_from(kind).map_err(|_| Error::InvalidStoredValue("cue kind"))?,
            number,
            start_seconds,
            end_seconds,
            attributes: json_attributes(&attributes)?,
        });
    }
    let mut tempos: HashMap<i64, Vec<Tempo>> = HashMap::new();
    let mut statement = connection.prepare_cached(&format!(
        "SELECT rb_track_id, start_seconds, bpm, meter, beat, attributes FROM rb_tempo \
         WHERE rb_track_id IN ({ROWS}) ORDER BY rb_track_id, position"
    ))?;
    let rows = statement.query_map([crate_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<u32>>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    for row in rows {
        let (track_row, start_seconds, bpm, meter, beat, attributes) = row?;
        tempos.entry(track_row).or_default().push(Tempo {
            start_seconds,
            bpm,
            meter,
            beat,
            attributes: json_attributes(&attributes)?,
        });
    }
    Ok((cues, tempos))
}

/// rekordbox's own entries, from an XML export in the mirror, for every track in a crate that
/// has one: track id to entry. One mirrored row per track is taken, the lowest.
fn echoes(connection: &Connection, crate_id: i64) -> Result<HashMap<i64, Track>> {
    let (mut cues, mut tempos) = analysis(connection, crate_id)?;
    let mut statement = connection.prepare_cached(
        "SELECT f.track_id, rt.id, rt.rb_key, rt.location_raw, rt.attributes FROM rb_track rt \
         JOIN rb_source s ON s.id = rt.rb_source_id AND s.kind = 'xml' \
         JOIN file f ON f.id = rt.file_id \
         JOIN crate_item ci ON ci.track_id = f.track_id \
         WHERE ci.crate_id = ?1 ORDER BY f.track_id, rt.id",
    )?;
    let rows = statement.query_map([crate_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Vec<u8>>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut found: HashMap<i64, Track> = HashMap::new();
    for row in rows {
        let (track_id, track_row, key, location_bytes, attributes) = row?;
        if found.contains_key(&track_id) {
            continue;
        }
        let attributes = json_attributes(&attributes)?;
        found.insert(
            track_id,
            Track {
                id: u64::try_from(key).map_err(|_| Error::InvalidStoredValue("TrackID"))?,
                location: attributes.get("Location").cloned().unwrap_or_default(),
                location_bytes,
                attributes,
                cues: cues.remove(&track_row).unwrap_or_default(),
                tempos: tempos.remove(&track_row).unwrap_or_default(),
            },
        );
    }
    Ok(found)
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
/// A handful of queries per crate, however many tracks it holds.
pub fn crate_tracks(connection: &Connection, crate_id: i64) -> Result<(String, Vec<CrateTrack>)> {
    let name: String = connection
        .query_row("SELECT name FROM crate WHERE id = ?1", [crate_id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or(Error::UnknownCrate(crate_id))?;
    let mut items = connection.prepare_cached(
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
    let mut echoes = echoes(connection, crate_id)?;
    let mut in_database = connection.prepare_cached(
        "SELECT DISTINCT f.track_id FROM rb_track rt \
         JOIN rb_source s ON s.id = rt.rb_source_id AND s.kind = 'master_db' \
         JOIN file f ON f.id = rt.file_id \
         JOIN crate_item ci ON ci.track_id = f.track_id WHERE ci.crate_id = ?1",
    )?;
    let in_database: HashSet<i64> = in_database
        .query_map([crate_id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut file_rows = connection.prepare_cached(
        "SELECT f.track_id, r.path_raw, f.path_raw, f.size, f.container, COALESCE(f.duration_ms, 0) \
         FROM file f JOIN library_root r ON r.id = f.root_id \
         JOIN crate_item ci ON ci.track_id = f.track_id \
         WHERE ci.crate_id = ?1 AND f.presence = 'present' AND f.lifecycle = 'active' \
         AND r.availability = 'available' \
         ORDER BY f.track_id, CASE f.container WHEN 'AIFF' THEN 0 WHEN 'WAV' THEN 0 WHEN 'FLAC' THEN 0 ELSE 1 END, f.id",
    )?;
    let mut files: HashMap<i64, FileRow> = HashMap::new();
    for row in file_rows.query_map([crate_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            FileRow {
                root: row.get(1)?,
                relative: row.get(2)?,
                size: row.get(3)?,
                container: row.get(4)?,
                duration_ms: row.get(5)?,
            },
        ))
    })? {
        let (track_id, file) = row?;
        files.entry(track_id).or_insert(file);
    }
    let mut tracks = Vec::with_capacity(items.len());
    for CrateItem {
        track_id,
        title,
        artist,
        album,
        year,
    } in items
    {
        if let Some(entry) = echoes.remove(&track_id) {
            tracks.push(CrateTrack::Echo(entry));
            continue;
        }
        if in_database.contains(&track_id) {
            tracks.push(CrateTrack::DatabaseOnly { title });
            continue;
        }
        let Some((row, path)) = files.remove(&track_id).and_then(|row| {
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

/// Where a library track stands in rekordbox's library, from the last direct read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RekordboxMember {
    /// rekordbox has one of its files, under this id.
    Known(String),
    /// rekordbox does not have it; this is the file to add.
    File(std::path::PathBuf),
    /// It has no file that can be reached, so it cannot be added.
    NoFile,
}

/// rekordbox's id for a playlist or folder, when it came from a direct read of rekordbox's
/// library. An export's playlists have none: only a database read names rekordbox's rows.
pub fn rekordbox_playlist_key(connection: &Connection, playlist_id: i64) -> Result<Option<String>> {
    Ok(connection
        .query_row(
            "SELECT json_extract(p.attributes, '$.' || ?2) FROM playlist p
             JOIN rb_source s ON s.id = p.rb_source_id
             WHERE p.id = ?1 AND s.kind = 'master_db'",
            rusqlite::params![playlist_id, oc_rekordbox::db::REKORDBOX_ID],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// rekordbox's id for a track when a direct read matched one of its files, if any.
pub fn rekordbox_key(connection: &Connection, track_id: i64) -> Result<Option<String>> {
    Ok(connection
        .query_row(
            "SELECT t.rb_key FROM rb_track t
             JOIN rb_source s ON s.id = t.rb_source_id
             JOIN file f ON f.id = t.file_id
             WHERE f.track_id = ?1 AND s.kind = 'master_db' AND t.match_status = 'matched'
             ORDER BY t.rb_key LIMIT 1",
            [track_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|key| key.to_string()))
}

/// rekordbox's id for a library file, when a direct read matched it.
pub fn rekordbox_file_key(connection: &Connection, file_id: i64) -> Result<Option<String>> {
    Ok(connection
        .query_row(
            "SELECT t.rb_key FROM rb_track t JOIN rb_source s ON s.id = t.rb_source_id
             WHERE t.file_id = ?1 AND s.kind = 'master_db' AND t.match_status = 'matched'
             ORDER BY t.rb_key LIMIT 1",
            [file_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|key| key.to_string()))
}
