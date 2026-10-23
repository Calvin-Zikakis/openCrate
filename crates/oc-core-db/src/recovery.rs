use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};
use tempfile::{Builder, NamedTempFile};

use crate::{
    check_integrity, clean_shutdown, own_directory, read_connection, refuse_orphan_journals,
    regular_file_exists, schema_version, set_clean_shutdown, validate_header, Error, Result,
    DATABASE_NAME,
};

pub(super) fn snapshot(source: &Connection, directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let version = schema_version(source)?;
    let temporary = Builder::new()
        .prefix(&format!("schema-{version}-"))
        .suffix(".partial")
        .tempfile_in(directory)?;
    copy_database(source, temporary.path())?;
    let destination = temporary.path().with_extension("sqlite3");
    publish(temporary, &destination)?;
    Ok(destination)
}

/// Restore into a new database directory. Never replaces an existing database.
///
/// The source must be a standalone snapshot, not a copied live database missing its WAL.
/// The caller chooses when to switch to the restored directory.
pub fn restore(snapshot: &Path, destination_directory: &Path) -> Result<PathBuf> {
    if !regular_file_exists(snapshot)? {
        return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
    }
    let source = read_connection(snapshot)?;
    validate_header(&source)?;
    check_integrity(&source)?;
    clean_shutdown(&source)?;
    let (directory, _owner) = own_directory(destination_directory)?;
    let destination = directory.join(DATABASE_NAME);
    if regular_file_exists(&destination)? {
        return Err(Error::DestinationExists);
    }
    refuse_orphan_journals(&directory)?;
    let temporary = Builder::new()
        .prefix("restore-")
        .suffix(".partial")
        .tempfile_in(&directory)?;
    copy_database(&source, temporary.path())?;
    let restored =
        Connection::open_with_flags(temporary.path(), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    set_clean_shutdown(&restored, true)?;
    restored.close().map_err(|(_, error)| error)?;
    publish(temporary, &destination)?;
    Ok(destination)
}

fn copy_database(source: &Connection, destination: &Path) -> Result<()> {
    let mut target = Connection::open_with_flags(destination, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    {
        let backup = Backup::new(source, &mut target)?;
        let mut retries = 0;
        loop {
            match backup.step(256)? {
                StepResult::Done => break,
                StepResult::More => retries = 0,
                StepResult::Busy | StepResult::Locked => {
                    retries += 1;
                    if retries >= 20 {
                        return Err(Error::BackupBusy);
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                _ => return Err(Error::BackupBusy),
            }
        }
    }
    // A snapshot must stand alone, even when its source uses WAL.
    target.pragma_update(None, "journal_mode", "DELETE")?;
    check_integrity(&target)?;
    target.close().map_err(|(_, error)| error)?;
    Ok(())
}

fn publish(temporary: NamedTempFile, destination: &Path) -> Result<()> {
    temporary.as_file().sync_all()?;
    let file = temporary
        .persist_noclobber(destination)
        .map_err(|error| Error::Io(error.error))?;
    file.sync_all()?;
    #[cfg(unix)]
    if let Some(directory) = destination.parent() {
        fs::File::open(directory)?.sync_all()?;
    }
    Ok(())
}
