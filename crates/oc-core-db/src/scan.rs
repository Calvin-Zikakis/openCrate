//! Applying one scan pass to a root's files.
//!
//! The scanner (`oc_paths::walk_root`) walks a root read-only and reports what it saw. This
//! module decides what that means for the rows: what is new, what changed, what came
//! back, and, only when the evidence is good enough, what is missing. Lifecycle is never
//! touched here; the rows' `lifecycle` belongs to openCrate's own operations.

use std::collections::{HashMap, HashSet};

use std::io;
use std::path::Path;

use oc_paths::{
    bytes_to_os_string, first_collision, hold_mass_disappearance, may_mark_missing, walk_root,
    Availability, Flavor, NameProfile, RelPath, WalkOptions, WalkReport,
};
use rusqlite::{params, Connection, TransactionBehavior};

use crate::roots::{rekey, ProfileSource};
use crate::{Database, Error, Result};

pub struct ObservedFile {
    pub path: RelPath,
    pub size: i64,
    pub mtime_ms: i64,
    pub container: String,
    pub file_token: Option<Vec<u8>>,
}

pub use oc_paths::DirPath;

pub struct ScanPass {
    /// The root's availability when this pass ran. Anything but `Available` changes nothing.
    pub root_availability: Availability,
    pub files: Vec<ObservedFile>,
    /// Every directory the walk found, whether or not it could be listed.
    pub existing_dirs: Vec<DirPath>,
    /// The directories whose listing completed. The root must be among them for any file
    /// to be marked missing.
    pub enumerated_dirs: Vec<DirPath>,
    /// The owner has confirmed a large disappearance that would otherwise be held.
    pub confirm_mass_missing: bool,
}

impl ScanPass {
    /// The pass for a walk that reached its root. Directories the walk could not list stay
    /// out of `enumerated_dirs`, so nothing under them can be concluded missing.
    #[must_use]
    pub fn from_walk(report: WalkReport, confirm_mass_missing: bool) -> Self {
        Self {
            root_availability: Availability::Available,
            files: report
                .files
                .into_iter()
                .map(|file| ObservedFile {
                    path: file.path,
                    size: file.size,
                    mtime_ms: file.mtime_ms,
                    container: file.container.to_owned(),
                    file_token: file.file_token,
                })
                .collect(),
            existing_dirs: report.existing_dirs,
            enumerated_dirs: report.enumerated_dirs,
            confirm_mass_missing,
        }
    }
}

/// Walk a stored root path without touching the database, so a long walk never holds the
/// writer. An error means the root could not be listed: record it as unavailable and apply
/// nothing.
pub fn walk_stored_root(
    path_raw: &[u8],
    options: &WalkOptions<'_>,
    confirm_mass_missing: bool,
) -> io::Result<ScanPass> {
    let path = bytes_to_os_string(path_raw).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "root path is not representable",
        )
    })?;
    Ok(ScanPass::from_walk(
        walk_root(Path::new(&path), options)?,
        confirm_mass_missing,
    ))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanOutcome {
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// Rows that were missing and have been seen again; the same row, so its links survive.
    pub revived: usize,
    /// Rows whose on-disk spelling changed within the volume's own name equivalence.
    pub respelled: usize,
    pub marked_missing: usize,
    /// How many rows were held back from `missing` pending confirmation (0 if none).
    pub held_missing: usize,
    /// A key collision showed the profile was too coarse, so the root fell back to `exact`.
    pub demoted_profile: bool,
    /// The root was not available, so the pass was ignored.
    pub skipped: bool,
}

struct Existing {
    id: i64,
    raw: Vec<u8>,
    present: bool,
    size: i64,
    mtime_ms: i64,
    container: String,
}

fn dir_key(profile: NameProfile, dir: &[Vec<u8>]) -> Vec<u8> {
    let mut key = Vec::new();
    for (index, component) in dir.iter().enumerate() {
        if index > 0 {
            key.push(0);
        }
        key.extend_from_slice(&profile.key_component(component));
    }
    key
}

/// Is the absence of a file whose parent directory is `parent` established by this pass?
/// Yes if that directory was listed, or if a listed ancestor shows the next step toward it
/// is not there. No if any listing on the way down failed or the root was not listed.
fn absence_established(
    profile: NameProfile,
    parent: &[Vec<u8>],
    existing: &HashSet<Vec<u8>>,
    enumerated: &HashSet<Vec<u8>>,
) -> bool {
    for depth in 0..=parent.len() {
        let here = dir_key(profile, &parent[..depth]);
        if !enumerated.contains(&here) {
            return false;
        }
        if depth == parent.len() {
            return true;
        }
        if !existing.contains(&dir_key(profile, &parent[..=depth])) {
            return true;
        }
    }
    false
}

fn load_existing(connection: &Connection, root_id: i64) -> Result<HashMap<Vec<u8>, Existing>> {
    let mut statement = connection.prepare(
        "SELECT id, path_key, path_raw, presence, size, mtime_ms, container FROM file \
         WHERE root_id = ?1",
    )?;
    let rows = statement.query_map([root_id], |row| {
        Ok((
            row.get::<_, Vec<u8>>(1)?,
            Existing {
                id: row.get(0)?,
                raw: row.get(2)?,
                present: row.get::<_, String>(3)? == "present",
                size: row.get(4)?,
                mtime_ms: row.get(5)?,
                container: row.get(6)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The library file already listed at this path inside a music folder, present or missing.
pub fn file_at(
    connection: &Connection,
    root_id: i64,
    components: &[Vec<u8>],
) -> Result<Option<i64>> {
    use rusqlite::OptionalExtension;
    let profile = connection.query_row(
        "SELECT name_profile FROM library_root WHERE id = ?1",
        [root_id],
        |row| row.get::<_, String>(0),
    )?;
    let profile = NameProfile::parse(&profile).ok_or(Error::InvalidStoredValue("name profile"))?;
    let path = RelPath::from_components(components, Flavor::Posix)
        .map_err(|_| Error::InvalidStoredValue("path"))?;
    Ok(connection
        .query_row(
            "SELECT id FROM file WHERE root_id = ?1 AND path_key = ?2",
            params![root_id, profile.key(&path).as_bytes()],
            |row| row.get(0),
        )
        .optional()?)
}

impl Database {
    /// Files openCrate itself moved inside their music folder (tidying a folder's name): each
    /// row takes its new path, so the next scan finds the same file there, with its track, crate
    /// places and history, instead of a new file beside a missing one.
    pub fn files_moved(&mut self, root_id: i64, moved: &[(i64, Vec<Vec<u8>>)]) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let profile = transaction.query_row(
            "SELECT name_profile FROM library_root WHERE id = ?1",
            [root_id],
            |row| row.get::<_, String>(0),
        )?;
        let profile =
            NameProfile::parse(&profile).ok_or(Error::InvalidStoredValue("name profile"))?;
        for (file_id, components) in moved {
            let path = RelPath::from_components(components, Flavor::Posix)
                .map_err(|_| Error::InvalidStoredValue("moved path"))?;
            // A row already at the new path (a file missing there, with its own track) would be
            // revived by the next scan with this file's content: refused, so it is said, rather
            // than ignored.
            let changed = transaction.execute(
                "UPDATE OR IGNORE file SET path_raw = ?3, path_key = ?4 \
                 WHERE id = ?1 AND root_id = ?2",
                params![
                    file_id,
                    root_id,
                    path.as_stored(),
                    profile.key(&path).as_bytes()
                ],
            )?;
            if changed == 0 {
                return Err(Error::Refused(format!(
                    "openCrate already lists a file at {}, so the library could not follow the \
                     move there",
                    String::from_utf8_lossy(
                        &path
                            .as_stored()
                            .iter()
                            .map(|byte| if *byte == 0 { b'/' } else { *byte })
                            .collect::<Vec<u8>>()
                    )
                )));
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn apply_scan(
        &mut self,
        root_id: i64,
        pass: &ScanPass,
        now_ms: i64,
    ) -> Result<ScanOutcome> {
        let mut outcome = ScanOutcome::default();
        if pass.root_availability != Availability::Available {
            outcome.skipped = true;
            return Ok(outcome);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let profile = transaction
            .query_row(
                "SELECT name_profile FROM library_root WHERE id = ?1",
                [root_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Error::UnknownRoot(root_id),
                other => other.into(),
            })?;
        let mut profile =
            NameProfile::parse(&profile).ok_or(Error::InvalidStoredValue("name profile"))?;

        if profile != NameProfile::Exact
            && first_collision(profile, pass.files.iter().map(|file| &file.path)).is_some()
        {
            rekey(
                &transaction,
                root_id,
                NameProfile::Exact,
                ProfileSource::Default,
            )?;
            profile = NameProfile::Exact;
            outcome.demoted_profile = true;
        }

        let existing = load_existing(&transaction, root_id)?;
        let present_before = existing.values().filter(|row| row.present).count();
        let mut observed_keys: HashSet<Vec<u8>> = HashSet::with_capacity(pass.files.len());

        let mut insert = transaction.prepare(
            "INSERT INTO file (root_id, path_raw, path_key, lifecycle, presence, size, mtime_ms, \
             container, file_token, last_seen_ms, created_ms) \
             VALUES (?1, ?2, ?3, 'active', 'present', ?4, ?5, ?6, ?7, ?8, ?8)",
        )?;
        let mut update = transaction.prepare(
            "UPDATE file SET path_raw = ?2, size = ?3, mtime_ms = ?4, container = ?5, \
             file_token = ?6, presence = 'present', missing_since_ms = NULL, last_seen_ms = ?7, \
             tags_mtime_ms = CASE WHEN ?8 THEN NULL ELSE tags_mtime_ms END \
             WHERE id = ?1",
        )?;
        for file in &pass.files {
            let key = profile.key(&file.path).as_bytes().to_vec();
            if !observed_keys.insert(key.clone()) {
                continue;
            }
            match existing.get(&key) {
                None => {
                    insert.execute(params![
                        root_id,
                        file.path.as_stored(),
                        key,
                        file.size,
                        file.mtime_ms,
                        file.container,
                        file.file_token,
                        now_ms,
                    ])?;
                    outcome.inserted += 1;
                }
                Some(row) => {
                    let respelled = row.raw != file.path.as_stored();
                    let changed = row.size != file.size
                        || row.mtime_ms != file.mtime_ms
                        || row.container != file.container;
                    update.execute(params![
                        row.id,
                        file.path.as_stored(),
                        file.size,
                        file.mtime_ms,
                        file.container,
                        file.file_token,
                        now_ms,
                        changed,
                    ])?;
                    if !row.present {
                        outcome.revived += 1;
                    } else if changed {
                        outcome.updated += 1;
                    } else if respelled {
                        outcome.respelled += 1;
                    } else {
                        outcome.unchanged += 1;
                    }
                }
            }
        }

        let existing_dirs: HashSet<Vec<u8>> = pass
            .existing_dirs
            .iter()
            .chain(&pass.enumerated_dirs)
            .map(|dir| dir_key(profile, dir))
            .collect();
        let enumerated: HashSet<Vec<u8>> = pass
            .enumerated_dirs
            .iter()
            .map(|dir| dir_key(profile, dir))
            .collect();
        let mut gone = Vec::new();
        for (key, row) in &existing {
            if !row.present || observed_keys.contains(key) {
                continue;
            }
            let path = RelPath::from_stored(&row.raw, Flavor::Posix)
                .map_err(|_| Error::InvalidStoredValue("file path"))?;
            let components: Vec<Vec<u8>> = path.components().map(<[u8]>::to_vec).collect();
            let parent = &components[..components.len() - 1];
            let established = absence_established(profile, parent, &existing_dirs, &enumerated);
            if may_mark_missing(pass.root_availability, established) {
                gone.push(row.id);
            }
        }
        if !gone.is_empty() {
            if hold_mass_disappearance(present_before, gone.len()) && !pass.confirm_mass_missing {
                outcome.held_missing = gone.len();
            } else {
                let mut mark = transaction.prepare(
                    "UPDATE file SET presence = 'missing', missing_since_ms = ?2 WHERE id = ?1",
                )?;
                for id in &gone {
                    mark.execute(params![id, now_ms])?;
                }
                outcome.marked_missing = gone.len();
            }
        }
        drop((insert, update));
        transaction.commit()?;
        Ok(outcome)
    }
}
