//! rekordbox's own library (`master.db`), read only from copies taken while rekordbox is closed.

mod backups;
mod read;
mod snapshot;
#[cfg(test)]
mod tests;

pub use backups::{list_backups, prune, restore, set_pinned};
pub use rbl_db::{DbError, LibraryLocation};

/// rekordbox's installed library on this computer, found from its agent's settings.
pub fn installed_library() -> Result<LibraryLocation, DbError> {
    rbl_db::detect()
}

/// Whether rekordbox or its agent is running; either may be changing the library.
pub fn rekordbox_running() -> bool {
    rbl_db::is_rekordbox_running()
}
pub use read::read_snapshot;
#[cfg(test)]
use snapshot::{folder_stamp, newest_version, plist_version};
pub use snapshot::{rekordbox_app_version, take_snapshot, Snapshot};
