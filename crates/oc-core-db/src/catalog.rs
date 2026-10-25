//! From scanned files to the browsable catalog: read each new or changed file's tags away
//! from the writer, then create or update its artist, release and track in one transaction.
//!
//! Tags fill what they have and the library layout (`Artist/Release/NN - Artist - Title`)
//! fills the rest, so an untagged or unreadable file still shows up. Tempo and key are not
//! taken from tags: rekordbox owns them and the read-back fills them (A21).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use oc_paths::{bytes_to_os_string, Flavor, RelPath};
use oc_tag::{guess_from_path, read_file, FileInfo, TagInfo};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::library::sort_key;
use crate::{Database, Error, Result};

const UNKNOWN_ARTIST: &str = "Unknown Artist";
const UNKNOWN_RELEASE: &str = "Unknown Release";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingTagRead {
    pub file_id: i64,
    pub root_path_raw: Vec<u8>,
    pub path_raw: Vec<u8>,
    pub mtime_ms: i64,
}

/// How many present, active files still need their tags read.
pub fn pending_tag_total(connection: &Connection) -> Result<usize> {
    let total: i64 = connection.query_row(
        "SELECT COUNT(*) FROM file WHERE lifecycle = 'active' AND presence = 'present' \
         AND (tags_mtime_ms IS NULL OR tags_mtime_ms != mtime_ms)",
        [],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(total).unwrap_or(0))
}

/// Present, active files whose tags were never read or were read for an older version of
/// the file.
pub fn pending_tag_reads(connection: &Connection, limit: usize) -> Result<Vec<PendingTagRead>> {
    let mut statement = connection.prepare(
        "SELECT f.id, r.path_raw, f.path_raw, f.mtime_ms FROM file f \
         JOIN library_root r ON r.id = f.root_id \
         WHERE f.lifecycle = 'active' AND f.presence = 'present' \
           AND (f.tags_mtime_ms IS NULL OR f.tags_mtime_ms != f.mtime_ms) \
         ORDER BY f.id LIMIT ?1",
    )?;
    let rows = statement.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
        Ok(PendingTagRead {
            file_id: row.get(0)?,
            root_path_raw: row.get(1)?,
            path_raw: row.get(2)?,
            mtime_ms: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[derive(Clone, Debug, PartialEq)]
pub struct TagResult {
    pub file_id: i64,
    /// The modified time the file had when it was queued; a changed file is not applied.
    pub mtime_ms: i64,
    pub outcome: std::result::Result<FileInfo, String>,
}

pub(crate) fn full_path(root: &[u8], relative: &[u8]) -> Option<PathBuf> {
    let mut path = PathBuf::from(bytes_to_os_string(root).ok()?);
    for component in relative.split(|byte| *byte == 0) {
        path.push(bytes_to_os_string(component).ok()?);
    }
    Some(path)
}

fn read_one(item: &PendingTagRead) -> TagResult {
    let outcome = match full_path(&item.root_path_raw, &item.path_raw) {
        None => Err("path is not representable on this system".to_owned()),
        // One file that crashes the reader must not take its neighbours down with it.
        Some(path) => catch_unwind(AssertUnwindSafe(|| read_file(&path)))
            .map_err(|_| "the tag reader failed on this file".to_owned())
            .and_then(|read| read.map_err(|error| error.to_string())),
    };
    TagResult {
        file_id: item.file_id,
        mtime_ms: item.mtime_ms,
        outcome,
    }
}

/// Read tags with a few threads. Does file I/O, so call it away from the writer.
#[must_use]
pub fn read_tags(pending: &[PendingTagRead], threads: usize) -> Vec<TagResult> {
    let next = AtomicUsize::new(0);
    let mut results: Vec<TagResult> = thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.clamp(1, 16))
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    while let Some(item) = pending.get(next.fetch_add(1, Ordering::Relaxed)) {
                        done.push(read_one(item));
                    }
                    done
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    });
    results.sort_by_key(|result| result.file_id);
    results
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogOutcome {
    pub tracks_created: usize,
    pub tracks_updated: usize,
    pub artists_created: usize,
    pub releases_created: usize,
    /// Files whose tags could not be read; they are catalogued from their path instead.
    pub tag_errors: usize,
    /// Results for files that changed or vanished after being queued; read again next time.
    pub stale: usize,
}

impl CatalogOutcome {
    pub fn add(&mut self, other: &Self) {
        self.tracks_created += other.tracks_created;
        self.tracks_updated += other.tracks_updated;
        self.artists_created += other.artists_created;
        self.releases_created += other.releases_created;
        self.tag_errors += other.tag_errors;
        self.stale += other.stale;
    }
}

struct FileRow {
    track_id: Option<i64>,
    path_raw: Vec<u8>,
    mtime_ms: i64,
}

fn find_or_create_artist(
    connection: &Connection,
    name: &str,
    outcome: &mut CatalogOutcome,
) -> Result<i64> {
    let key = sort_key(name);
    if let Some((id, stored)) = connection
        .query_row(
            "SELECT id, name FROM artist WHERE sort_key = ?1",
            [&key],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        // The same artist spelled anew ("daft punk" retagged "Daft Punk"): the newest read
        // spelling is the one shown and searched.
        if stored != name {
            connection.execute(
                "UPDATE artist SET name = ?2 WHERE id = ?1",
                params![id, name],
            )?;
            connection.execute("DELETE FROM artist_fts WHERE rowid = ?1", [id])?;
            connection.execute(
                "INSERT INTO artist_fts (rowid, name) VALUES (?1, ?2)",
                params![id, name],
            )?;
        }
        return Ok(id);
    }
    connection.execute(
        "INSERT INTO artist (name, sort_key) VALUES (?1, ?2)",
        params![name, key],
    )?;
    let id = connection.last_insert_rowid();
    connection.execute(
        "INSERT INTO artist_fts (rowid, name) VALUES (?1, ?2)",
        params![id, name],
    )?;
    outcome.artists_created += 1;
    Ok(id)
}

/// What identifies and describes a release when it is first seen.
struct ReleaseSpec<'a> {
    artist_id: i64,
    artist_name: &'a str,
    title: &'a str,
    year: Option<i32>,
    label: Option<&'a str>,
}

fn find_or_create_release(
    connection: &Connection,
    spec: &ReleaseSpec<'_>,
    now_ms: i64,
    outcome: &mut CatalogOutcome,
) -> Result<i64> {
    let key = sort_key(spec.title);
    if let Some((id, stored)) = connection
        .query_row(
            "SELECT id, title FROM release WHERE artist_id = ?1 AND sort_key = ?2",
            params![spec.artist_id, key],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if stored != spec.title {
            connection.execute(
                "UPDATE release SET title = ?2 WHERE id = ?1",
                params![id, spec.title],
            )?;
        }
        // The artist's spelling may have changed too: the search row follows both.
        connection.execute("DELETE FROM release_fts WHERE rowid = ?1", [id])?;
        connection.execute(
            "INSERT INTO release_fts (rowid, title, artist) VALUES (?1, ?2, ?3)",
            params![id, spec.title, spec.artist_name],
        )?;
        return Ok(id);
    }
    connection.execute(
        "INSERT INTO release (artist_id, title, sort_key, artist_key, year, label, added_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            spec.artist_id,
            spec.title,
            key,
            sort_key(spec.artist_name),
            spec.year,
            spec.label.unwrap_or_default(),
            now_ms
        ],
    )?;
    let id = connection.last_insert_rowid();
    connection.execute(
        "INSERT INTO release_fts (rowid, title, artist) VALUES (?1, ?2, ?3)",
        params![id, spec.title, spec.artist_name],
    )?;
    outcome.releases_created += 1;
    Ok(id)
}

/// Create or refresh the artist, release and track for one file. `read` is false when the tags
/// could not be read: what the path suggests stands in, and a track keeps the year and label it had.
#[allow(clippy::too_many_arguments)]
fn catalog_file(
    connection: &Connection,
    file_id: i64,
    file: &FileRow,
    tags: &TagInfo,
    read: bool,
    duration_ms: i64,
    now_ms: i64,
    outcome: &mut CatalogOutcome,
) -> Result<()> {
    let track_artist = tags
        .artist
        .as_deref()
        .or(tags.album_artist.as_deref())
        .unwrap_or(UNKNOWN_ARTIST);
    let release_artist = tags
        .album_artist
        .as_deref()
        .or(tags.artist.as_deref())
        .unwrap_or(UNKNOWN_ARTIST);
    let release_title = tags.album.as_deref().unwrap_or(UNKNOWN_RELEASE);
    let title = tags.title.as_deref().unwrap_or("Untitled");

    // One artist in two spellings (an album artist guessed from the folder name, say) is the
    // track's own spelling, so the two do not rename the artist back and forth.
    let release_artist = if sort_key(release_artist) == sort_key(track_artist) {
        track_artist
    } else {
        release_artist
    };
    let artist_id = find_or_create_artist(connection, track_artist, outcome)?;
    let release_artist_id = if release_artist == track_artist {
        artist_id
    } else {
        find_or_create_artist(connection, release_artist, outcome)?
    };
    let release_id = find_or_create_release(
        connection,
        &ReleaseSpec {
            artist_id: release_artist_id,
            artist_name: release_artist,
            title: release_title,
            year: tags.year,
            label: tags.label.as_deref(),
        },
        now_ms,
        outcome,
    )?;
    let disc = i64::from(tags.disc_number.unwrap_or(1));
    let number = i64::from(tags.track_number.unwrap_or(0));
    let genre = tags.genre.as_deref().map_or("", str::trim);
    let label = tags.label.as_deref().map_or("", str::trim);
    let tag_bpm = tags.bpm.filter(|bpm| bpm.is_finite() && *bpm > 0.0);
    let tag_key = tags.key.as_deref().map_or("", str::trim);
    let track_id = if let Some(track_id) = file.track_id {
        connection.execute(
            "UPDATE track SET release_id = ?2, artist_id = ?3, disc_no = ?4, track_no = ?5, \
             title = ?6, title_key = ?7, artist_key = ?8, duration_ms = ?9, genre = ?10, \
             year = CASE WHEN ?13 THEN ?11 ELSE year END, \
             label = CASE WHEN ?13 THEN ?12 ELSE label END, \
             tag_bpm = CASE WHEN ?13 THEN ?14 ELSE tag_bpm END, \
             tag_key = CASE WHEN ?13 THEN ?15 ELSE tag_key END WHERE id = ?1",
            params![
                track_id,
                release_id,
                artist_id,
                disc,
                number,
                title,
                sort_key(title),
                sort_key(track_artist),
                duration_ms,
                genre,
                tags.year,
                label,
                read,
                tag_bpm,
                tag_key
            ],
        )?;
        connection.execute("DELETE FROM track_fts WHERE rowid = ?1", [track_id])?;
        outcome.tracks_updated += 1;
        track_id
    } else {
        connection.execute(
            "INSERT INTO track (release_id, artist_id, disc_no, track_no, title, title_key, \
             artist_key, duration_ms, added_ms, genre, year, label, tag_bpm, tag_key) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                release_id,
                artist_id,
                disc,
                number,
                title,
                sort_key(title),
                sort_key(track_artist),
                duration_ms,
                now_ms,
                genre,
                tags.year,
                label,
                tag_bpm,
                tag_key
            ],
        )?;
        let track_id = connection.last_insert_rowid();
        connection.execute(
            "UPDATE file SET track_id = ?2 WHERE id = ?1",
            params![file_id, track_id],
        )?;
        outcome.tracks_created += 1;
        track_id
    };
    connection.execute(
        "INSERT INTO track_fts (rowid, title, artist, release, version) VALUES (?1, ?2, ?3, ?4, '')",
        params![track_id, title, track_artist, release_title],
    )?;
    Ok(())
}

impl Database {
    /// Apply a batch of tag reads in one transaction.
    pub fn apply_tag_results(
        &mut self,
        results: &[TagResult],
        now_ms: i64,
    ) -> Result<CatalogOutcome> {
        let mut outcome = CatalogOutcome::default();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for result in results {
            let file = transaction
                .query_row(
                    "SELECT track_id, path_raw, mtime_ms FROM file WHERE id = ?1",
                    [result.file_id],
                    |row| {
                        Ok(FileRow {
                            track_id: row.get(0)?,
                            path_raw: row.get(1)?,
                            mtime_ms: row.get(2)?,
                        })
                    },
                )
                .optional()?;
            let Some(file) = file.filter(|file| file.mtime_ms == result.mtime_ms) else {
                outcome.stale += 1;
                continue;
            };
            let relative = RelPath::from_stored(&file.path_raw, Flavor::Posix)
                .map_err(|_| Error::InvalidStoredValue("file path"))?;
            let components: Vec<&[u8]> = relative.components().collect();
            let mut tags = result
                .outcome
                .as_ref()
                .map(|info| info.tags.clone())
                .unwrap_or_default();
            tags.fill_from(&guess_from_path(&components));
            let mut duration_ms = 0;
            match &result.outcome {
                Ok(info) => {
                    duration_ms = info.audio.duration_ms;
                    transaction.execute(
                        "UPDATE file SET container = ?2, bitrate_kbps = ?3, sample_rate = ?4, \
                         bit_depth = ?5, channels = ?6, duration_ms = ?7, tags_mtime_ms = ?8, \
                         tags_error = NULL WHERE id = ?1",
                        params![
                            result.file_id,
                            info.container,
                            info.audio.bitrate_kbps,
                            info.audio.sample_rate,
                            info.audio.bit_depth,
                            info.audio.channels,
                            info.audio.duration_ms,
                            result.mtime_ms
                        ],
                    )?;
                }
                Err(message) => {
                    outcome.tag_errors += 1;
                    transaction.execute(
                        "UPDATE file SET tags_mtime_ms = ?2, tags_error = ?3 WHERE id = ?1",
                        params![result.file_id, result.mtime_ms, message],
                    )?;
                    // A track already known keeps what it has: guesses from the path, an empty
                    // genre and no length must not replace good values over a read that failed.
                    if file.track_id.is_some() {
                        continue;
                    }
                }
            }
            catalog_file(
                &transaction,
                result.file_id,
                &file,
                &tags,
                result.outcome.is_ok(),
                duration_ms,
                now_ms,
                &mut outcome,
            )?;
        }
        transaction.commit()?;
        Ok(outcome)
    }
}
