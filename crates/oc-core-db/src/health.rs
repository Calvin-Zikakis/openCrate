//! A read-only look over the library for the leftovers of untidy tags: folders whose names
//! carry stray separators, and the same recording filed in more than one place. Nothing is
//! changed; the owner decides what to do with each finding.

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::Result;

/// A folder whose name "Tidy names" would change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UntidyFolder {
    pub root_id: i64,
    /// Relative to the music folder, `/`-separated, for showing only.
    pub folder: String,
    pub suggested: String,
    /// Files below it.
    pub files: usize,
}

/// One recording found in more than one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duplicate {
    pub artist: String,
    pub title: String,
    /// Relative paths, `/`-separated.
    pub files: Vec<String>,
    /// The library files, in the same order.
    pub file_ids: Vec<i64>,
    /// The music folder each file is in, in the same order: two copies can share a relative path.
    pub root_ids: Vec<i64>,
}

/// A file in a music folder whose tags the scan could not read, so it is not in the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokenFile {
    pub root_id: i64,
    /// Relative to the music folder, `/`-separated, for showing only.
    pub path: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub untidy: Vec<UntidyFolder>,
    pub duplicates: Vec<Duplicate>,
    pub broken: Vec<BrokenFile>,
    /// Files looked at.
    pub files: usize,
}

/// Lengths closer than this are the same recording; a radio edit and an extended mix are not.
const SAME_LENGTH_MS: i64 = 2_000;

/// One file of a recording, ordered by length so neighbours can be compared.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Seen {
    length: i64,
    path: String,
    artist: String,
    title: String,
    file_id: i64,
    root_id: i64,
}

fn shown(components: &[&[u8]]) -> String {
    components
        .iter()
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn check(connection: &Connection) -> Result<Health> {
    let mut statement = connection.prepare(
        "SELECT f.root_id, f.path_raw, a.name, t.title, COALESCE(f.duration_ms, t.duration_ms), f.id \
         FROM file f JOIN track t ON t.id = f.track_id JOIN artist a ON a.id = t.artist_id \
         WHERE f.presence = 'present' AND f.lifecycle = 'active' ORDER BY f.root_id, f.path_raw",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    let mut health = Health::default();
    let mut folders: BTreeMap<(i64, String), (String, usize)> = BTreeMap::new();
    let mut recordings: BTreeMap<(String, String), Vec<Seen>> = BTreeMap::new();
    for row in rows {
        let (root_id, path, artist, title, length, file_id) = row?;
        health.files += 1;
        let components: Vec<&[u8]> = path.split(|byte| *byte == 0).collect();
        let Some((_, directories)) = components.split_last() else {
            continue;
        };
        for depth in 0..directories.len() {
            let name = String::from_utf8_lossy(directories[depth]);
            let suggested = oc_tag::tidy_name(&name);
            if suggested != name && !suggested.is_empty() {
                let entry = folders
                    .entry((root_id, shown(&directories[..=depth])))
                    .or_insert((suggested, 0));
                entry.1 += 1;
            }
        }
        let key = (
            oc_tag::tidy_name(&artist).to_lowercase(),
            title.to_lowercase(),
        );
        recordings.entry(key).or_default().push(Seen {
            length,
            path: shown(&components),
            artist,
            title,
            file_id,
            root_id,
        });
    }
    let mut unreadable = connection.prepare(
        "SELECT root_id, path_raw, tags_error FROM file WHERE tags_error IS NOT NULL \
         AND presence = 'present' AND lifecycle = 'active' ORDER BY root_id, path_raw",
    )?;
    for row in unreadable.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Vec<u8>>(1)?,
            row.get::<_, String>(2)?,
        ))
    })? {
        let (root_id, path, error) = row?;
        let components: Vec<&[u8]> = path.split(|byte| *byte == 0).collect();
        health.broken.push(BrokenFile {
            root_id,
            path: shown(&components),
            error,
        });
    }
    health.untidy = folders
        .into_iter()
        .map(|((root_id, folder), (suggested, files))| UntidyFolder {
            root_id,
            folder,
            suggested,
            files,
        })
        .collect();
    for (_, mut found) in recordings {
        found.sort();
        // Group by length: neighbours within the tolerance are one recording.
        let mut group: Vec<Seen> = Vec::new();
        let mut flush = |group: &mut Vec<Seen>| {
            if group.len() > 1 {
                health.duplicates.push(Duplicate {
                    artist: group[0].artist.clone(),
                    title: group[0].title.clone(),
                    files: group.iter().map(|item| item.path.clone()).collect(),
                    file_ids: group.iter().map(|item| item.file_id).collect(),
                    root_ids: group.iter().map(|item| item.root_id).collect(),
                });
            }
            group.clear();
        };
        for item in found {
            if group
                .last()
                .is_some_and(|last| item.length - last.length > SAME_LENGTH_MS)
            {
                flush(&mut group);
            }
            group.push(item);
        }
        flush(&mut group);
    }
    Ok(health)
}

/// Every present file below `folder` (as the check shows it, `/`-separated) in a music folder,
/// with its path inside the music folder as components.
pub fn files_below(
    connection: &Connection,
    root_id: i64,
    folder: &str,
) -> Result<Vec<(i64, Vec<Vec<u8>>)>> {
    let depth = folder.split('/').count();
    let mut statement = connection.prepare(
        "SELECT id, path_raw FROM file WHERE root_id = ?1 AND presence = 'present' \
         AND lifecycle = 'active' ORDER BY path_raw",
    )?;
    let rows = statement.query_map([root_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    let mut found = Vec::new();
    for row in rows {
        let (file_id, path) = row?;
        let components: Vec<&[u8]> = path.split(|byte| *byte == 0).collect();
        // An empty folder name is the whole music folder.
        if folder.is_empty() || (components.len() > depth && shown(&components[..depth]) == folder)
        {
            found.push((
                file_id,
                components.iter().map(|part| part.to_vec()).collect(),
            ));
        }
    }
    Ok(found)
}

/// One copy of a recording filed more than once, with what decides which to keep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Copy {
    pub file_id: i64,
    pub root_id: i64,
    /// Inside the music folder, as components.
    pub relative: Vec<Vec<u8>>,
    pub container: String,
    pub bitrate_kbps: Option<i64>,
    /// rekordbox's id for it, from the last direct read.
    pub rb_key: Option<String>,
    /// rekordbox has analysed it: it has a tempo (a direct read) or beat grid points (an export).
    pub analysed: bool,
    pub cues: i64,
    /// rekordbox playlists it is in.
    pub playlists: i64,
}

pub fn copy_of(connection: &Connection, file_id: i64) -> Result<Option<Copy>> {
    use rusqlite::OptionalExtension;
    let found = connection
        .query_row(
            "SELECT f.root_id, f.path_raw, f.container, f.bitrate_kbps FROM file f \
             WHERE f.id = ?1 AND f.presence = 'present' AND f.lifecycle = 'active'",
            [file_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((root_id, path, container, bitrate_kbps)) = found else {
        return Ok(None);
    };
    let rekordbox: Option<(i64, i64, bool)> = connection
        .query_row(
            "SELECT t.id, t.rb_key, COALESCE(t.bpm, 0) > 0 FROM rb_track t \
             JOIN rb_source s ON s.id = t.rb_source_id \
             WHERE t.file_id = ?1 AND s.kind = 'master_db' AND t.match_status = 'matched' \
             ORDER BY t.rb_key LIMIT 1",
            [file_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let count = |sql: &str, key: i64| -> Result<i64> {
        Ok(connection.query_row(sql, [key], |row| row.get(0))?)
    };
    let (analysed, cues, playlists) = match rekordbox {
        Some((row, key, tempo)) => (
            tempo || count("SELECT COUNT(*) FROM rb_tempo WHERE rb_track_id = ?1", row)? > 0,
            count("SELECT COUNT(*) FROM rb_cue WHERE rb_track_id = ?1", row)?,
            count(
                "SELECT COUNT(DISTINCT i.playlist_id) FROM playlist_item i \
                 JOIN playlist p ON p.id = i.playlist_id JOIN rb_source s ON s.id = p.rb_source_id \
                 WHERE i.rb_key = ?1 AND s.kind = 'master_db'",
                key,
            )?,
        ),
        None => (false, 0, 0),
    };
    Ok(Some(Copy {
        file_id,
        root_id,
        relative: path.split(|byte| *byte == 0).map(<[u8]>::to_vec).collect(),
        container,
        bitrate_kbps,
        rb_key: rekordbox.map(|(_, key, _)| key.to_string()),
        analysed,
        cues,
        playlists,
    }))
}

impl crate::Database {
    /// Copies of a duplicate moved out to the archive leave the library: their file rows go, and
    /// a track left with no file goes too, after its crate places and own fields pass to the
    /// kept copy's track (where that track does not have them already). The next scan would
    /// otherwise call them missing, and the track would stay listed with no file.
    pub fn forget_archived(&mut self, kept: i64, archived: &[i64]) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let kept_track: Option<i64> =
            transaction.query_row("SELECT track_id FROM file WHERE id = ?1", [kept], |row| {
                row.get(0)
            })?;
        for &file_id in archived {
            let track: Option<i64> = transaction.query_row(
                "SELECT track_id FROM file WHERE id = ?1",
                [file_id],
                |row| row.get(0),
            )?;
            transaction.execute(
                "UPDATE rb_track SET match_status = 'unmapped', match_kind = NULL, root_id = NULL, \
                 file_id = NULL WHERE file_id = ?1",
                [file_id],
            )?;
            transaction.execute("DELETE FROM file WHERE id = ?1", [file_id])?;
            let Some(track) = track else { continue };
            let still_filed: bool = transaction.query_row(
                "SELECT EXISTS (SELECT 1 FROM file WHERE track_id = ?1)",
                [track],
                |row| row.get(0),
            )?;
            // With no track for the kept copy to take them, the track stays, crate places and all.
            let Some(kept_track) = kept_track else {
                continue;
            };
            if still_filed {
                continue;
            }
            if kept_track != track {
                transaction.execute(
                    "UPDATE crate_item SET track_id = ?2 WHERE track_id = ?1 AND NOT EXISTS \
                     (SELECT 1 FROM crate_item other WHERE other.crate_id = crate_item.crate_id \
                      AND other.track_id = ?2)",
                    [track, kept_track],
                )?;
                transaction.execute(
                    "UPDATE track_field SET track_id = ?2 WHERE track_id = ?1 AND NOT EXISTS \
                     (SELECT 1 FROM track_field other WHERE other.track_id = ?2 \
                      AND other.name = track_field.name)",
                    [track, kept_track],
                )?;
            }
            transaction.execute("DELETE FROM track_fts WHERE rowid = ?1", [track])?;
            transaction.execute("DELETE FROM track WHERE id = ?1", [track])?;
            // Its undo history names a track id that a later track may be given.
            crate::edits::forget_track(&transaction, track)?;
        }
        transaction.commit()?;
        Ok(())
    }
}
