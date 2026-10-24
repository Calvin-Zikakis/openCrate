//! Library roots, their Location aliases and availability. A root is where files
//! live on disk; offline is a property of the root and is never copied to files.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use oc_paths::{
    bytes_to_os_string, os_str_to_bytes, Availability, FileId, Flavor, LocationPath, NameProfile,
    RelPath, RootAliases, Shape, KEY_SCHEME_VERSION,
};
use rusqlite::{params, Connection, TransactionBehavior};

use crate::{Database, Error, Result};

macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            #[must_use]
            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

text_enum!(RootKind {
    Local => "local",
    Removable => "removable",
    Network => "network",
});

text_enum!(
    /// How the name profile was chosen. `Default` also covers a demotion after a collision.
    ProfileSource {
        OsQuery => "os_query",
        StatProbe => "stat_probe",
        UserSet => "user_set",
        Default => "default",
    }
);

text_enum!(AliasSource {
    RootPath => "root_path",
    User => "user",
});

pub struct NewRoot<'a> {
    pub path: &'a Path,
    pub label: &'a str,
    pub kind: RootKind,
    pub profile: NameProfile,
    pub profile_source: ProfileSource,
    pub dir_token: Option<Vec<u8>>,
    pub volume_token: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub id: i64,
    pub path_raw: Vec<u8>,
    pub label: String,
    pub kind: RootKind,
    pub profile: NameProfile,
    pub profile_source: ProfileSource,
    pub key_scheme: u32,
    pub dir_token: Option<Vec<u8>>,
    pub volume_token: Option<Vec<u8>>,
    pub availability: Availability,
    pub availability_reason: String,
    pub availability_checked_ms: Option<i64>,
    pub last_available_ms: Option<i64>,
}

fn stored<T>(value: Option<T>, what: &'static str) -> Result<T> {
    value.ok_or(Error::InvalidStoredValue(what))
}

pub fn list_roots(connection: &Connection) -> Result<Vec<Root>> {
    let mut statement = connection.prepare(
        "SELECT id, path_raw, label, kind, name_profile, name_profile_source, key_scheme, \
         dir_token, volume_token, availability, availability_reason, availability_checked_ms, \
         last_available_ms FROM library_root ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, u32>(6)?,
            row.get::<_, Option<Vec<u8>>>(7)?,
            row.get::<_, Option<Vec<u8>>>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, Option<i64>>(11)?,
            row.get::<_, Option<i64>>(12)?,
        ))
    })?;
    let mut roots = Vec::new();
    for row in rows {
        let (
            id,
            path_raw,
            label,
            kind,
            profile,
            source,
            key_scheme,
            dir,
            volume,
            availability,
            reason,
            checked,
            last,
        ) = row?;
        roots.push(Root {
            id,
            path_raw,
            label,
            kind: stored(RootKind::parse(&kind), "root kind")?,
            profile: stored(NameProfile::parse(&profile), "name profile")?,
            profile_source: stored(ProfileSource::parse(&source), "profile source")?,
            key_scheme,
            dir_token: dir,
            volume_token: volume,
            availability: stored(Availability::parse(&availability), "availability")?,
            availability_reason: reason,
            availability_checked_ms: checked,
            last_available_ms: last,
        });
    }
    Ok(roots)
}

fn split_components(stored: &[u8]) -> Vec<Vec<u8>> {
    if stored.is_empty() {
        Vec::new()
    } else {
        stored
            .split(|byte| *byte == 0)
            .map(<[u8]>::to_vec)
            .collect()
    }
}

/// Every root with its profile and aliases, ready for [`oc_paths::resolve`].
pub fn root_aliases(connection: &Connection) -> Result<Vec<RootAliases>> {
    let mut roots: Vec<RootAliases> = Vec::new();
    let mut statement = connection.prepare(
        "SELECT r.id, r.name_profile, a.shape, a.components FROM library_root r \
         LEFT JOIN root_alias a ON a.root_id = r.id ORDER BY r.id, a.id",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        if roots.last().is_none_or(|last| last.root_id != id) {
            let profile: String = row.get(1)?;
            roots.push(RootAliases {
                root_id: id,
                profile: stored(NameProfile::parse(&profile), "name profile")?,
                aliases: Vec::new(),
            });
        }
        let shape: Option<String> = row.get(2)?;
        if let (Some(shape), Some(root)) = (shape, roots.last_mut()) {
            let components: Vec<u8> = row.get(3)?;
            let alias = LocationPath::from_parts(
                stored(Shape::parse(&shape), "alias shape")?,
                split_components(&components),
            )
            .map_err(|_| Error::InvalidStoredValue("alias"))?;
            root.aliases.push(alias);
        }
    }
    Ok(roots)
}

fn insert_alias(
    connection: &Connection,
    root_id: i64,
    alias: &LocationPath,
    source: AliasSource,
    now_ms: i64,
) -> Result<bool> {
    let added = connection.execute(
        "INSERT OR IGNORE INTO root_alias (root_id, shape, components, source, added_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            root_id,
            alias.shape().to_text(),
            alias.components_stored(),
            source.as_str(),
            now_ms
        ],
    )?;
    Ok(added == 1)
}

impl Database {
    /// Record a root already validated against the file system (see
    /// `oc_paths::check_new_root`). Its own path becomes its first alias.
    pub fn add_root(&mut self, new: &NewRoot<'_>, now_ms: i64) -> Result<i64> {
        let alias = LocationPath::from_os_path(new.path).map_err(|_| Error::InvalidRoot)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO library_root (path_raw, label, kind, name_profile, name_profile_source, \
             key_scheme, dir_token, volume_token, availability, availability_reason, \
             availability_checked_ms, last_available_ms, created_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'available', 'registered', ?9, ?9, ?9)",
            params![
                os_str_to_bytes(new.path.as_os_str()),
                new.label,
                new.kind.as_str(),
                new.profile.as_str(),
                new.profile_source.as_str(),
                KEY_SCHEME_VERSION,
                new.dir_token,
                new.volume_token,
                now_ms,
            ],
        )?;
        let id = transaction.last_insert_rowid();
        insert_alias(&transaction, id, &alias, AliasSource::RootPath, now_ms)?;
        transaction.commit()?;
        Ok(id)
    }

    /// Add a prefix an export may use for this root. Returns false if it was already known.
    pub fn add_alias(&mut self, root_id: i64, alias: &LocationPath, now_ms: i64) -> Result<bool> {
        let known: bool = self.connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM library_root WHERE id = ?1)",
            [root_id],
            |row| row.get(0),
        )?;
        if !known {
            return Err(Error::UnknownRoot(root_id));
        }
        insert_alias(&self.connection, root_id, alias, AliasSource::User, now_ms)
    }

    /// The owner confirmed that the folder now at this path is the library: record its current
    /// identity and mark it available. Nothing else changes.
    pub fn confirm_root(
        &mut self,
        root_id: i64,
        dir_token: Option<&[u8]>,
        now_ms: i64,
    ) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE library_root SET dir_token = ?2, availability = 'available', \
             availability_reason = 'confirmed by the owner', availability_checked_ms = ?3, \
             last_available_ms = ?3 WHERE id = ?1",
            params![root_id, dir_token, now_ms],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(Error::UnknownRoot(root_id))
        }
    }

    /// Remove a folder from the library, after the owner confirmed. Its files, aliases and
    /// the tracks that only those files made are deleted; rekordbox entries that pointed into it
    /// become "not in your folders" again. Artists and releases that end up empty are left and
    /// simply not shown. Nothing on disk is touched. Returns how many files were removed.
    pub fn remove_root(&mut self, root_id: i64) -> Result<usize> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let known: bool = transaction.query_row(
            "SELECT EXISTS (SELECT 1 FROM library_root WHERE id = ?1)",
            [root_id],
            |row| row.get(0),
        )?;
        if !known {
            return Err(Error::UnknownRoot(root_id));
        }
        transaction.execute(
            "UPDATE rb_track SET match_status = 'unmapped', match_kind = NULL, root_id = NULL, \
             file_id = NULL WHERE root_id = ?1 OR file_id IN (SELECT id FROM file WHERE root_id = ?1)",
            [root_id],
        )?;
        transaction.execute(
            "UPDATE track SET bpm = NULL, musical_key = '', derived_source_id = NULL, \
             derived_generation = NULL WHERE derived_source_id IS NOT NULL AND id IN \
             (SELECT track_id FROM file WHERE root_id = ?1)",
            [root_id],
        )?;
        let removed = transaction.execute("DELETE FROM file WHERE root_id = ?1", [root_id])?;
        // Tracks left with no file go below; their undo history must not reach a later track.
        transaction.execute(
            "DELETE FROM edit_history_item WHERE track_id IN (SELECT id FROM track WHERE NOT EXISTS \
             (SELECT 1 FROM file WHERE file.track_id = track.id))",
            [],
        )?;
        transaction.execute(
            "DELETE FROM edit_history WHERE NOT EXISTS \
             (SELECT 1 FROM edit_history_item i WHERE i.history_id = edit_history.id)",
            [],
        )?;
        transaction.execute(
            "DELETE FROM track_fts WHERE rowid IN (SELECT id FROM track WHERE NOT EXISTS \
             (SELECT 1 FROM file WHERE file.track_id = track.id))",
            [],
        )?;
        transaction.execute(
            "DELETE FROM track WHERE NOT EXISTS (SELECT 1 FROM file WHERE file.track_id = track.id)",
            [],
        )?;
        transaction.execute("DELETE FROM root_alias WHERE root_id = ?1", [root_id])?;
        transaction.execute("DELETE FROM library_root WHERE id = ?1", [root_id])?;
        transaction.commit()?;
        Ok(removed)
    }

    /// Store the result of an availability check. Only `Available` advances `last_available_ms`.
    pub fn set_availability(
        &mut self,
        root_id: i64,
        availability: Availability,
        reason: &str,
        now_ms: i64,
    ) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE library_root SET availability = ?2, availability_reason = ?3, \
             availability_checked_ms = ?4, \
             last_available_ms = CASE WHEN ?2 = 'available' THEN ?4 ELSE last_available_ms END \
             WHERE id = ?1",
            params![root_id, availability.as_str(), reason, now_ms],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(Error::UnknownRoot(root_id))
        }
    }

    /// Change a root's profile and recompute every key in one transaction. Refuses, changing
    /// nothing, if two files would share a key.
    pub fn set_name_profile(
        &mut self,
        root_id: i64,
        profile: NameProfile,
        source: ProfileSource,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        rekey(&transaction, root_id, profile, source)?;
        transaction.commit()?;
        Ok(())
    }
}

/// Recompute `path_key` for a root. Two phases, so a permutation of keys cannot trip the
/// unique index halfway: first park every key on a value no real key can have (it starts
/// with NUL), then write the final keys.
pub(crate) fn rekey(
    connection: &Connection,
    root_id: i64,
    profile: NameProfile,
    source: ProfileSource,
) -> Result<()> {
    let known: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM library_root WHERE id = ?1)",
        [root_id],
        |row| row.get(0),
    )?;
    if !known {
        return Err(Error::UnknownRoot(root_id));
    }
    let mut statement = connection.prepare("SELECT id, path_raw FROM file WHERE root_id = ?1")?;
    let rows = statement
        .query_map([root_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    let mut keyed = Vec::with_capacity(rows.len());
    for (id, raw) in rows {
        let path = RelPath::from_stored(&raw, Flavor::Posix)
            .map_err(|_| Error::InvalidStoredValue("file path"))?;
        let key = profile.key(&path).as_bytes().to_vec();
        if let Some(earlier) = seen.insert(key.clone(), raw.clone()) {
            if earlier != raw {
                return Err(Error::NameCollision);
            }
        }
        keyed.push((id, key));
    }
    let mut park = connection.prepare("UPDATE file SET path_key = ?2 WHERE id = ?1")?;
    for (id, _) in &keyed {
        let mut parked = vec![0u8];
        parked.extend_from_slice(&id.to_be_bytes());
        park.execute(params![id, parked])?;
    }
    for (id, key) in &keyed {
        park.execute(params![id, key])?;
    }
    connection.execute(
        "UPDATE library_root SET name_profile = ?2, name_profile_source = ?3, key_scheme = ?4 \
         WHERE id = ?1",
        params![
            root_id,
            profile.as_str(),
            source.as_str(),
            KEY_SCHEME_VERSION
        ],
    )?;
    Ok(())
}

/// Decide whether a root can be trusted right now, from read-only checks. `Unavailable`:
/// the path is missing, not a directory, or unreadable. `Unverified`: a directory is there but
/// it is not the one that was registered (its recorded identity differs), so a different
/// drive or an empty mount point cannot be mistaken for this library.
#[must_use]
pub fn check_root(root: &Root) -> (Availability, String) {
    let Ok(os_path) = bytes_to_os_string(&root.path_raw) else {
        return (
            Availability::Unavailable,
            "path is not representable here".to_owned(),
        );
    };
    let path = Path::new(&os_path);
    match fs::metadata(path) {
        Err(error) => return (Availability::Unavailable, error.kind().to_string()),
        Ok(metadata) if !metadata.is_dir() => {
            return (Availability::Unavailable, "not a directory".to_owned());
        }
        Ok(_) => {}
    }
    if let Some(expected) = &root.dir_token {
        match FileId::of(path).map(|id| id.persistent_token()) {
            Ok(Some(actual)) if &actual != expected => {
                return (
                    Availability::Unverified,
                    "directory identity changed".to_owned(),
                );
            }
            Err(error) => return (Availability::Unavailable, error.kind().to_string()),
            Ok(_) => {}
        }
    }
    (Availability::Available, "reachable".to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootStats {
    pub root_id: i64,
    pub files: i64,
    pub missing: i64,
}

/// File counts per root, for a folder list.
pub fn root_stats(connection: &Connection) -> Result<Vec<RootStats>> {
    let mut statement = connection.prepare(
        "SELECT root_id, COUNT(*), COALESCE(SUM(presence = 'missing'), 0) FROM file GROUP BY root_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RootStats {
            root_id: row.get(0)?,
            files: row.get(1)?,
            missing: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
