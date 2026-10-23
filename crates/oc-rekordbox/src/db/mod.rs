//! rekordbox's own library (`master.db`), read only from copies taken while rekordbox is closed.

mod snapshot;
#[cfg(test)]
mod tests;

pub use rbl_db::{DbError, LibraryLocation};
#[cfg(test)]
use snapshot::{folder_stamp, plist_version};
pub use snapshot::{rekordbox_app_version, take_snapshot, Snapshot};
