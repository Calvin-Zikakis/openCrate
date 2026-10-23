//! The executor: one loop that looks at the recorded phase and at the disk and does the next
//! thing. Running a fresh operation and recovering a crashed one are the same code.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use oc_paths::FileId;

use crate::fault::{FaultPoint, Faults};
use crate::identity::{identity_of, Identity};
use crate::journal::{Operation, Phase, Rejection, Store};
use crate::stage::sync_parent;
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The destination is the active file and the staged name is gone.
    Published,
    /// Stopped without changing the destination; the staged file is kept.
    Rejected(Rejection),
}

/// Describe what is to be published. Reads the staged file once to fix its identity.
pub fn plan(id: u64, source: &Path, destination: &Path) -> Result<Operation> {
    let expected = identity_of(source)?.ok_or(Error::Io(io::ErrorKind::NotFound.into()))?;
    Ok(Operation {
        id,
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        expected,
        phase: Phase::Planned,
    })
}

/// Create `destination` as a second name for `source`, atomically and only if nothing is there.
/// A hard link fails when the destination exists, so a file that arrives concurrently is never
/// replaced. Both paths must be on one volume.
pub fn link_no_clobber(source: &Path, destination: &Path) -> Result<()> {
    match fs::hard_link(source, destination) {
        Ok(()) => {
            sync_parent(destination);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(Error::DestinationExists),
        Err(error) if is_cross_volume(&error) => Err(Error::CrossVolume),
        Err(error) if is_unsupported(&error) => Err(Error::LinksUnsupported),
        Err(error) => Err(error.into()),
    }
}

fn is_cross_volume(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::CrossesDevices
}

fn is_unsupported(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Unsupported | io::ErrorKind::PermissionDenied
    )
}

/// What is on disk for an operation right now.
struct Seen {
    source: Option<Identity>,
    destination: Option<Identity>,
    /// Both names are the same file, so the destination is certainly ours.
    linked: bool,
}

fn observe(operation: &Operation) -> Result<Seen> {
    let source = identity_of(&operation.source)?;
    let destination = identity_of(&operation.destination)?;
    let linked = match (&source, &destination) {
        (Some(_), Some(_)) => {
            let a = FileId::of(&operation.source)?;
            let b = FileId::of(&operation.destination)?;
            a == b
        }
        _ => false,
    };
    Ok(Seen {
        source,
        destination,
        linked,
    })
}

pub struct Executor<'a, S: Store> {
    store: &'a mut S,
    faults: &'a mut dyn Faults,
}

impl<'a, S: Store> Executor<'a, S> {
    pub fn new(store: &'a mut S, faults: &'a mut dyn Faults) -> Self {
        Self { store, faults }
    }

    fn point(&mut self, point: FaultPoint) -> Result<()> {
        self.faults
            .point(point)
            .map_err(|crash| Error::Crash(crash.0))
    }

    /// Carry an operation as far as it can go: from `Planned` for a new one, or from whatever
    /// phase was recorded for one found at startup.
    pub fn run(&mut self, mut operation: Operation) -> Result<Outcome> {
        loop {
            match operation.phase {
                Phase::Planned => self.start(&mut operation)?,
                Phase::Publishing => self.publish(&mut operation)?,
                Phase::Recorded => self.clean(&mut operation)?,
                Phase::Cleaned => return Ok(Outcome::Published),
                Phase::Rejected(reason) => return Ok(Outcome::Rejected(reason)),
            }
        }
    }

    /// Resume every operation the store says is unfinished. Returns how each ended.
    pub fn recover_all(&mut self) -> Result<Vec<(u64, Outcome)>> {
        let mut ended = Vec::new();
        for operation in self.store.unfinished()? {
            let id = operation.id;
            ended.push((id, self.run(operation)?));
        }
        Ok(ended)
    }

    fn reject(&mut self, operation: &mut Operation, reason: Rejection) -> Result<()> {
        operation.phase = Phase::Rejected(reason);
        self.store.save(operation)
    }

    /// Planned: nothing external has happened, so the only question is whether the staged file
    /// is still the one that was planned.
    fn start(&mut self, operation: &mut Operation) -> Result<()> {
        // Idempotent: the record exists from now on, so a crash anywhere later is recoverable.
        self.store.save(operation)?;
        if identity_of(&operation.source)? != Some(operation.expected) {
            return self.reject(operation, Rejection::SourceChanged);
        }
        self.point(FaultPoint::BeforeIntentSaved)?;
        operation.phase = Phase::Publishing;
        self.store.save(operation)?;
        self.point(FaultPoint::AfterIntentSaved)
    }

    /// Publishing: the link may or may not exist. Decide from the disk, never from the record.
    fn publish(&mut self, operation: &mut Operation) -> Result<()> {
        let seen = observe(operation)?;
        match (&seen.source, &seen.destination) {
            // The destination is this operation's file: either we linked it, or an earlier run did.
            (Some(source), Some(destination))
                if seen.linked
                    && *source == operation.expected
                    && *destination == operation.expected =>
            {
                self.record(operation)
            }
            // The staged name is gone but the destination holds exactly the planned content.
            // Nothing of ours removes the staged name before recording, so this means someone
            // else did; the content is safe at the destination, so record it.
            (None, Some(destination)) if *destination == operation.expected => {
                self.record(operation)
            }
            // Something that is not our link is there. Never overwrite it.
            (_, Some(_)) => self.reject(operation, Rejection::DestinationOccupied),
            (Some(source), None) if *source == operation.expected => {
                self.point(FaultPoint::BeforeLink)?;
                match link_no_clobber(&operation.source, &operation.destination) {
                    // The next turn of the loop sees the link and records it.
                    Ok(()) => self.point(FaultPoint::AfterLink),
                    // Lost a race with a file that arrived just now: the next turn rejects it.
                    Err(Error::DestinationExists) => Ok(()),
                    Err(error) => Err(error),
                }
            }
            _ => self.reject(operation, Rejection::SourceChanged),
        }
    }

    fn record(&mut self, operation: &mut Operation) -> Result<()> {
        self.point(FaultPoint::BeforeRecordCommit)?;
        operation.phase = Phase::Recorded;
        self.store.commit_record(operation)?;
        self.point(FaultPoint::AfterRecordCommit)
    }

    /// Recorded: remove the staged name, but only while the destination still holds the file.
    fn clean(&mut self, operation: &mut Operation) -> Result<()> {
        let seen = observe(operation)?;
        if seen.destination != Some(operation.expected) {
            // The only copy may be the staged one now. Keep it and say so.
            return self.reject(operation, Rejection::DestinationChanged);
        }
        if seen.source.is_some() {
            if seen.source != Some(operation.expected) && !seen.linked {
                // A different file took the staged name; it is not ours to delete.
                return self.reject(operation, Rejection::SourceChanged);
            }
            self.point(FaultPoint::BeforeRemoveStaged)?;
            remove_staged(&operation.source)?;
            self.point(FaultPoint::AfterRemoveStaged)?;
        }
        operation.phase = Phase::Cleaned;
        self.store.save(operation)
    }
}

fn remove_staged(path: &PathBuf) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
