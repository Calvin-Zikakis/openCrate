//! Lossless path identity for openCrate: stored path form, name profiles and keys,
//! rekordbox Location mapping, read-only volume probes and the file/match states.
//!
//! Proposed contract: `docs/design/path-identity-and-availability.md`. Raw bytes are
//! the truth; a key may miss a match but must never invent one. This crate has no
//! database and never writes to the file system.

pub mod component;
pub mod location;
pub mod probe;
pub mod profile;
pub mod state;
pub mod walk;
pub mod wtf8;

pub use component::{
    bytes_to_os_string, check_component, os_str_to_bytes, ComponentFault, Flavor, OsNameError,
    PathError, RelPath,
};
pub use location::{
    prefix_histogram, resolve, LocationError, LocationPath, Resolution, RootAliases, Shape,
};
pub use probe::{
    check_new_root, probe_root, relation, Axis, FileId, Probe, RootError, RootRelation,
};
pub use profile::{first_collision, NameKey, NameProfile, KEY_SCHEME_VERSION};
pub use state::{
    classify_match, hold_mass_disappearance, match_status, may_claim_absent, may_mark_missing,
    next_presence, Availability, DiskCheck, Lifecycle, MatchKind, MatchStatus, Presence,
    HOLD_MIN_FILES, HOLD_PERCENT,
};
pub use walk::{container_for, walk_root, DirPath, WalkOptions, WalkReport, WalkedFile};
