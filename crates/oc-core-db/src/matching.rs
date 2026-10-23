//! Matching a decoded rekordbox Location to an indexed file. Database only: no file
//! system access, so it works while a root is offline. It never says "missing"; whether
//! a file is on disk is a separate, on-demand check.

use oc_paths::{
    classify_match, match_status, resolve, Flavor, LocationPath, MatchKind, MatchStatus, RelPath,
    Resolution, RootAliases,
};
use rusqlite::{params, Connection, OptionalExtension};

use crate::roots::root_aliases;
use crate::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationMatch {
    pub status: MatchStatus,
    pub kind: Option<MatchKind>,
    pub root_id: Option<i64>,
    pub file_id: Option<i64>,
}

impl LocationMatch {
    fn none(status: MatchStatus) -> Self {
        Self {
            status,
            kind: None,
            root_id: None,
            file_id: None,
        }
    }
}

/// The roots and aliases at one moment. Reload after roots or aliases change.
pub struct Matcher {
    roots: Vec<RootAliases>,
}

impl Matcher {
    pub fn load(connection: &Connection) -> Result<Self> {
        Ok(Self {
            roots: root_aliases(connection)?,
        })
    }

    pub fn match_location(&self, connection: &Connection, decoded: &[u8]) -> Result<LocationMatch> {
        let Ok(location) = LocationPath::parse(decoded) else {
            return Ok(LocationMatch::none(MatchStatus::Unrecognized));
        };
        let resolution = resolve(&location, &self.roots);
        let Resolution::Mapped {
            root_id, remainder, ..
        } = &resolution
        else {
            return Ok(LocationMatch::none(match_status(&resolution, false)));
        };
        let profile = self
            .roots
            .iter()
            .find(|root| root.root_id == *root_id)
            .map(|root| root.profile)
            .ok_or(Error::UnknownRoot(*root_id))?;
        let key = profile.key(remainder);
        let found: Option<(i64, Vec<u8>)> = connection
            .prepare_cached("SELECT id, path_raw FROM file WHERE root_id = ?1 AND path_key = ?2")?
            .query_row(params![root_id, key.as_bytes()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        let status = match_status(&resolution, found.is_some());
        Ok(match found {
            None => LocationMatch {
                root_id: Some(*root_id),
                ..LocationMatch::none(status)
            },
            Some((file_id, raw)) => {
                let stored = RelPath::from_stored(&raw, Flavor::Posix)
                    .map_err(|_| Error::InvalidStoredValue("file path"))?;
                LocationMatch {
                    status,
                    kind: Some(classify_match(&stored, remainder)),
                    root_id: Some(*root_id),
                    file_id: Some(file_id),
                }
            }
        })
    }
}
