//! Publishing a staged file into the library so that a crash at any moment loses nothing and
//! duplicates nothing (review finding R1).
//!
//! The protocol, per file:
//!
//! 1. The operation is recorded (`Planned`) with the source, the destination and the content
//!    identity (size and SHA-256) the source had when it was planned.
//! 2. Before the one external side effect, the intent is recorded (`Publishing`).
//! 3. The staged file is hard-linked to the destination. A hard link fails if the destination
//!    exists, which makes publication no-clobber in a single atomic step. A check followed by an
//!    ordinary rename would not be.
//! 4. The database commit that makes the file active records the operation as `Recorded` in the
//!    same transaction.
//! 5. Only then is the staged name removed (`Cleaned`).
//!
//! Recovery is the same code as running: every step looks at what is really on disk and at the
//! recorded phase, so a restart converges whatever the crash point. A destination that holds
//! something other than this operation's file is never overwritten; the operation is rejected and
//! the staged file stays.
//!
//! The staging folder must be on the destination's volume (a hard link cannot cross volumes) and
//! outside the synced library, so a sync tool never sees a half-written or duplicate name. A file
//! that starts on another volume is first copied there with [`stage_copy`].
//!
//! Hard links need a file system that supports them (NTFS, APFS, ext4 do; exFAT and some network
//! shares do not). On such a volume publication reports [`Error::LinksUnsupported`] instead of
//! falling back to something that could clobber; the fallback is an owner decision.

mod fault;
mod identity;
mod journal;
mod run;
mod stage;

#[cfg(test)]
mod tests;

pub use fault::{Crash, FaultPoint, Faults, NoFaults};
pub use identity::{identity_of, Identity};
pub use journal::{MemoryStore, Operation, Phase, Rejection, Store};
pub use run::{link_no_clobber, plan, Executor, Outcome};
pub use stage::{stage_copy, sweep_partials, PART_SUFFIX};

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// An injected crash (tests) or a simulated power loss.
    #[error("crashed at {0:?}")]
    Crash(FaultPoint),
    #[error("destination already exists")]
    DestinationExists,
    #[error("this file system cannot hard-link, so a no-clobber publication is not possible here")]
    LinksUnsupported,
    #[error("the staged file and the destination are on different volumes")]
    CrossVolume,
    #[error("the store failed: {0}")]
    Store(String),
    #[error("file system error: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
