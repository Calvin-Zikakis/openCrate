//! SQLite ownership and recovery. Callers must provide a private, local app-data directory.

pub mod catalog;
pub mod crates;
pub mod downloads;
pub mod health;
pub mod library;
pub mod matching;
pub mod mirror;
pub mod publish_store;
mod recovery;
pub mod roots;
pub mod scan;
pub mod service;
pub mod synthetic;
mod worker;

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use thiserror::Error;

pub use recovery::restore;
pub use worker::{ReaderLease, ReaderPool, Writer};

const APPLICATION_ID: i32 = 0x4f43_5241;
const DATABASE_NAME: &str = "library.sqlite3";
const MIGRATIONS: &[&str] = &[
    include_str!("schema/001_lifecycle.sql"),
    include_str!("schema/002_library.sql"),
    include_str!("schema/003_rekordbox_mirror.sql"),
    include_str!("schema/004_catalog.sql"),
    include_str!("schema/005_watch_folder.sql"),
    include_str!("schema/006_publish_operation.sql"),
    include_str!("schema/007_watch_baseline.sql"),
    include_str!("schema/008_crates.sql"),
    include_str!("schema/009_library_order.sql"),
    include_str!("schema/010_reread_tags.sql"),
    include_str!("schema/011_imported_release.sql"),
    include_str!("schema/012_app_setting.sql"),
    include_str!("schema/013_waveform.sql"),
];

#[derive(Debug, Error)]
pub enum Error {
    #[error("database is already owned by another openCrate instance")]
    AlreadyOpen,
    #[error("database is not an openCrate database")]
    ForeignDatabase,
    #[error("database schema {found} is newer than supported schema {supported}")]
    FutureSchema { found: i32, supported: i32 },
    #[error("database has an invalid schema version")]
    InvalidSchema,
    #[error("database integrity check failed")]
    Integrity,
    #[error("database lifecycle state is invalid")]
    InvalidState,
    #[error("database path must be a regular file, not a symlink")]
    InvalidFile,
    #[error("recovery destination already exists; choose a new app-data directory")]
    DestinationExists,
    #[error(
        "orphan SQLite journal files exist; preserve them and choose a new app-data directory"
    )]
    OrphanJournal,
    #[error("SQLite did not enable WAL mode")]
    WalUnavailable,
    #[error("SQLite snapshot remained busy; retry when the source is idle")]
    BackupBusy,
    #[error("a window must start at zero or later and hold 1 to {max} rows", max = library::MAX_WINDOW)]
    InvalidWindow,
    #[error("invalid synthetic library size: {0}")]
    InvalidSyntheticSize(u32),
    #[error("rekordbox export could not be read: {0}")]
    Export(#[from] oc_rekordbox::xml::Error),
    #[error("{0}")]
    Root(#[from] oc_paths::RootError),
    #[error("that is not a valid rekordbox Location prefix")]
    InvalidAlias,
    #[error("no rekordbox export with id {0}")]
    UnknownSource(i64),
    #[error("root path cannot be recorded as a Location prefix")]
    InvalidRoot,
    #[error("no library root with id {0}")]
    UnknownRoot(i64),
    #[error("no crate with id {0}")]
    UnknownCrate(i64),
    #[error("tracks can only be arranged from the library's own sorts")]
    InvalidArrangeSort,
    #[error("A crate needs a name of 1 to 100 characters.")]
    InvalidCrateName,
    #[error("no download folder with id {0}")]
    UnknownWatchFolder(i64),
    #[error("That folder is, contains or lies inside your music library. Download folders must be separate from it.")]
    WatchOverlapsLibrary,
    #[error("That folder is, contains or lies inside another download folder.")]
    WatchOverlapsWatch,
    #[error("That folder is, contains or lies inside a download folder. Your music library must be separate from your download folders.")]
    RootOverlapsWatch,
    #[error("two files would share one name key; the profile is too coarse")]
    NameCollision,
    #[error("stored {0} is not valid")]
    InvalidStoredValue(&'static str),
    #[error("the writer worker has stopped")]
    WorkerStopped,
    #[error("a job on the writer worker panicked")]
    WorkerPanicked,
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Publish(#[from] oc_publish::Error),
    #[error("database operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenStatus {
    Created,
    Clean,
    CheckedAfterUncleanShutdown,
}

/// One synchronous writer owner. Move it to a worker thread, never the UI thread.
pub struct Database {
    // Declared first so the connection closes before the directory lock is released.
    connection: Connection,
    _owner: OwnerLock,
    directory: PathBuf,
    status: OpenStatus,
}

/// The cooperative directory lock, held for as long as this value lives.
///
/// A lock belongs to the open file description, which a child process shares with this
/// descriptor from `fork` until it execs. If another thread spawns a process at that moment,
/// closing the descriptor alone leaves the directory locked, so release it explicitly.
struct OwnerLock(File);

impl Drop for OwnerLock {
    fn drop(&mut self) {
        // Drop cannot report a failure. If unlocking fails, the operating system still
        // releases the lock once the last copy of the descriptor closes.
        let _ = self.0.unlock();
    }
}

impl Database {
    pub fn open(directory: &Path) -> Result<Self> {
        let (directory, owner) = own_directory(directory)?;
        let path = directory.join(DATABASE_NAME);
        let existing = regular_file_exists(&path)?;
        let status = if existing {
            let probe = read_connection(&path)?;
            validate_header(&probe)?;
            if clean_shutdown(&probe)? {
                OpenStatus::Clean
            } else {
                check_integrity(&probe)?;
                OpenStatus::CheckedAfterUncleanShutdown
            }
        } else {
            refuse_orphan_journals(&directory)?;
            // Reserve the name without replacing a concurrent, non-cooperating arrival.
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            OpenStatus::Created
        };
        let mut connection = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let journal: String =
            connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if journal != "wal" {
            return Err(Error::WalUnavailable);
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
        if existing {
            set_clean_shutdown(&connection, false)?;
        }
        migrate(&mut connection, MIGRATIONS, &directory.join("snapshots"))?;
        set_clean_shutdown(&connection, false)?;
        Ok(Self {
            connection,
            _owner: owner,
            directory,
            status,
        })
    }

    pub fn status(&self) -> OpenStatus {
        self.status
    }

    /// Returns an independent read-only connection; pooling belongs to the app worker layer.
    pub fn reader(&self) -> Result<Connection> {
        read_connection(&self.directory.join(DATABASE_NAME))
    }

    /// Creates a checked standalone database, including committed data still in the WAL.
    pub fn snapshot(&self) -> Result<PathBuf> {
        recovery::snapshot(&self.connection, &self.directory.join("snapshots"))
    }

    /// An ordinary drop intentionally leaves the dirty marker for next-open validation.
    pub fn close(self) -> Result<()> {
        set_clean_shutdown(&self.connection, true)?;
        self.connection.close().map_err(|(_, error)| error)?;
        Ok(())
    }
}

fn own_directory(directory: &Path) -> Result<(PathBuf, OwnerLock)> {
    fs::create_dir_all(directory)?;
    let directory = directory.canonicalize()?;
    let lock_path = directory.join("owner.lock");
    regular_file_exists(&lock_path)?;
    let owner = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    match owner.try_lock() {
        Ok(()) => Ok((directory, OwnerLock(owner))),
        Err(TryLockError::WouldBlock) => Err(Error::AlreadyOpen),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

fn regular_file_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(Error::InvalidFile),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn refuse_orphan_journals(directory: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        if regular_file_exists(&directory.join(format!("{DATABASE_NAME}{suffix}")))? {
            return Err(Error::OrphanJournal);
        }
    }
    Ok(())
}

fn read_connection(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(2))?;
    connection.pragma_update(None, "query_only", true)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    Ok(connection)
}

fn schema_version(connection: &Connection) -> Result<i32> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

fn validate_header(connection: &Connection) -> Result<()> {
    let application_id: i32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(Error::ForeignDatabase);
    }
    let version = schema_version(connection)?;
    let supported = i32::try_from(MIGRATIONS.len()).map_err(|_| Error::InvalidSchema)?;
    if version > supported {
        return Err(Error::FutureSchema {
            found: version,
            supported,
        });
    }
    if version < 1 {
        return Err(Error::InvalidSchema);
    }
    Ok(())
}

fn check_integrity(connection: &Connection) -> Result<()> {
    let mut statement = connection.prepare("PRAGMA integrity_check")?;
    let mut rows = statement.query([])?;
    let first: Option<String> = rows.next()?.map(|row| row.get(0)).transpose()?;
    if first.as_deref() != Some("ok") || rows.next()?.is_some() {
        return Err(Error::Integrity);
    }
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    if statement.query([])?.next()?.is_some() {
        return Err(Error::Integrity);
    }
    Ok(())
}

fn clean_shutdown(connection: &Connection) -> Result<bool> {
    let mut statement = connection.prepare("SELECT id, clean_shutdown FROM app_state")?;
    let mut rows = statement.query([])?;
    let Some(row) = rows.next()? else {
        return Err(Error::InvalidState);
    };
    let id: i64 = row.get(0)?;
    let clean: i64 = row.get(1)?;
    if id != 1 || !matches!(clean, 0 | 1) || rows.next()?.is_some() {
        return Err(Error::InvalidState);
    }
    Ok(clean == 1)
}

fn set_clean_shutdown(connection: &Connection, clean: bool) -> Result<()> {
    if connection.execute(
        "UPDATE app_state SET clean_shutdown = ?1 WHERE id = 1",
        [clean],
    )? != 1
    {
        return Err(Error::InvalidState);
    }
    Ok(())
}

fn migrate(connection: &mut Connection, migrations: &[&str], snapshots: &Path) -> Result<()> {
    let current = usize::try_from(schema_version(connection)?).map_err(|_| Error::InvalidSchema)?;
    for (index, sql) in migrations.iter().enumerate().skip(current) {
        // Only an existing schema is worth a snapshot; creation starts from nothing.
        if current > 0 {
            recovery::snapshot(connection, snapshots)?;
        }
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
        let version = i32::try_from(index + 1).map_err(|_| Error::InvalidSchema)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod crates_tests;
#[cfg(test)]
mod downloads_tests;
#[cfg(test)]
mod health_tests;
#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod mirror_tests;
#[cfg(test)]
mod publish_tests;
#[cfg(test)]
mod scan_flow_tests;
#[cfg(test)]
mod tests;
