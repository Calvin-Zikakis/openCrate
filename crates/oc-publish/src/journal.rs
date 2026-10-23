//! The durable operation record. In the application this is a table written in the same SQLite
//! transaction as the catalog change; here it is a trait so the protocol can be proven without
//! one. An implementation must make `commit_record` atomic: either the file is active and the
//! phase is `Recorded`, or neither happened.

use std::path::PathBuf;

use crate::identity::Identity;
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// The staged file changed or vanished after it was planned.
    SourceChanged,
    /// Something else is already at the destination. It is left alone.
    DestinationOccupied,
    /// After recording, the destination no longer holds the file, so the staged copy is kept.
    DestinationChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Nothing external has happened.
    Planned,
    /// The link may or may not exist yet.
    Publishing,
    /// The catalog says the destination is the active file.
    Recorded,
    /// The staged name is gone. Final.
    Cleaned,
    /// Stopped for the owner to look at; both files are untouched. Final.
    Rejected(Rejection),
}

impl Phase {
    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Cleaned | Self::Rejected(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operation {
    pub id: u64,
    /// The staged file: on the destination's volume, outside the synced library.
    pub source: PathBuf,
    pub destination: PathBuf,
    pub expected: Identity,
    pub phase: Phase,
}

pub trait Store {
    /// Durably save the operation, replacing the earlier record with the same id.
    fn save(&mut self, operation: &Operation) -> Result<()>;
    /// In one transaction: make the destination the active file for this operation (idempotent
    /// by operation id) and save the operation with phase `Recorded`.
    fn commit_record(&mut self, operation: &Operation) -> Result<()>;
    /// Operations not yet final, oldest first. Run at startup.
    /// A failure to read must be an error: an empty list would read as "nothing to recover".
    fn unfinished(&self) -> Result<Vec<Operation>>;
}

/// A store that keeps everything in memory. Survives a simulated crash because the test keeps it.
#[derive(Debug, Default)]
pub struct MemoryStore {
    operations: Vec<Operation>,
    /// Operation ids that were made active, in order. Each id may appear at most once.
    pub active: Vec<u64>,
}

impl MemoryStore {
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Operation> {
        self.operations.iter().find(|operation| operation.id == id)
    }
}

impl Store for MemoryStore {
    fn save(&mut self, operation: &Operation) -> Result<()> {
        match self.operations.iter_mut().find(|o| o.id == operation.id) {
            Some(existing) => *existing = operation.clone(),
            None => self.operations.push(operation.clone()),
        }
        Ok(())
    }

    fn commit_record(&mut self, operation: &Operation) -> Result<()> {
        if operation.phase != Phase::Recorded {
            return Err(Error::Store(
                "commit_record needs the Recorded phase".into(),
            ));
        }
        if !self.active.contains(&operation.id) {
            self.active.push(operation.id);
        }
        self.save(operation)
    }

    fn unfinished(&self) -> Result<Vec<Operation>> {
        Ok(self
            .operations
            .iter()
            .filter(|operation| !operation.phase.is_final())
            .cloned()
            .collect())
    }
}
