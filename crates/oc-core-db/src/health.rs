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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub untidy: Vec<UntidyFolder>,
    pub duplicates: Vec<Duplicate>,
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
        "SELECT f.root_id, f.path_raw, a.name, t.title, COALESCE(f.duration_ms, t.duration_ms) \
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
        ))
    })?;
    let mut health = Health::default();
    let mut folders: BTreeMap<(i64, String), (String, usize)> = BTreeMap::new();
    let mut recordings: BTreeMap<(String, String), Vec<Seen>> = BTreeMap::new();
    for row in rows {
        let (root_id, path, artist, title, length) = row?;
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
