//! A deterministic synthetic library for tests and benchmarks. The shape follows the
//! frontend demo catalog: ten tracks per release, releases spread across 240
//! artists, three formats, a missing tempo on every seventeenth track. Nothing here
//! reads a real library; the paths are inert byte strings under a made-up root.

use oc_paths::{Flavor, NameProfile, RelPath, KEY_SCHEME_VERSION};
use rusqlite::{params, TransactionBehavior};

use crate::library::sort_key;
use crate::{Database, Error, Result};

pub const MAX_TRACKS: u32 = 100_000;
const TRACKS_PER_RELEASE: u32 = 10;
const ARTIST_COUNT: u32 = 240;
/// 2023-11-14 22:13:20 UTC, an arbitrary fixed origin so runs are reproducible.
const EPOCH_MS: i64 = 1_700_000_000_000;

const TITLES: [&str; 8] = [
    "After Hours",
    "Soft Focus",
    "Night Drive",
    "Open Water",
    "Low Orbit",
    "Slow Motion",
    "Morning Light",
    "Parallel Lines",
];
const RELEASES: [&str; 6] = [
    "Between Spaces",
    "Nightfall Studies",
    "Colour Theory",
    "The Long Way Home",
    "Echoes in Motion",
    "Quiet Signals",
];
const ARTISTS: [&str; 4] = ["Studio", "Ensemble", "Project", "Collective"];
const FORMATS: [(&str, &str); 3] = [("AIFF", "aif"), ("MP3", "mp3"), ("FLAC", "flac")];
const KEYS: [&str; 6] = ["8A", "9A", "10A", "11A", "4A", "5A"];

#[must_use]
pub fn artist_name(artist_index: u32) -> String {
    let family = ARTISTS[artist_index as usize % ARTISTS.len()];
    format!("{family} {:03}", artist_index + 1)
}

#[must_use]
pub fn release_title(release_index: u32) -> String {
    let name = RELEASES[release_index as usize % RELEASES.len()];
    format!("{name} {:04}", release_index + 1)
}

#[must_use]
pub fn track_title(track_index: u32) -> String {
    let name = TITLES[track_index as usize % TITLES.len()];
    format!("{name} {:06}", track_index + 1)
}

/// Container of the active file of the zero-based track.
#[must_use]
pub fn track_format(track_index: u32) -> &'static str {
    FORMATS[track_index as usize % FORMATS.len()].0
}

/// Tempo of the zero-based track, absent on every seventeenth.
#[must_use]
pub fn track_bpm(track_index: u32) -> Option<f64> {
    (!track_index.is_multiple_of(17)).then(|| f64::from(110 + track_index % 31))
}

/// Fill an empty database with `tracks` synthetic tracks in one transaction. Ids are
/// one-based: track `n` (zero-based) gets id `n + 1`, as do releases and artists.
pub fn seed(database: &mut Database, tracks: u32) -> Result<()> {
    if tracks > MAX_TRACKS {
        return Err(Error::InvalidSyntheticSize(tracks));
    }
    let releases = tracks.div_ceil(TRACKS_PER_RELEASE);
    let artists = releases.min(ARTIST_COUNT);
    let transaction = database
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "INSERT INTO library_root (id, path_raw, label, kind, name_profile, name_profile_source, \
         key_scheme, availability, created_ms) \
         VALUES (1, ?1, 'Synthetic', 'local', 'exact', 'default', ?2, 'available', ?3)",
        params![b"/synthetic".as_slice(), KEY_SCHEME_VERSION, EPOCH_MS],
    )?;
    {
        let mut artist =
            transaction.prepare("INSERT INTO artist (id, name, sort_key) VALUES (?1, ?2, ?3)")?;
        let mut artist_fts =
            transaction.prepare("INSERT INTO artist_fts (rowid, name) VALUES (?1, ?2)")?;
        for index in 0..artists {
            let (id, name) = (i64::from(index) + 1, artist_name(index));
            artist.execute(params![id, name, sort_key(&name)])?;
            artist_fts.execute(params![id, name])?;
        }
        let mut release = transaction.prepare(
            "INSERT INTO release (id, artist_id, title, sort_key, artist_key, year, added_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut release_fts = transaction
            .prepare("INSERT INTO release_fts (rowid, title, artist) VALUES (?1, ?2, ?3)")?;
        for index in 0..releases {
            let id = i64::from(index) + 1;
            let artist_index = index % ARTIST_COUNT;
            let (title, artist_text) = (release_title(index), artist_name(artist_index));
            release.execute(params![
                id,
                i64::from(artist_index) + 1,
                title,
                sort_key(&title),
                sort_key(&artist_text),
                2000 + i64::from(index % 25),
                EPOCH_MS + i64::from(index) * i64::from(TRACKS_PER_RELEASE) * 1_000,
            ])?;
            release_fts.execute(params![id, title, artist_text])?;
        }
        let mut track = transaction.prepare(
            "INSERT INTO track (id, release_id, artist_id, disc_no, track_no, title, title_key, \
             artist_key, duration_ms, bpm, musical_key, added_ms) \
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        let mut track_fts = transaction.prepare(
            "INSERT INTO track_fts (rowid, title, artist, release, version) \
             VALUES (?1, ?2, ?3, ?4, '')",
        )?;
        let mut file = transaction.prepare(
            "INSERT INTO file (track_id, root_id, path_raw, path_key, lifecycle, presence, size, \
             mtime_ms, container, last_seen_ms, created_ms) \
             VALUES (?1, 1, ?2, ?3, 'active', 'present', ?4, ?5, ?6, ?5, ?5)",
        )?;
        for index in 0..tracks {
            let release_index = index / TRACKS_PER_RELEASE;
            let artist_index = release_index % ARTIST_COUNT;
            let (title, artist_text, release_text) = (
                track_title(index),
                artist_name(artist_index),
                release_title(release_index),
            );
            let id = i64::from(index) + 1;
            let added = EPOCH_MS + i64::from(index) * 1_000;
            let number = index % TRACKS_PER_RELEASE + 1;
            track.execute(params![
                id,
                i64::from(release_index) + 1,
                i64::from(artist_index) + 1,
                number,
                title,
                sort_key(&title),
                sort_key(&artist_text),
                (210 + i64::from(index % 300)) * 1_000,
                track_bpm(index),
                KEYS[index as usize % KEYS.len()],
                added,
            ])?;
            track_fts.execute(params![id, title, artist_text, release_text])?;
            let (container, extension) = FORMATS[index as usize % FORMATS.len()];
            let name = format!("{number:02} - {artist_text} - {title}.{extension}");
            let path = RelPath::from_components(
                [artist_text.as_str(), release_text.as_str(), name.as_str()],
                Flavor::Posix,
            )
            .map_err(|_| Error::InvalidSyntheticSize(tracks))?;
            file.execute(params![
                id,
                path.as_stored(),
                NameProfile::Exact.key(&path).as_bytes(),
                30_000_000 + i64::from(index % 5_000),
                added,
                container,
            ])?;
        }
    }
    transaction.commit()?;
    Ok(())
}
