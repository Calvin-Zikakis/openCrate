//! rekordbox's own library (`master.db`), read only from copies taken while rekordbox is closed.

mod backups;
mod read;
mod snapshot;
#[cfg(test)]
mod tests;

pub use backups::{list_backups, prune, restore, set_pinned};
pub use rbl_db::{DbError, LibraryLocation};
pub use read::read_snapshot;
#[cfg(test)]
use snapshot::{folder_stamp, plist_version};
pub use snapshot::{rekordbox_app_version, take_snapshot, Snapshot};
