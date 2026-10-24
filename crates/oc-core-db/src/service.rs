//! One library database wired for an application: a writer thread for every change and a
//! pool of cached readers for every query. The UI layer talks to this, never to SQLite.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use oc_paths::{
    bytes_to_os_string, check_new_root, probe_root, relation, walk_root, Availability, Axis,
    FileId, RootRelation, WalkOptions,
};

use rusqlite::OptionalExtension;

use crate::catalog::{pending_tag_reads, pending_tag_total, read_tags, CatalogOutcome};
use crate::downloads::{
    imported_names, is_reachable, list_watch_folders, top_level_names, watch_baseline, WatchFolder,
};
use crate::library::{read_playlists, LibraryPage, LibraryQuery, PlaylistRow};
use crate::mirror::{
    alias_suggestions, mirrors, parse_alias, read_export, read_track_analysis, source_path,
    AliasSuggestion, MirrorInfo, MirrorOutcome, MirrorSource, SourceKind, TrackAnalysis,
};
use crate::publish_store::PublishStore;
use crate::roots::{check_root, list_roots, root_stats, NewRoot, ProfileSource, Root, RootKind};
use crate::scan::{ScanOutcome, ScanPass};
use crate::synthetic;
use crate::{Database, Error, ReaderPool, Result, Writer};
use oc_rekordbox::xml::Collection;

/// Files whose tags are read, then applied, per round.
const TAG_BATCH: usize = 500;

use crate::now_ms;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ScanPhase {
    Checking,
    Walking,
    Applying,
    ReadingTags,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    /// Audio files found by the walk; known from `Applying` on.
    pub files_seen: usize,
    pub tags_read: usize,
    /// Files whose tags this scan will read, known from `ReadingTags` on.
    pub tags_total: usize,
}

/// A registered folder with how many files it holds and how many are missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootInfo {
    pub root: Root,
    pub files: i64,
    pub missing: i64,
}

pub struct ScanOptions<'a> {
    /// Checked while walking and between tag batches. A cancelled walk applies nothing.
    pub cancel: Option<&'a AtomicBool>,
    /// Threads reading tags; 1 to 16.
    pub tag_threads: usize,
    /// The owner has confirmed a large disappearance that would otherwise be held.
    pub confirm_mass_missing: bool,
    /// Called on the scanning thread at each step.
    pub progress: Option<&'a dyn Fn(&ScanProgress)>,
}

impl Default for ScanOptions<'_> {
    fn default() -> Self {
        Self {
            cancel: None,
            tag_threads: 4,
            confirm_mass_missing: false,
            progress: None,
        }
    }
}

impl ScanOptions<'_> {
    fn report(&self, phase: ScanPhase, files_seen: usize, tags_read: usize, tags_total: usize) {
        if let Some(progress) = self.progress {
            progress(&ScanProgress {
                phase,
                files_seen,
                tags_read,
                tags_total,
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanSummary {
    pub availability: Availability,
    pub reason: String,
    pub scan: ScanOutcome,
    pub tags_read: usize,
    pub catalog: CatalogOutcome,
    pub cancelled: bool,
}

pub struct LibraryService {
    writer: Writer,
    pool: ReaderPool,
    /// The folder holding the database. No library root may contain it, equal it or lie
    /// inside it: a sync tool watching a library must never see the database.
    data_directory: PathBuf,
}

impl LibraryService {
    /// Open or create the database in `directory` (a private local folder) and start the
    /// writer. `readers` is the number of concurrent queries; at least one.
    pub fn open(directory: &Path, readers: usize) -> Result<Self> {
        let database = Database::open(directory)?;
        let pool = ReaderPool::new(&database, readers)?;
        Ok(Self {
            writer: Writer::spawn(database),
            pool,
            data_directory: oc_paths::canonical(directory)?,
        })
    }

    /// One window of one view. Blocks only while every reader is busy.
    pub fn read_page(
        &self,
        query: &LibraryQuery,
        start: usize,
        count: usize,
    ) -> Result<LibraryPage> {
        self.pool.lease().read_page(query, start, count)
    }

    /// Fill an empty library with synthetic tracks for a demo. Does nothing, and says so,
    /// if the library already holds any track.
    pub fn seed_demo(&self, tracks: u32) -> Result<bool> {
        self.writer.run(move |database| {
            let existing: i64 =
                database
                    .connection
                    .query_row("SELECT COUNT(*) FROM track", [], |row| row.get(0))?;
            if existing > 0 {
                return Ok(false);
            }
            synthetic::seed(database, tracks)?;
            Ok(true)
        })?
    }

    /// Every playlist and folder from the mirrored exports.
    pub fn playlists(&self) -> Result<Vec<PlaylistRow>> {
        read_playlists(self.pool.lease().connection())
    }

    /// The cues and beat grid rekordbox has for a library track, if an export matched it.
    pub fn track_analysis(&self, track_id: i64) -> Result<Option<TrackAnalysis>> {
        read_track_analysis(self.pool.lease().connection(), track_id)
    }

    /// Every mirrored rekordbox export with how its entries matched the library.
    pub fn mirrors(&self) -> Result<Vec<MirrorInfo>> {
        mirrors(self.pool.lease().connection())
    }

    /// Read a rekordbox XML export and mirror it. The file is parsed here, away from the
    /// writer; the replacement is one transaction. A file that cannot be read is recorded as
    /// a failure and the previous mirror stays.
    pub fn import_export(&self, path: &Path) -> Result<MirrorOutcome> {
        let read = match read_export(path) {
            Ok(read) => read,
            Err(error) => {
                let (path, message) = (path.to_path_buf(), error.to_string());
                self.writer.run(move |database| {
                    database.record_mirror_failure(&path, &message, now_ms())
                })??;
                return Err(error);
            }
        };
        let path = path.to_path_buf();
        self.writer.run(move |database| {
            let outcome = database.replace_mirror(
                &MirrorSource {
                    kind: SourceKind::Xml,
                    path: &path,
                    file_mtime_ms: read.file_mtime_ms,
                },
                &read.collection,
                now_ms(),
            )?;
            // An export read after a database read takes over as the rekordbox source.
            database.drop_sources(outcome.source_id, &[SourceKind::MasterDb])?;
            Ok(outcome)
        })?
    }

    /// Mirror a collection read from a checked copy of rekordbox's `master.db`. It is recorded
    /// under the live library's path and replaces every other rekordbox source.
    pub fn import_database(
        &self,
        live_master_db: &Path,
        collection: Collection,
    ) -> Result<MirrorOutcome> {
        let path = live_master_db.to_path_buf();
        self.writer.run(move |database| {
            let outcome = database.replace_mirror(
                &MirrorSource {
                    kind: SourceKind::MasterDb,
                    path: &path,
                    file_mtime_ms: None,
                },
                &collection,
                now_ms(),
            )?;
            database.drop_sources(outcome.source_id, &[SourceKind::Xml, SourceKind::MasterDb])?;
            Ok(outcome)
        })?
    }

    /// What differs between openCrate and rekordbox's last direct read, field by field; fields
    /// that agree become the merge's base.
    pub fn merge_review(&self) -> Result<Vec<crate::merge::Change>> {
        self.writer
            .run(|database| database.merge_review(now_ms()))?
    }

    /// Record values both sides now hold, after a sync applied them.
    pub fn record_merge_base(&self, agreed: Vec<(i64, crate::merge::Field, String)>) -> Result<()> {
        self.writer
            .run(move |database| database.record_merge_base(&agreed, now_ms()))?
    }

    /// rekordbox's id for a playlist or folder read directly from its library; none for one
    /// from an export.
    pub fn rekordbox_playlist_key(&self, playlist_id: i64) -> Result<Option<String>> {
        crate::offers::rekordbox_playlist_key(self.pool.lease().connection(), playlist_id)
    }

    /// For each track, in order: rekordbox's id for it from the last direct read, or the file to
    /// add when rekordbox does not have it.
    pub fn rekordbox_members(
        &self,
        track_ids: &[i64],
    ) -> Result<Vec<crate::offers::RekordboxMember>> {
        let mut members = Vec::with_capacity(track_ids.len());
        for &track_id in track_ids {
            let known = crate::offers::rekordbox_key(self.pool.lease().connection(), track_id)?;
            members.push(match known {
                Some(key) => crate::offers::RekordboxMember::Known(key),
                None => match self.track_file(track_id)? {
                    Some(path) => crate::offers::RekordboxMember::File(path),
                    None => crate::offers::RekordboxMember::NoFile,
                },
            });
        }
        Ok(members)
    }

    /// Files openCrate filed that rekordbox does not have yet, newest first, and when rekordbox
    /// was last read (none if never), so the page can say when the answer may be stale.
    pub fn rekordbox_candidates(&self) -> Result<(Vec<crate::offers::Candidate>, Option<i64>)> {
        let lease = self.pool.lease();
        let connection = lease.connection();
        let found = crate::offers::candidates(connection)?;
        let read_ms =
            connection.query_row("SELECT MAX(last_read_ms) FROM rb_source", [], |row| {
                row.get(0)
            })?;
        Ok((found, read_ms))
    }

    /// A crate's name and tracks, each as it can go into openCrate's rekordbox file.
    pub fn crate_for_rekordbox(
        &self,
        crate_id: i64,
    ) -> Result<(String, Vec<crate::offers::CrateTrack>)> {
        crate::offers::crate_tracks(self.pool.lease().connection(), crate_id)
    }

    /// Keep files out of rekordbox (no longer offered as new), or bring them back.
    pub fn set_rekordbox_hidden(
        &self,
        file_ids: Vec<i64>,
        hidden: bool,
        now_ms: i64,
    ) -> Result<()> {
        self.writer.run(move |database| {
            let transaction = database.connection.transaction()?;
            for file_id in &file_ids {
                if hidden {
                    transaction.execute(
                        "INSERT OR IGNORE INTO rekordbox_hidden (file_id, hidden_ms) VALUES (?1, ?2)",
                        rusqlite::params![file_id, now_ms],
                    )?;
                } else {
                    transaction.execute("DELETE FROM rekordbox_hidden WHERE file_id = ?1", [file_id])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })?
    }

    /// Save a setting and record offers in one transaction, so the file openCrate writes from
    /// the setting and the offers it shows never disagree. Fails whole if a file is unknown.
    pub fn record_offers_with_setting(
        &self,
        key: &str,
        value: String,
        file_ids: Vec<i64>,
        now_ms: i64,
    ) -> Result<()> {
        let key = key.to_owned();
        self.writer.run(move |database| {
            let transaction = database.connection.transaction()?;
            transaction.execute(
                "INSERT INTO app_setting (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value],
            )?;
            {
                let mut insert = transaction.prepare(
                    "INSERT INTO rekordbox_offer (file_id, offered_ms) VALUES (?1, ?2) \
                     ON CONFLICT(file_id) DO UPDATE SET offered_ms = excluded.offered_ms",
                )?;
                for file_id in &file_ids {
                    insert.execute(rusqlite::params![file_id, now_ms])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })?
    }

    /// Remember that these files were written into openCrate's rekordbox XML file.
    pub fn record_offers(&self, file_ids: Vec<i64>, now_ms: i64) -> Result<()> {
        self.writer.run(move |database| {
            let transaction = database.connection.transaction()?;
            {
                let mut insert = transaction.prepare(
                    "INSERT INTO rekordbox_offer (file_id, offered_ms) VALUES (?1, ?2) \
                     ON CONFLICT(file_id) DO UPDATE SET offered_ms = excluded.offered_ms",
                )?;
                for file_id in &file_ids {
                    insert.execute(rusqlite::params![file_id, now_ms])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })?
    }

    #[cfg(test)]
    pub(crate) fn offer_count(&self) -> Result<i64> {
        Ok(self.pool.lease().connection().query_row(
            "SELECT COUNT(*) FROM rekordbox_offer",
            [],
            |row| row.get(0),
        )?)
    }

    /// The private folder holding the database; app-owned files such as rekordbox backups
    /// live beside it.
    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    /// Read a known export again, from the path it was first read from.
    pub fn reread_mirror(&self, source_id: i64) -> Result<MirrorOutcome> {
        let path = source_path(self.pool.lease().connection(), source_id)?;
        self.import_export(&path)
    }

    /// Prefixes that would make unmapped entries match indexed files, with the evidence.
    pub fn alias_suggestions(&self, source_id: i64, limit: usize) -> Result<Vec<AliasSuggestion>> {
        alias_suggestions(self.pool.lease().connection(), source_id, limit)
    }

    /// Tell a folder which rekordbox Location prefix belongs to it. Returns false if known.
    pub fn add_alias(&self, root_id: i64, prefix: &str) -> Result<bool> {
        let alias = parse_alias(prefix)?;
        self.writer
            .run(move |database| database.add_alias(root_id, &alias, now_ms()))?
    }

    /// Remove a folder from the library (files on disk are untouched). Returns the number of
    /// index entries removed.
    pub fn remove_root(&self, root_id: i64) -> Result<usize> {
        self.writer
            .run(move |database| database.remove_root(root_id))?
    }

    /// Look at every folder read-only and remember what was found, so an unplugged drive is
    /// noticed without a scan. Marks nothing missing; it writes only when the answer changed.
    pub fn check_roots(&self) -> Result<Vec<RootInfo>> {
        for root in list_roots(self.pool.lease().connection())? {
            let (availability, reason) = check_root(&root);
            if availability == root.availability && reason == root.availability_reason {
                continue;
            }
            let id = root.id;
            self.writer.run(move |database| {
                database.set_availability(id, availability, &reason, now_ms())
            })??;
        }
        self.roots()
    }

    /// The owner says the folder now at this path is the library (for example the drive was
    /// replaced, or the folder was recreated). Its current identity is recorded.
    pub fn confirm_root(&self, root_id: i64) -> Result<()> {
        let root = list_roots(self.pool.lease().connection())?
            .into_iter()
            .find(|root| root.id == root_id)
            .ok_or(Error::UnknownRoot(root_id))?;
        let os_path = bytes_to_os_string(&root.path_raw).map_err(|_| Error::InvalidRoot)?;
        let path = Path::new(&os_path);
        if !path.is_dir() {
            return Err(Error::Io(std::io::Error::from(
                std::io::ErrorKind::NotFound,
            )));
        }
        let token = FileId::of(path)?.persistent_token();
        self.writer
            .run(move |database| database.confirm_root(root_id, token.as_deref(), now_ms()))?
    }

    /// Every registered folder with its counts.
    pub fn roots(&self) -> Result<Vec<RootInfo>> {
        let lease = self.pool.lease();
        let stats = root_stats(lease.connection())?;
        Ok(list_roots(lease.connection())?
            .into_iter()
            .map(|root| {
                let found = stats.iter().find(|stat| stat.root_id == root.id);
                RootInfo {
                    files: found.map_or(0, |stat| stat.files),
                    missing: found.map_or(0, |stat| stat.missing),
                    root,
                }
            })
            .collect())
    }

    fn watch_paths(&self) -> Result<Vec<PathBuf>> {
        Ok(list_watch_folders(self.pool.lease().connection())?
            .iter()
            .filter_map(|folder| bytes_to_os_string(&folder.path_raw).ok())
            .map(PathBuf::from)
            .collect())
    }

    /// Where a release's audio files are on disk, album order first, for reading cover art.
    /// Only files the index lists as present; at most `limit` of them.
    pub fn release_files(&self, release_id: i64, limit: usize) -> Result<Vec<PathBuf>> {
        let lease = self.pool.lease();
        let mut statement = lease.connection().prepare(
            "SELECT r.path_raw, f.path_raw FROM file f \
             JOIN track t ON t.id = f.track_id JOIN library_root r ON r.id = f.root_id \
             WHERE t.release_id = ?1 AND f.presence = 'present' AND f.lifecycle = 'active' \
             ORDER BY t.disc_no, t.track_no, f.id LIMIT ?2",
        )?;
        let rows = statement.query_map(
            rusqlite::params![release_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )?;
        let mut paths = Vec::new();
        for row in rows {
            let (root, relative) = row?;
            if let Some(path) = crate::catalog::full_path(&root, &relative) {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    /// Every present file below a folder the library check names, with its path inside the
    /// music folder as components.
    pub fn files_below(&self, root_id: i64, folder: &str) -> Result<Vec<(i64, Vec<Vec<u8>>)>> {
        crate::health::files_below(self.pool.lease().connection(), root_id, folder)
    }

    /// One copy of a recording filed more than once, with what rekordbox has for it.
    pub fn duplicate_copy(&self, file_id: i64) -> Result<Option<crate::health::Copy>> {
        crate::health::copy_of(self.pool.lease().connection(), file_id)
    }

    /// rekordbox's id for a library file, when the last direct read matched it.
    pub fn rekordbox_file_key(&self, file_id: i64) -> Result<Option<String>> {
        crate::offers::rekordbox_file_key(self.pool.lease().connection(), file_id)
    }

    /// A library file's root and full path, when it is present.
    pub fn file_location(&self, file_id: i64) -> Result<Option<(i64, PathBuf)>> {
        use rusqlite::OptionalExtension;
        let lease = self.pool.lease();
        let row: Option<(i64, Vec<u8>, Vec<u8>)> = lease
            .connection()
            .query_row(
                "SELECT r.id, r.path_raw, f.path_raw FROM file f JOIN library_root r ON r.id = f.root_id \
                 WHERE f.id = ?1 AND f.presence = 'present'",
                [file_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        Ok(row.and_then(|(root_id, root, relative)| {
            crate::catalog::full_path(&root, &relative).map(|path| (root_id, path))
        }))
    }

    /// Every present file of a track, with its music folder: what an edit to the track changes.
    pub fn track_files(&self, track_id: i64) -> Result<Vec<(i64, PathBuf)>> {
        let lease = self.pool.lease();
        let mut statement = lease.connection().prepare(
            "SELECT r.id, r.path_raw, f.path_raw FROM file f JOIN library_root r ON r.id = f.root_id \
             WHERE f.track_id = ?1 AND f.presence = 'present' AND f.lifecycle = 'active' \
             AND r.availability = 'available' ORDER BY f.id",
        )?;
        let rows = statement.query_map([track_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let mut files = Vec::new();
        for row in rows {
            let (root_id, root, relative) = row?;
            if let Some(path) = crate::catalog::full_path(&root, &relative) {
                files.push((root_id, path));
            }
        }
        Ok(files)
    }

    /// The file to play for a track: a present, active one, lossless first. `None` when the track
    /// has no file that can be reached.
    pub fn track_file(&self, track_id: i64) -> Result<Option<PathBuf>> {
        let lease = self.pool.lease();
        let mut statement = lease.connection().prepare(
            "SELECT r.path_raw, f.path_raw FROM file f JOIN library_root r ON r.id = f.root_id \
             WHERE f.track_id = ?1 AND f.presence = 'present' AND f.lifecycle = 'active' \
             AND r.availability = 'available' \
             ORDER BY CASE f.container WHEN 'AIFF' THEN 0 WHEN 'WAV' THEN 0 WHEN 'FLAC' THEN 0 ELSE 1 END, f.id",
        )?;
        let rows = statement.query_map([track_id], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (root, relative) = row?;
            if let Some(path) = crate::catalog::full_path(&root, &relative) {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    /// A track's waveform: the stored one while its file is unchanged and it is in `format`,
    /// otherwise computed with `compute` from the file (only read) and stored with that format.
    /// `None` when the track has no reachable file.
    pub fn waveform(
        &self,
        track_id: i64,
        format: i64,
        compute: impl FnOnce(&Path) -> Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>> {
        let found = {
            let lease = self.pool.lease();
            let mut statement = lease.connection().prepare(
                "SELECT f.id, f.size, f.mtime_ms, r.path_raw, f.path_raw, w.size, w.mtime_ms, w.peaks, w.format \
                 FROM file f JOIN library_root r ON r.id = f.root_id \
                 LEFT JOIN waveform w ON w.file_id = f.id \
                 WHERE f.track_id = ?1 AND f.presence = 'present' AND f.lifecycle = 'active' \
                 AND r.availability = 'available' \
                 ORDER BY CASE f.container WHEN 'AIFF' THEN 0 WHEN 'WAV' THEN 0 WHEN 'FLAC' THEN 0 ELSE 1 END, f.id \
                 LIMIT 1",
            )?;
            statement
                .query_row([track_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<Vec<u8>>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                    ))
                })
                .optional()?
        };
        let Some((
            file_id,
            size,
            mtime,
            root,
            relative,
            stored_size,
            stored_mtime,
            stored,
            stored_format,
        )) = found
        else {
            return Ok(None);
        };
        if let Some(peaks) = stored.filter(|_| {
            stored_format == Some(format)
                && stored_size == Some(size)
                && stored_mtime == Some(mtime)
        }) {
            return Ok(Some(peaks));
        }
        let Some(path) = crate::catalog::full_path(&root, &relative) else {
            return Ok(None);
        };
        let Some(peaks) = compute(&path) else {
            return Ok(None);
        };
        let kept = peaks.clone();
        self.writer.run(move |database| {
            database.connection.execute(
                "INSERT INTO waveform (file_id, size, mtime_ms, peaks, format) VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(file_id) DO UPDATE SET size = excluded.size, \
                 mtime_ms = excluded.mtime_ms, peaks = excluded.peaks, format = excluded.format",
                rusqlite::params![file_id, size, mtime, kept, format],
            )
        })??;
        Ok(Some(peaks))
    }

    /// Releases with tracks by this artist, newest first, at most `limit`.
    pub fn artist_release_ids(&self, artist_id: i64, limit: usize) -> Result<Vec<i64>> {
        let lease = self.pool.lease();
        let mut statement = lease.connection().prepare(
            "SELECT r.id FROM release r WHERE r.id IN \
             (SELECT release_id FROM track WHERE artist_id = ?1) OR r.artist_id = ?1 \
             ORDER BY r.added_ms DESC, r.id DESC LIMIT ?2",
        )?;
        let ids = statement
            .query_map(
                rusqlite::params![artist_id, i64::try_from(limit).unwrap_or(i64::MAX)],
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        Ok(ids)
    }

    /// Arrange the whole track list by hand, starting from the order `from` shows.
    pub fn move_in_library(
        &self,
        track_ids: Vec<i64>,
        before: Option<i64>,
        from: crate::library::Sort,
    ) -> Result<()> {
        self.writer
            .run(move |database| database.move_in_library(&track_ids, before, from))?
    }

    /// The owner's crates, in their order, with track counts.
    pub fn crates(&self) -> Result<Vec<crate::crates::CrateRow>> {
        crate::crates::list_crates(self.pool.lease().connection())
    }

    pub fn create_crate(&self, name: &str) -> Result<i64> {
        let name = name.to_owned();
        self.writer
            .run(move |database| database.create_crate(&name, now_ms()))?
    }

    pub fn rename_crate(&self, crate_id: i64, name: &str) -> Result<()> {
        let name = name.to_owned();
        self.writer
            .run(move |database| database.rename_crate(crate_id, &name))?
    }

    /// Delete a crate; its tracks stay in the library.
    pub fn delete_crate(&self, crate_id: i64) -> Result<()> {
        self.writer
            .run(move |database| database.delete_crate(crate_id))?
    }

    pub fn add_to_crate(&self, crate_id: i64, track_ids: Vec<i64>) -> Result<usize> {
        self.writer
            .run(move |database| database.add_to_crate(crate_id, &track_ids, now_ms()))?
    }

    pub fn remove_from_crate(&self, crate_id: i64, track_ids: Vec<i64>) -> Result<usize> {
        self.writer
            .run(move |database| database.remove_from_crate(crate_id, &track_ids))?
    }

    /// Move tracks to just before `before` (the end when `None`). Returns the new order.
    pub fn move_in_crate(
        &self,
        crate_id: i64,
        track_ids: Vec<i64>,
        before: Option<i64>,
    ) -> Result<Vec<i64>> {
        self.writer
            .run(move |database| database.move_in_crate(crate_id, &track_ids, before))?
    }

    /// Every download folder.
    pub fn watch_folders(&self) -> Result<Vec<WatchFolder>> {
        list_watch_folders(self.pool.lease().connection())
    }

    /// Add a download folder. It must exist, be readable, and neither overlap the library,
    /// openCrate's own data folder, nor another download folder. Nothing in it is read beyond
    /// the check, and nothing is staged: only releases that arrive later are picked up.
    pub fn add_watch_folder(&self, path: &Path, label: &str) -> Result<i64> {
        let reserved = [self.data_directory.as_path()];
        let canonical = check_new_root(path, &[], &reserved)?;
        let baseline = top_level_names(&canonical)?;
        for root in list_roots(self.pool.lease().connection())? {
            if let Ok(other) = bytes_to_os_string(&root.path_raw) {
                if relation(&canonical, Path::new(&other)) != RootRelation::Disjoint {
                    return Err(Error::WatchOverlapsLibrary);
                }
            }
        }
        if self
            .watch_paths()?
            .iter()
            .any(|other| relation(&canonical, other) != RootRelation::Disjoint)
        {
            return Err(Error::WatchOverlapsWatch);
        }
        let label = label.to_owned();
        self.writer.run(move |database| {
            database.add_watch_folder(&canonical, &label, &baseline, now_ms())
        })?
    }

    /// What a download folder held when it was added; `None` if it was added before such
    /// lists were kept.
    pub fn watch_baseline(&self, id: i64) -> Result<Option<std::collections::HashSet<Vec<u8>>>> {
        watch_baseline(self.pool.lease().connection(), id)
    }

    /// Count everything in a download folder now as already there. Reads the folder's names
    /// only; nothing in it changes.
    pub fn retake_watch_baseline(&self, id: i64) -> Result<()> {
        let folder = list_watch_folders(self.pool.lease().connection())?
            .into_iter()
            .find(|folder| folder.id == id)
            .ok_or(Error::UnknownWatchFolder(id))?;
        let path = bytes_to_os_string(&folder.path_raw).map_err(|_| Error::InvalidRoot)?;
        let names = top_level_names(Path::new(&path))?;
        self.writer
            .run(move |database| database.retake_watch_baseline(id, &names))?
    }

    /// Folders named from untidy tags and recordings filed twice. Reads only.
    pub fn check_health(&self) -> Result<crate::health::Health> {
        crate::health::check(self.pool.lease().connection())
    }

    /// A library-wide setting, or `None` if it was never set.
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .pool
            .lease()
            .connection()
            .query_row(
                "SELECT value FROM app_setting WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Store a library-wide setting; `None` removes it, so the default applies again.
    pub fn set_setting(&self, key: &str, value: Option<String>) -> Result<()> {
        let key = key.to_owned();
        self.writer.run(move |database| {
            match value {
                Some(value) => database.connection.execute(
                    "INSERT INTO app_setting (key, value) VALUES (?1, ?2) \
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![key, value],
                )?,
                None => database
                    .connection
                    .execute("DELETE FROM app_setting WHERE key = ?1", [key])?,
            };
            Ok(())
        })?
    }

    /// The top-level names imported from a download folder so far.
    pub fn imported_releases(&self, id: i64) -> Result<std::collections::HashSet<Vec<u8>>> {
        imported_names(self.pool.lease().connection(), id)
    }

    /// Record that `files` files of the release `name` were imported from download folder `id`.
    pub fn mark_imported(&self, id: i64, name: Vec<u8>, files: usize) -> Result<()> {
        self.writer
            .run(move |database| database.mark_imported(id, &name, files, now_ms()))?
    }

    /// Publish one staged file to its place in the library with the crash-safe protocol of
    /// `oc_publish`: recorded first, linked without ever replacing a file, then the staged name
    /// removed. The catalog learns about the file from the next scan of its root.
    pub fn publish_file(
        &self,
        staged: PathBuf,
        destination: PathBuf,
    ) -> Result<oc_publish::Outcome> {
        self.writer.run(move |database| {
            let mut store = PublishStore::new(database);
            let operation = store.begin(&staged, &destination)?;
            let outcome =
                oc_publish::Executor::new(&mut store, &mut oc_publish::NoFaults).run(operation)?;
            Ok(outcome)
        })?
    }

    /// Finish or reject every publication a crash interrupted. Run once at startup.
    pub fn recover_publications(&self) -> Result<usize> {
        self.writer.run(move |database| {
            let mut store = PublishStore::new(database);
            let ended =
                oc_publish::Executor::new(&mut store, &mut oc_publish::NoFaults).recover_all()?;
            Ok(ended.len())
        })?
    }

    /// Switch a download folder on or off. Takes effect at once; nothing else changes.
    pub fn set_watch_folder_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        self.writer
            .run(move |database| database.set_watch_enabled(id, enabled))?
    }

    /// Remove a download folder from the list. Nothing on disk is touched.
    pub fn remove_watch_folder(&self, id: i64) -> Result<()> {
        self.writer
            .run(move |database| database.remove_watch_folder(id))?
    }

    /// Look at every download folder read-only and remember whether it can be reached. A
    /// folder that cannot be reached stays in the list and is picked up again on return.
    pub fn check_watch_folders(&self) -> Result<Vec<WatchFolder>> {
        for folder in list_watch_folders(self.pool.lease().connection())? {
            let online = is_reachable(&folder);
            if online != folder.online {
                let id = folder.id;
                self.writer
                    .run(move |database| database.set_watch_online(id, online, now_ms()))??;
            }
        }
        self.watch_folders()
    }

    /// Register a library folder. Checks it read-only (it must exist, be a directory and not
    /// overlap another root), probes how its volume compares names, and records it.
    pub fn add_root(&self, path: &Path, label: &str) -> Result<i64> {
        let existing: Vec<PathBuf> = list_roots(self.pool.lease().connection())?
            .iter()
            .filter_map(|root| bytes_to_os_string(&root.path_raw).ok())
            .map(PathBuf::from)
            .collect();
        let others: Vec<&Path> = existing.iter().map(PathBuf::as_path).collect();
        let reserved = [self.data_directory.as_path()];
        let canonical = check_new_root(path, &others, &reserved)?;
        if self
            .watch_paths()?
            .iter()
            .any(|other| relation(&canonical, other) != RootRelation::Disjoint)
        {
            return Err(Error::RootOverlapsWatch);
        }
        let probe = probe_root(&canonical, 5_000)?;
        let profile_source = if probe.case == Axis::Unknown && probe.normalization == Axis::Unknown
        {
            ProfileSource::Default
        } else {
            ProfileSource::StatProbe
        };
        let dir_token = FileId::of(&canonical)?.persistent_token();
        let (label, profile) = (label.to_owned(), probe.profile());
        self.writer.run(move |database| {
            database.add_root(
                &NewRoot {
                    path: &canonical,
                    label: &label,
                    kind: RootKind::Local,
                    profile,
                    profile_source,
                    dir_token,
                    volume_token: None,
                },
                now_ms(),
            )
        })?
    }

    /// Check a root, walk it, apply what was found, then read the tags of new and changed
    /// files and build the catalog from them. The walk and the tag reads run on the calling
    /// thread; only the database changes go through the writer, in short transactions.
    pub fn scan_root(&self, root_id: i64, options: &ScanOptions<'_>) -> Result<ScanSummary> {
        let root = list_roots(self.pool.lease().connection())?
            .into_iter()
            .find(|root| root.id == root_id)
            .ok_or(Error::UnknownRoot(root_id))?;
        options.report(ScanPhase::Checking, 0, 0, 0);
        let (availability, reason) = check_root(&root);
        let mut summary = ScanSummary {
            availability,
            reason: reason.clone(),
            scan: ScanOutcome {
                skipped: true,
                ..ScanOutcome::default()
            },
            tags_read: 0,
            catalog: CatalogOutcome::default(),
            cancelled: false,
        };
        self.writer.run(move |database| {
            database.set_availability(root_id, availability, &reason, now_ms())
        })??;
        if availability != Availability::Available {
            options.report(ScanPhase::Finished, 0, 0, 0);
            return Ok(summary);
        }
        options.report(ScanPhase::Walking, 0, 0, 0);
        let path = bytes_to_os_string(&root.path_raw).map_err(|_| Error::InvalidRoot)?;
        let walked = walk_root(
            Path::new(&path),
            &WalkOptions {
                cancel: options.cancel,
                ..WalkOptions::default()
            },
        );
        let report = match walked {
            Ok(report) => report,
            Err(error) => {
                let reason = error.to_string();
                summary.availability = Availability::Unavailable;
                summary.reason.clone_from(&reason);
                self.writer.run(move |database| {
                    database.set_availability(root_id, Availability::Unavailable, &reason, now_ms())
                })??;
                options.report(ScanPhase::Finished, 0, 0, 0);
                return Ok(summary);
            }
        };
        if report.cancelled {
            summary.cancelled = true;
            options.report(ScanPhase::Finished, 0, 0, 0);
            return Ok(summary);
        }
        let files_seen = report.files.len();
        options.report(ScanPhase::Applying, files_seen, 0, 0);
        let pass = ScanPass::from_walk(report, options.confirm_mass_missing);
        summary.scan = self
            .writer
            .run(move |database| database.apply_scan(root_id, &pass, now_ms()))??;

        let tags_total = pending_tag_total(self.pool.lease().connection())?;
        options.report(ScanPhase::ReadingTags, files_seen, 0, tags_total);
        loop {
            if options
                .cancel
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                summary.cancelled = true;
                break;
            }
            let pending = pending_tag_reads(self.pool.lease().connection(), TAG_BATCH)?;
            if pending.is_empty() {
                break;
            }
            let results = read_tags(&pending, options.tag_threads);
            summary.tags_read += results.len();
            let applied = self
                .writer
                .run(move |database| database.apply_tag_results(&results, now_ms()))??;
            // Every applied result creates or updates a track, so none means nothing moved on.
            let made_no_progress = applied.tracks_created + applied.tracks_updated == 0;
            summary.catalog.add(&applied);
            options.report(
                ScanPhase::ReadingTags,
                files_seen,
                summary.tags_read,
                tags_total,
            );
            if made_no_progress {
                break;
            }
        }
        options.report(
            ScanPhase::Finished,
            files_seen,
            summary.tags_read,
            tags_total,
        );
        Ok(summary)
    }

    /// Finish queued writes and mark the database clean.
    pub fn shutdown(self) -> Result<()> {
        self.writer.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use tempfile::tempdir;

    use super::*;
    use crate::library::{Sort, View};
    use crate::{Database, OpenStatus};

    #[test]
    fn a_demo_library_is_seeded_once_and_readable_from_many_threads() {
        let directory = tempdir().unwrap();
        let service = Arc::new(LibraryService::open(directory.path(), 3).unwrap());
        assert_eq!(
            service
                .read_page(&LibraryQuery::new(View::Tracks), 0, 10)
                .unwrap()
                .total,
            0
        );
        assert!(service.seed_demo(500).unwrap());
        assert!(
            !service.seed_demo(500).unwrap(),
            "a library with tracks is left alone"
        );
        let handles: Vec<_> = (0..6)
            .map(|worker| {
                let service = Arc::clone(&service);
                thread::spawn(move || {
                    let mut query = LibraryQuery::new(View::Tracks);
                    query.sort = if worker % 2 == 0 {
                        Sort::Title
                    } else {
                        Sort::Recent
                    };
                    for start in (0..500).step_by(100) {
                        let page = service.read_page(&query, start, 100).unwrap();
                        assert_eq!((page.total, page.items.len()), (500, 100));
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        Arc::into_inner(service).unwrap().shutdown().unwrap();
        let reopened = Database::open(directory.path()).unwrap();
        assert_eq!(reopened.status(), OpenStatus::Clean);
    }
}
