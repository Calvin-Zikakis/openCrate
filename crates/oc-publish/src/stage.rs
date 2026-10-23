//! Getting a file onto the destination volume, outside the synced library, before publication.
//!
//! The copy is written under a `.part` name, flushed, verified against the source and only then
//! renamed to its staged name, so a staged name always holds a complete, verified file. A crash
//! leaves at most a `.part` file, which [`sweep_partials`] removes on restart.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::fault::{FaultPoint, Faults};
use crate::identity::{identity_of, Identity};
use crate::{Error, Result};

pub const PART_SUFFIX: &str = ".part";

fn crash(faults: &mut dyn Faults, point: FaultPoint) -> Result<()> {
    faults.point(point).map_err(|crash| Error::Crash(crash.0))
}

/// Copy `source` to `staged` (a path inside the publication staging folder) and return the
/// identity the copy was verified to have. `staged` must not exist. The source is only read.
pub fn stage_copy(source: &Path, staged: &Path, faults: &mut dyn Faults) -> Result<Identity> {
    let part = part_name(staged);
    // A leftover from an earlier crash is ours to replace; nothing else uses a `.part` name here.
    match fs::remove_file(&part) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut from = File::open(source)?;
    let mut to = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)?;
    let mut buffer = vec![0_u8; 1 << 20];
    let mut first = true;
    loop {
        let read = from.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        to.write_all(&buffer[..read])?;
        if first {
            // The file now exists with some, not all, of its content.
            first = false;
            crash(faults, FaultPoint::MidCopy)?;
        }
    }
    to.sync_all()?;
    drop(to);
    let copied = identity_of(&part)?.ok_or(Error::Io(io::ErrorKind::NotFound.into()))?;
    let original = identity_of(source)?.ok_or(Error::Io(io::ErrorKind::NotFound.into()))?;
    if copied != original {
        let _ = fs::remove_file(&part);
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "the copy does not match the source",
        )));
    }
    crash(faults, FaultPoint::BeforeCopyRename)?;
    link_then_drop_part(&part, staged)?;
    sync_parent(staged);
    Ok(copied)
}

/// Give the verified copy its staged name without ever replacing an existing file.
fn link_then_drop_part(part: &Path, staged: &Path) -> Result<()> {
    crate::run::link_no_clobber(part, staged)?;
    fs::remove_file(part)?;
    Ok(())
}

fn part_name(staged: &Path) -> PathBuf {
    let mut name = staged.as_os_str().to_owned();
    name.push(PART_SUFFIX);
    PathBuf::from(name)
}

/// Remove unfinished copies in the publication staging folder. Only files ending in
/// [`PART_SUFFIX`] directly inside `folder` are touched. Returns how many were removed.
pub fn sweep_partials(folder: &Path) -> io::Result<usize> {
    let mut removed = 0;
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let is_part = entry.file_name().to_string_lossy().ends_with(PART_SUFFIX);
        if is_part && entry.file_type()?.is_file() {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Make the new directory entry durable. Not possible on every platform; where it is not, the
/// protocol still converges because recovery looks at the disk, it just may repeat a step.
pub fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}
