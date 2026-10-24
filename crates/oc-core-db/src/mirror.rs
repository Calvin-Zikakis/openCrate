//! The rekordbox mirror: one export, replaced wholesale and atomically.
//!
//! rekordbox owns playlists, cues and grids; this is a read-only copy of what one export
//! said, matched to the indexed files through [`Matcher`]. A successful read replaces the
//! source's rows in one transaction and advances its generation. A failed read, or one that
//! breaks halfway, changes nothing. Reading an unchanged export again changes nothing.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::BufReader;
use std::path::Path;
use std::time::UNIX_EPOCH;

use oc_paths::{os_str_to_bytes, LocationPath, MatchStatus, RelPath};
use oc_rekordbox::xml::{self, Attributes, Collection, PlaylistNode};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::matching::{LocationMatch, Matcher};
use crate::roots::{list_roots, root_aliases};
use crate::{Database, Error, Result};

pub struct ExportRead {
    pub collection: Collection,
    pub file_mtime_ms: Option<i64>,
}

/// Parse an export file. Call this away from the writer thread, then hand the result to
/// [`Database::replace_mirror`].
pub fn read_export(path: &Path) -> Result<ExportRead> {
    let file_mtime_ms = fs::metadata(path)?
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_millis()).ok());
    let collection = xml::read(BufReader::new(File::open(path)?))?;
    Ok(ExportRead {
        collection,
        file_mtime_ms,
    })
}

/// Where a mirror came from: an XML export file, or a copy of rekordbox's own `master.db`
/// (identified by the live library's path, not the copy's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Xml,
    MasterDb,
}

impl SourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Xml => "xml",
            Self::MasterDb => "master_db",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "xml" => Some(Self::Xml),
            "master_db" => Some(Self::MasterDb),
            _ => None,
        }
    }
}

pub struct MirrorSource<'a> {
    pub kind: SourceKind,
    pub path: &'a Path,
    pub file_mtime_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MirrorOutcome {
    pub source_id: i64,
    pub generation: i64,
    /// Nothing relevant differed from the stored mirror, so nothing was rewritten.
    pub unchanged: bool,
    pub entries: usize,
    pub matched: usize,
    pub unmapped: usize,
    pub not_indexed: usize,
    pub ambiguous: usize,
    pub unrecognized: usize,
    pub playlists: usize,
    pub playlist_items: usize,
    /// Tracks that now carry a tempo and key from this source.
    pub derived_set: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorStatus {
    pub source_id: i64,
    pub kind: SourceKind,
    pub path_raw: Vec<u8>,
    pub generation: i64,
    pub entries: i64,
    pub file_mtime_ms: Option<i64>,
    pub last_read_ms: Option<i64>,
    pub last_attempt_ms: Option<i64>,
    pub last_error: Option<String>,
}

pub fn mirror_sources(connection: &Connection) -> Result<Vec<MirrorStatus>> {
    let mut statement = connection.prepare(
        "SELECT id, path_raw, generation, entries, file_mtime_ms, last_read_ms, last_attempt_ms, \
         last_error, kind FROM rb_source ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        let kind: String = row.get(8)?;
        Ok(MirrorStatus {
            source_id: row.get(0)?,
            kind: SourceKind::parse(&kind).ok_or(rusqlite::Error::InvalidColumnType(
                8,
                "kind".into(),
                rusqlite::types::Type::Text,
            ))?,
            path_raw: row.get(1)?,
            generation: row.get(2)?,
            entries: row.get(3)?,
            file_mtime_ms: row.get(4)?,
            last_read_ms: row.get(5)?,
            last_attempt_ms: row.get(6)?,
            last_error: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn json(attributes: &Attributes) -> Result<String> {
    serde_json::to_string(attributes).map_err(|_| Error::InvalidStoredValue("attributes"))
}

fn name_of(attributes: &Attributes) -> String {
    attributes.get("Name").cloned().unwrap_or_default()
}

/// A tempo of zero is how rekordbox writes "not analyzed".
fn tempo_of(attributes: &Attributes) -> Option<f64> {
    attributes
        .get("AverageBpm")
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
}

fn hash_attributes(hasher: &mut DefaultHasher, attributes: &Attributes) {
    attributes.hash(hasher);
}

fn hash_nodes(hasher: &mut DefaultHasher, nodes: &[PlaylistNode]) {
    nodes.len().hash(hasher);
    for node in nodes {
        match node {
            PlaylistNode::Folder {
                attributes,
                children,
            } => {
                0u8.hash(hasher);
                hash_attributes(hasher, attributes);
                hash_nodes(hasher, children);
            }
            PlaylistNode::Playlist {
                attributes,
                track_ids,
            } => {
                1u8.hash(hasher);
                hash_attributes(hasher, attributes);
                track_ids.hash(hasher);
            }
        }
    }
}

/// Covers everything the mirror stores, including how each entry matched, so a re-read
/// after the library index changed is not mistaken for an unchanged export.
fn state_digest(collection: &Collection, matches: &[LocationMatch]) -> i64 {
    let mut hasher = DefaultHasher::new();
    hash_attributes(&mut hasher, &collection.attributes);
    collection.product.hash(&mut hasher);
    hash_attributes(&mut hasher, &collection.collection_attributes);
    for (track, found) in collection.tracks.iter().zip(matches) {
        track.id.hash(&mut hasher);
        track.location_bytes.hash(&mut hasher);
        hash_attributes(&mut hasher, &track.attributes);
        for cue in &track.cues {
            (cue.kind, cue.number, cue.start_seconds.to_bits()).hash(&mut hasher);
            cue.end_seconds.map(f64::to_bits).hash(&mut hasher);
            hash_attributes(&mut hasher, &cue.attributes);
        }
        for tempo in &track.tempos {
            (tempo.start_seconds.to_bits(), tempo.bpm.to_bits()).hash(&mut hasher);
            (&tempo.meter, tempo.beat).hash(&mut hasher);
            hash_attributes(&mut hasher, &tempo.attributes);
        }
        (found.status, found.kind, found.root_id, found.file_id).hash(&mut hasher);
    }
    hash_nodes(&mut hasher, &collection.playlists);
    i64::from_ne_bytes(hasher.finish().to_ne_bytes())
}

/// The ids a source's playlists and folders had before it is read again. Each keeps its id, found
/// by rekordbox's own id when the read gives one and else by its path of names, so the playlist
/// open in the app and the folders open in the sidebar are still the same ones after a read that
/// added a playlist above them. New ones take ids past every id in use.
struct PlaylistIds {
    kept: HashMap<String, i64>,
    next: i64,
}

fn playlist_key(path: &str, attributes: &Attributes) -> String {
    match attributes.get(oc_rekordbox::db::REKORDBOX_ID) {
        Some(id) => format!("id:{id}"),
        None => format!("path:{path}"),
    }
}

/// `parent`'s path with `name` added; a unit separator cannot appear in a name rekordbox shows.
fn child_path(parent: &str, name: &str) -> String {
    format!("{parent}\u{1f}{name}")
}

impl PlaylistIds {
    /// Read before the source's rows are cleared.
    fn before_clearing(connection: &Connection, source_id: i64) -> Result<Self> {
        let mut statement = connection.prepare(
            "SELECT id, parent_id, name, attributes FROM playlist WHERE rb_source_id = ?1",
        )?;
        let rows = statement
            .query_map([source_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let parents: HashMap<i64, (Option<i64>, &str)> = rows
            .iter()
            .map(|(id, parent, name, _)| (*id, (*parent, name.as_str())))
            .collect();
        let path = |mut id: i64| {
            let mut names = Vec::new();
            // Bounded, in case a stored tree ever loops.
            for _ in 0..=parents.len() {
                let Some((parent, name)) = parents.get(&id) else {
                    break;
                };
                names.push(*name);
                match parent {
                    Some(parent) => id = *parent,
                    None => break,
                }
            }
            names
                .iter()
                .rev()
                .fold(String::new(), |path, name| child_path(&path, name))
        };
        let mut kept = HashMap::new();
        for (id, _, _, attributes) in &rows {
            let attributes: Attributes = serde_json::from_str(attributes).unwrap_or_default();
            kept.entry(playlist_key(&path(*id), &attributes))
                .or_insert(*id);
        }
        let next =
            connection.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM playlist", [], |row| {
                row.get(0)
            })?;
        Ok(Self { kept, next })
    }

    fn id_for(&mut self, key: &str) -> i64 {
        self.kept.remove(key).unwrap_or_else(|| {
            self.next += 1;
            self.next - 1
        })
    }
}

fn insert_nodes(
    connection: &Connection,
    source_id: i64,
    parent: Option<(i64, &str)>,
    nodes: &[PlaylistNode],
    ids: &mut PlaylistIds,
    outcome: &mut MirrorOutcome,
) -> Result<()> {
    for (position, node) in nodes.iter().enumerate() {
        let (kind, attributes) = match node {
            PlaylistNode::Folder { attributes, .. } => ("folder", attributes),
            PlaylistNode::Playlist { attributes, .. } => ("playlist", attributes),
        };
        let name = name_of(attributes);
        let path = child_path(parent.map_or("", |(_, path)| path), &name);
        let id = ids.id_for(&playlist_key(&path, attributes));
        connection.execute(
            "INSERT INTO playlist (id, rb_source_id, parent_id, kind, name, position, attributes) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                source_id,
                parent.map(|(id, _)| id),
                kind,
                name,
                i64::try_from(position).map_err(|_| Error::InvalidStoredValue("position"))?,
                json(attributes)?
            ],
        )?;
        outcome.playlists += 1;
        match node {
            PlaylistNode::Folder { children, .. } => {
                insert_nodes(
                    connection,
                    source_id,
                    Some((id, &path)),
                    children,
                    ids,
                    outcome,
                )?;
            }
            PlaylistNode::Playlist { track_ids, .. } => {
                let mut item = connection.prepare_cached(
                    "INSERT INTO playlist_item (playlist_id, position, rb_key) VALUES (?1, ?2, ?3)",
                )?;
                for (slot, key) in track_ids.iter().enumerate() {
                    item.execute(params![
                        id,
                        i64::try_from(slot).map_err(|_| Error::InvalidStoredValue("position"))?,
                        i64::try_from(*key).map_err(|_| Error::InvalidStoredValue("TrackID"))?
                    ])?;
                    outcome.playlist_items += 1;
                }
            }
        }
    }
    Ok(())
}

/// Everything a source owns, removed before it is rewritten. Tempo and key that this source
/// gave to tracks are cleared here and set again for whatever still matches.
const CLEAR_SOURCE: [&str; 6] = [
    "DELETE FROM rb_cue WHERE rb_track_id IN (SELECT id FROM rb_track WHERE rb_source_id = ?1)",
    "DELETE FROM rb_tempo WHERE rb_track_id IN (SELECT id FROM rb_track WHERE rb_source_id = ?1)",
    "DELETE FROM playlist_item WHERE playlist_id IN (SELECT id FROM playlist WHERE rb_source_id = ?1)",
    "DELETE FROM playlist WHERE rb_source_id = ?1",
    "UPDATE track SET bpm = NULL, musical_key = '', derived_source_id = NULL, \
     derived_generation = NULL WHERE derived_source_id = ?1",
    "DELETE FROM rb_track WHERE rb_source_id = ?1",
];

fn position(index: usize) -> Result<i64> {
    i64::try_from(index).map_err(|_| Error::InvalidStoredValue("position"))
}

/// Entries with their cues and tempo points, in `TrackID` order, and the tempo and key that
/// matched tracks take from them (the first entry per file wins).
fn insert_entries(
    connection: &Connection,
    source_id: i64,
    generation: i64,
    collection: &Collection,
    results: &[LocationMatch],
    outcome: &mut MirrorOutcome,
) -> Result<()> {
    let mut insert_track = connection.prepare(
        "INSERT INTO rb_track (rb_source_id, rb_key, location_raw, name, artist, bpm, \
         musical_key, rating, attributes, match_status, match_kind, root_id, file_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )?;
    let mut insert_cue = connection.prepare(
        "INSERT INTO rb_cue (rb_track_id, position, kind, number, start_seconds, \
         end_seconds, name, attributes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    let mut insert_tempo = connection.prepare(
        "INSERT INTO rb_tempo (rb_track_id, position, start_seconds, bpm, meter, beat, \
         attributes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut project = connection.prepare(
        "UPDATE track SET bpm = ?1, musical_key = ?2, derived_source_id = ?3, \
         derived_generation = ?4 WHERE id = (SELECT track_id FROM file WHERE id = ?5)",
    )?;
    let mut projected: HashSet<i64> = HashSet::new();
    let mut order: Vec<usize> = (0..collection.tracks.len()).collect();
    order.sort_by_key(|index| collection.tracks[*index].id);
    for index in order {
        let (track, found) = (&collection.tracks[index], &results[index]);
        let bpm = tempo_of(&track.attributes);
        let key = track
            .attributes
            .get("Tonality")
            .cloned()
            .unwrap_or_default();
        insert_track.execute(params![
            source_id,
            i64::try_from(track.id).map_err(|_| Error::InvalidStoredValue("TrackID"))?,
            track.location_bytes,
            name_of(&track.attributes),
            track.attributes.get("Artist").cloned().unwrap_or_default(),
            bpm,
            key,
            track
                .attributes
                .get("Rating")
                .and_then(|text| text.trim().parse::<i64>().ok()),
            json(&track.attributes)?,
            found.status.as_str(),
            found.kind.map(oc_paths::MatchKind::as_str),
            found.root_id,
            found.file_id,
        ])?;
        let track_row = connection.last_insert_rowid();
        for (slot, cue) in track.cues.iter().enumerate() {
            insert_cue.execute(params![
                track_row,
                position(slot)?,
                cue.kind,
                cue.number,
                cue.start_seconds,
                cue.end_seconds,
                name_of(&cue.attributes),
                json(&cue.attributes)?,
            ])?;
        }
        for (slot, tempo) in track.tempos.iter().enumerate() {
            insert_tempo.execute(params![
                track_row,
                position(slot)?,
                tempo.start_seconds,
                tempo.bpm,
                tempo.meter,
                tempo.beat,
                json(&tempo.attributes)?,
            ])?;
        }
        if let Some(file_id) = found.file_id {
            if projected.insert(file_id) {
                outcome.derived_set +=
                    project.execute(params![bpm, key, source_id, generation, file_id])?;
            }
        }
    }
    Ok(())
}

fn export_attributes(collection: &Collection) -> String {
    serde_json::json!({
        "root": collection.attributes,
        "product": collection.product,
        "collection": collection.collection_attributes,
        "ignored_elements": collection.ignored_elements,
    })
    .to_string()
}

impl Database {
    /// Replace the mirror of one export. Atomic: on any error the previous mirror stays.
    pub fn replace_mirror(
        &mut self,
        source: &MirrorSource<'_>,
        collection: &Collection,
        now_ms: i64,
    ) -> Result<MirrorOutcome> {
        let path_raw = os_str_to_bytes(source.path.as_os_str());
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT OR IGNORE INTO rb_source (kind, path_raw) VALUES (?1, ?2)",
            params![source.kind.as_str(), &path_raw],
        )?;
        let (source_id, generation, old_digest): (i64, i64, Option<i64>) = transaction.query_row(
            "SELECT id, generation, state_digest FROM rb_source WHERE kind = ?1 AND path_raw = ?2",
            params![source.kind.as_str(), &path_raw],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let mut outcome = MirrorOutcome {
            source_id,
            generation,
            entries: collection.tracks.len(),
            ..MirrorOutcome::default()
        };
        let finder = Matcher::load(&transaction)?;
        let mut results = Vec::with_capacity(collection.tracks.len());
        for track in &collection.tracks {
            let found = finder.match_location(&transaction, &track.location_bytes)?;
            match found.status {
                MatchStatus::Matched => outcome.matched += 1,
                MatchStatus::Unmapped => outcome.unmapped += 1,
                MatchStatus::NotIndexed => outcome.not_indexed += 1,
                MatchStatus::Ambiguous => outcome.ambiguous += 1,
                MatchStatus::Unrecognized => outcome.unrecognized += 1,
            }
            results.push(found);
        }
        let digest = state_digest(collection, &results);
        if generation > 0 && old_digest == Some(digest) {
            transaction.execute(
                "UPDATE rb_source SET file_mtime_ms = ?2, last_read_ms = ?3, last_attempt_ms = ?3, \
                 last_error = NULL WHERE id = ?1",
                params![source_id, source.file_mtime_ms, now_ms],
            )?;
            transaction.commit()?;
            outcome.unchanged = true;
            return Ok(outcome);
        }

        let mut playlist_ids = PlaylistIds::before_clearing(&transaction, source_id)?;
        for statement in CLEAR_SOURCE {
            transaction.execute(statement, [source_id])?;
        }
        let new_generation = generation + 1;
        insert_entries(
            &transaction,
            source_id,
            new_generation,
            collection,
            &results,
            &mut outcome,
        )?;
        insert_nodes(
            &transaction,
            source_id,
            None,
            &collection.playlists,
            &mut playlist_ids,
            &mut outcome,
        )?;
        transaction.execute(
            "UPDATE rb_source SET generation = ?2, entries = ?3, file_mtime_ms = ?4, \
             last_read_ms = ?5, last_attempt_ms = ?5, last_error = NULL, state_digest = ?6, \
             export_attributes = ?7 WHERE id = ?1",
            params![
                source_id,
                new_generation,
                position(collection.tracks.len())?,
                source.file_mtime_ms,
                now_ms,
                digest,
                export_attributes(collection),
            ],
        )?;
        transaction.commit()?;
        outcome.generation = new_generation;
        Ok(outcome)
    }

    /// Remove every mirror of the given kinds except `keep`, with the tempo and key they gave to
    /// tracks. Returns how many sources went. The app shows one rekordbox source at a time.
    pub fn drop_sources(&mut self, keep: i64, kinds: &[SourceKind]) -> Result<usize> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut doomed = Vec::new();
        {
            let mut statement =
                transaction.prepare("SELECT id, kind FROM rb_source WHERE id <> ?1")?;
            let rows = statement.query_map([keep], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, kind) = row?;
                if SourceKind::parse(&kind).is_some_and(|kind| kinds.contains(&kind)) {
                    doomed.push(id);
                }
            }
        }
        for id in &doomed {
            for statement in CLEAR_SOURCE {
                transaction.execute(statement, [id])?;
            }
            transaction.execute("DELETE FROM rb_source WHERE id = ?1", [id])?;
        }
        transaction.commit()?;
        Ok(doomed.len())
    }

    /// Remember that a read failed. The previous mirror and its generation are untouched.
    pub fn record_mirror_failure(&mut self, path: &Path, error: &str, now_ms: i64) -> Result<()> {
        let path_raw = os_str_to_bytes(path.as_os_str());
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT OR IGNORE INTO rb_source (kind, path_raw) VALUES ('xml', ?1)",
            [&path_raw],
        )?;
        transaction.execute(
            "UPDATE rb_source SET last_error = ?2, last_attempt_ms = ?3 \
             WHERE kind = 'xml' AND path_raw = ?1",
            params![path_raw, error, now_ms],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

/// How the entries of one mirrored export matched the library, for a Sync screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MirrorCounts {
    pub matched: i64,
    pub unmapped: i64,
    pub not_indexed: i64,
    pub ambiguous: i64,
    pub unrecognized: i64,
    /// Playlists, not the folders that hold them.
    pub playlists: i64,
    /// Playlist entries naming a track the export does not contain.
    pub unresolved_references: i64,
}

pub fn mirror_counts(connection: &Connection, source_id: i64) -> Result<MirrorCounts> {
    let mut counts = MirrorCounts::default();
    let mut statement = connection.prepare(
        "SELECT match_status, COUNT(*) FROM rb_track WHERE rb_source_id = ?1 GROUP BY match_status",
    )?;
    let rows = statement.query_map([source_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (status, total) = row?;
        match status.as_str() {
            "matched" => counts.matched = total,
            "unmapped" => counts.unmapped = total,
            "not_indexed" => counts.not_indexed = total,
            "ambiguous" => counts.ambiguous = total,
            _ => counts.unrecognized += total,
        }
    }
    counts.playlists = connection.query_row(
        "SELECT COUNT(*) FROM playlist WHERE rb_source_id = ?1 AND kind = 'playlist'",
        [source_id],
        |row| row.get(0),
    )?;
    counts.unresolved_references = connection.query_row(
        "SELECT COUNT(*) FROM playlist_item i JOIN playlist p ON p.id = i.playlist_id \
         WHERE p.rb_source_id = ?1 AND NOT EXISTS \
           (SELECT 1 FROM rb_track t WHERE t.rb_source_id = ?1 AND t.rb_key = i.rb_key)",
        [source_id],
        |row| row.get(0),
    )?;
    Ok(counts)
}

/// "Entries in this export would match files in that folder if their location began with
/// this prefix." Found by trying every prefix depth of each unmapped entry against the
/// files actually indexed, so the depth is evidence, not a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasSuggestion {
    pub root_id: i64,
    pub root_label: String,
    /// The decoded Location form, such as `/G:/Library`.
    pub prefix: String,
    pub entries: usize,
}

/// Unmapped entries examined per call; enough to be sure and bounded for huge exports.
const SUGGESTION_SAMPLE: i64 = 5_000;

pub fn alias_suggestions(
    connection: &Connection,
    source_id: i64,
    limit: usize,
) -> Result<Vec<AliasSuggestion>> {
    let roots = root_aliases(connection)?;
    let labels: Vec<(i64, String)> = list_roots(connection)?
        .into_iter()
        .map(|root| (root.id, root.label))
        .collect();
    let mut find = connection.prepare("SELECT 1 FROM file WHERE root_id = ?1 AND path_key = ?2")?;
    let mut statement = connection.prepare(
        "SELECT location_raw FROM rb_track WHERE rb_source_id = ?1 AND match_status = 'unmapped' \
         LIMIT ?2",
    )?;
    let locations = statement
        .query_map(params![source_id, SUGGESTION_SAMPLE], |row| {
            row.get::<_, Vec<u8>>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut tally: std::collections::HashMap<(i64, Vec<u8>), usize> =
        std::collections::HashMap::new();
    for raw in locations {
        let Ok(location) = LocationPath::parse(&raw) else {
            continue;
        };
        let components = location.components();
        for depth in 0..components.len() {
            let Ok(remainder) =
                RelPath::from_components(&components[depth..], location.shape().flavor())
            else {
                continue;
            };
            let Ok(prefix) =
                LocationPath::from_parts(location.shape(), components[..depth].to_vec())
            else {
                continue;
            };
            for root in &roots {
                let key = root.profile.key(&remainder);
                let hit = find
                    .query_row(params![root.root_id, key.as_bytes()], |_| Ok(()))
                    .optional()?
                    .is_some();
                if hit {
                    *tally
                        .entry((root.root_id, prefix.to_decoded()))
                        .or_default() += 1;
                }
            }
        }
    }
    let mut suggestions: Vec<AliasSuggestion> = tally
        .into_iter()
        .filter_map(|((root_id, decoded), entries)| {
            let prefix = String::from_utf8(decoded).ok()?;
            let root_label = labels
                .iter()
                .find(|(id, _)| *id == root_id)
                .map(|(_, label)| label.clone())?;
            Some(AliasSuggestion {
                root_id,
                root_label,
                prefix,
                entries,
            })
        })
        .collect();
    suggestions.sort_by(|left, right| {
        right
            .entries
            .cmp(&left.entries)
            .then_with(|| left.prefix.cmp(&right.prefix))
    });
    suggestions.truncate(limit);
    Ok(suggestions)
}

/// What is known about one mirrored export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorInfo {
    pub status: MirrorStatus,
    pub counts: MirrorCounts,
}

pub fn mirrors(connection: &Connection) -> Result<Vec<MirrorInfo>> {
    mirror_sources(connection)?
        .into_iter()
        .map(|status| {
            let counts = mirror_counts(connection, status.source_id)?;
            Ok(MirrorInfo { status, counts })
        })
        .collect()
}

/// A decoded Location prefix typed or chosen by the owner, checked before it is stored.
pub fn parse_alias(prefix: &str) -> Result<LocationPath> {
    LocationPath::parse(prefix.trim().as_bytes()).map_err(|_| Error::InvalidAlias)
}

/// The path an export was last read from, as an OS path.
pub fn source_path(connection: &Connection, source_id: i64) -> Result<std::path::PathBuf> {
    let (raw, kind): (Vec<u8>, String) = connection
        .query_row(
            "SELECT path_raw, kind FROM rb_source WHERE id = ?1",
            [source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(Error::UnknownSource(source_id))?;
    if kind != SourceKind::Xml.as_str() {
        return Err(Error::NotAnExport(source_id));
    }
    let os =
        oc_paths::bytes_to_os_string(&raw).map_err(|_| Error::InvalidStoredValue("source path"))?;
    Ok(std::path::PathBuf::from(os))
}

/// One cue or loop as rekordbox wrote it. Times are fractional seconds, never rounded.
#[derive(Clone, Debug, PartialEq)]
pub struct CueRow {
    /// rekordbox's `Type`: 0 cue, 1 fade-in, 2 fade-out, 3 load, 4 loop.
    pub kind: i64,
    /// `Num`: -1 is a memory cue, 0 and up is a hot cue slot.
    pub number: Option<i64>,
    pub start_seconds: f64,
    pub end_seconds: Option<f64>,
    pub name: String,
}

/// One beat-grid point: the tempo from `start_seconds` on.
#[derive(Clone, Debug, PartialEq)]
pub struct TempoRow {
    pub start_seconds: f64,
    pub bpm: f64,
    pub meter: Option<String>,
    pub beat: Option<i64>,
}

/// What a rekordbox export says about one library track.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackAnalysis {
    pub source_id: i64,
    pub generation: i64,
    pub bpm: Option<f64>,
    pub musical_key: String,
    pub cues: Vec<CueRow>,
    pub tempos: Vec<TempoRow>,
}

/// The cues and beat grid rekordbox has for a library track, from the newest export whose
/// entry matched one of the track's files. `None` when no export matched it. Read only; the
/// numbers are shown as rekordbox wrote them and never edited here.
pub fn read_track_analysis(
    connection: &Connection,
    track_id: i64,
) -> Result<Option<TrackAnalysis>> {
    let found = connection
        .query_row(
            "SELECT rb.id, rb.rb_source_id, source.generation, rb.bpm, rb.musical_key \
             FROM rb_track rb JOIN file f ON f.id = rb.file_id \
             JOIN rb_source source ON source.id = rb.rb_source_id \
             WHERE f.track_id = ?1 AND rb.match_status = 'matched' \
             ORDER BY source.last_read_ms DESC, rb.rb_source_id DESC, f.id LIMIT 1",
            [track_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((rb_track, source_id, generation, bpm, musical_key)) = found else {
        return Ok(None);
    };
    let mut cues = connection.prepare(
        "SELECT kind, number, start_seconds, end_seconds, name FROM rb_cue \
         WHERE rb_track_id = ?1 ORDER BY start_seconds, position",
    )?;
    let cues = cues
        .query_map([rb_track], |row| {
            Ok(CueRow {
                kind: row.get(0)?,
                number: row.get(1)?,
                start_seconds: row.get(2)?,
                end_seconds: row.get(3)?,
                name: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut tempos = connection.prepare(
        "SELECT start_seconds, bpm, meter, beat FROM rb_tempo \
         WHERE rb_track_id = ?1 ORDER BY position",
    )?;
    let tempos = tempos
        .query_map([rb_track], |row| {
            Ok(TempoRow {
                start_seconds: row.get(0)?,
                bpm: row.get(1)?,
                meter: row.get(2)?,
                beat: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Some(TrackAnalysis {
        source_id,
        generation,
        bpm,
        musical_key,
        cues,
        tempos,
    }))
}
