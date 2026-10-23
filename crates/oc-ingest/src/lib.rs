//! Pure rules for watching a download folder: which names are still being downloaded, whether a
//! file has stopped changing, and which entries make up one release. No clock and no database:
//! the caller passes the time in, and decides what to do with
//! the answer. Only `list` touches the file system, and only to read. Nothing here ever
//! suggests touching a file in the download folder; a later slice copies a ready release into
//! staging.

pub mod classify;
pub mod list;
pub mod release;
pub mod stability;

pub use classify::{classify, path_has_partial, EntryKind};
pub use list::{list_folder, Listing};
pub use release::{evaluate, Entry, ReleaseKey, ReleaseState, ReleaseStatus, Waiting};
pub use stability::{Observation, Stability, StabilityConfig, StabilityTracker};
