//! A checked copy of `master.db` read into the same [`Collection`] an XML export gives, so the
//! mirror and every view treat the two routes alike. Attribute names and scales follow the XML.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use rbl_db::{DbError, Library, LibraryLocation, OpenMode};
use rusqlite::Connection;

use crate::xml::{Attributes, Collection, Cue, PlaylistNode, Track};

pub fn read_snapshot(master_db: &Path, passphrase: &str) -> Result<Collection, DbError> {
    let library = Library::open(
        LibraryLocation {
            master_db: master_db.to_path_buf(),
            share_root: master_db.parent().unwrap_or(master_db).join("share"),
            passphrase: passphrase.to_owned(),
            is_real_install: false,
        },
        OpenMode::ReadOnly,
    )?;
    let connection = library.connection();
    let mut tracks = read_tracks(connection)?;
    let mut tags = my_tags(connection)?;
    let mut cues = read_cues(connection)?;
    for track in &mut tracks {
        if let Some(names) = tags.remove(&track.id) {
            track.attributes.insert("MyTags".into(), names.join("; "));
        }
        track.cues = cues.remove(&track.id).unwrap_or_default();
    }
    let known: BTreeSet<u64> = tracks.iter().map(|track| track.id).collect();
    let (playlists, referenced) = read_playlists(connection)?;
    let unresolved_track_ids = referenced.difference(&known).copied().collect();
    let mut collection_attributes = Attributes::new();
    collection_attributes.insert("Entries".into(), tracks.len().to_string());
    Ok(Collection {
        attributes: Attributes::new(),
        product: None,
        collection_attributes,
        tracks,
        playlists: vec![PlaylistNode::Folder {
            attributes: named("ROOT", "0"),
            children: playlists,
        }],
        unresolved_track_ids,
        ignored_elements: Vec::new(),
    })
}

fn id(text: &str) -> Result<u64, DbError> {
    text.trim()
        .parse()
        .map_err(|_| DbError::Schema(format!("a rekordbox id is not a number: {text:?}")))
}

/// A stored `FolderPath` in the shape an export's Location decodes to, so both kinds of read
/// match files the same way: a drive path gains the leading slash (`C:/Music` becomes
/// `/C:/Music`), and its backslashes, if any, become slashes. A Mac or UNC path is unchanged.
pub fn decoded_folder_path(folder_path: &str) -> Vec<u8> {
    let bytes = folder_path.as_bytes();
    let drive = matches!(bytes, [letter, b':', ..] if letter.is_ascii_alphabetic());
    if !drive {
        return bytes.to_vec();
    }
    let mut decoded = Vec::with_capacity(bytes.len() + 1);
    decoded.push(b'/');
    decoded.extend(
        bytes
            .iter()
            .map(|byte| if *byte == b'\\' { b'/' } else { *byte }),
    );
    decoded
}

/// The attribute holding a playlist's or folder's id in rekordbox's library.
pub const REKORDBOX_ID: &str = "RekordboxId";

fn named(name: &str, kind: &str) -> Attributes {
    let mut attributes = Attributes::new();
    attributes.insert("Name".into(), name.to_owned());
    attributes.insert("Type".into(), kind.to_owned());
    attributes
}

fn read_tracks(connection: &Connection) -> Result<Vec<Track>, DbError> {
    let mut statement = connection.prepare(
        "SELECT c.ID, COALESCE(c.FolderPath, ''), COALESCE(c.Title, ''), ar.Name, al.Name, g.Name,
                l.Name, k.ScaleName, co.Commnt, c.BPM, c.Length, c.Rating, c.ReleaseYear, c.Commnt
         FROM djmdContent c
         LEFT JOIN djmdArtist ar ON ar.ID = c.ArtistID AND ar.rb_local_deleted = 0
         LEFT JOIN djmdAlbum al ON al.ID = c.AlbumID AND al.rb_local_deleted = 0
         LEFT JOIN djmdGenre g ON g.ID = c.GenreID AND g.rb_local_deleted = 0
         LEFT JOIN djmdLabel l ON l.ID = c.LabelID AND l.rb_local_deleted = 0
         LEFT JOIN djmdKey k ON k.ID = c.KeyID AND k.rb_local_deleted = 0
         LEFT JOIN djmdColor co ON co.ID = c.ColorID AND co.rb_local_deleted = 0
         WHERE c.rb_local_deleted = 0
         ORDER BY CAST(c.ID AS INTEGER)",
    )?;
    let rows = statement.query_map([], |row| {
        let mut attributes = Attributes::new();
        attributes.insert("Name".into(), row.get(2)?);
        for (column, key) in [
            (3, "Artist"),
            (4, "Album"),
            (5, "Genre"),
            (6, "Label"),
            (7, "Tonality"),
            (8, "Colour"),
            (13, "Comments"),
        ] {
            if let Some(text) = row.get::<_, Option<String>>(column)? {
                if !text.is_empty() {
                    attributes.insert(key.into(), text);
                }
            }
        }
        let bpm_x100 = row.get::<_, Option<i64>>(9)?.unwrap_or(0);
        attributes.insert(
            "AverageBpm".into(),
            format!("{}.{:02}", bpm_x100 / 100, bpm_x100 % 100),
        );
        let length = row.get::<_, Option<i64>>(10)?.unwrap_or(0);
        attributes.insert("TotalTime".into(), length.to_string());
        let stars = row.get::<_, Option<i64>>(11)?.unwrap_or(0).clamp(0, 5);
        attributes.insert("Rating".into(), (stars * 51).to_string());
        if let Some(year) = row.get::<_, Option<i64>>(12)?.filter(|year| *year > 0) {
            attributes.insert("Year".into(), year.to_string());
        }
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            attributes,
        ))
    })?;
    let mut tracks = Vec::new();
    for row in rows {
        let (key, location, attributes) = row?;
        tracks.push(Track {
            id: id(&key)?,
            location_bytes: decoded_folder_path(&location),
            location,
            attributes,
            cues: Vec::new(),
            tempos: Vec::new(),
        });
    }
    Ok(tracks)
}

fn my_tags(connection: &Connection) -> Result<HashMap<u64, Vec<String>>, DbError> {
    let mut statement = connection.prepare(
        "SELECT s.ContentID, COALESCE(t.Name, '') FROM djmdSongMyTag s
         JOIN djmdMyTag t ON t.ID = s.MyTagID AND t.rb_local_deleted = 0
         WHERE s.rb_local_deleted = 0
         ORDER BY s.ContentID, s.TrackNo, s.MyTagID",
    )?;
    let mut tags: HashMap<u64, Vec<String>> = HashMap::new();
    for row in statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))? {
        let (content, name) = row?;
        tags.entry(id(&content)?).or_default().push(name);
    }
    Ok(tags)
}

/// rekordbox's `Kind`: 0 a memory cue, 4 an active memory loop, 1-3 and 5-17 hot cues A to P.
/// The XML numbers hot cues from 0 and gives memory cues -1.
fn cue_number(kind: i64) -> Option<i32> {
    let number = match kind {
        0 | 4 => -1,
        1..=3 => kind - 1,
        5..=17 => kind - 2,
        _ => return None,
    };
    i32::try_from(number).ok()
}

fn read_cues(connection: &Connection) -> Result<HashMap<u64, Vec<Cue>>, DbError> {
    let mut statement = connection.prepare(
        "SELECT ContentID, COALESCE(InMsec, 0), COALESCE(OutMsec, -1), COALESCE(Kind, 0),
                COALESCE(Comment, '')
         FROM djmdCue WHERE rb_local_deleted = 0
         ORDER BY ContentID, InMsec, ID",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut cues: HashMap<u64, Vec<Cue>> = HashMap::new();
    for row in rows {
        let (content, in_ms, out_ms, kind, comment) = row?;
        let Some(number) = cue_number(kind) else {
            continue;
        };
        let mut attributes = Attributes::new();
        if !comment.is_empty() {
            attributes.insert("Name".into(), comment);
        }
        #[allow(clippy::cast_precision_loss)] // cue times are far below 2^52 ms
        cues.entry(id(&content)?).or_default().push(Cue {
            kind: if out_ms > 0 { 4 } else { 0 },
            number: Some(number),
            start_seconds: in_ms as f64 / 1000.0,
            end_seconds: (out_ms > 0).then(|| out_ms as f64 / 1000.0),
            attributes,
        });
    }
    Ok(cues)
}

struct Node {
    id: String,
    name: String,
    folder: bool,
    smart: bool,
}

/// The tree under rekordbox's `root`, each level in `Seq` order, and every track id a playlist
/// names. A node whose parent is gone is left out, as rekordbox does.
fn read_playlists(connection: &Connection) -> Result<(Vec<PlaylistNode>, BTreeSet<u64>), DbError> {
    let mut statement = connection.prepare(
        "SELECT ID, COALESCE(Name, ''), COALESCE(Attribute, 0), COALESCE(ParentID, 'root')
         FROM djmdPlaylist WHERE rb_local_deleted = 0
         ORDER BY COALESCE(Seq, 0), CAST(ID AS INTEGER)",
    )?;
    let mut children: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })? {
        let (id, name, attribute, parent) = row?;
        children.entry(parent).or_default().push(Node {
            id,
            name,
            folder: attribute == 1,
            smart: attribute == 4,
        });
    }
    let mut items: HashMap<String, Vec<u64>> = HashMap::new();
    let mut referenced = BTreeSet::new();
    let mut statement = connection.prepare(
        "SELECT PlaylistID, ContentID FROM djmdSongPlaylist WHERE rb_local_deleted = 0
         ORDER BY PlaylistID, TrackNo, ID",
    )?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (playlist, content) = row?;
        let track = id(&content)?;
        referenced.insert(track);
        items.entry(playlist).or_default().push(track);
    }
    let tree = build(&mut children, &mut items, "root", 0);
    Ok((tree, referenced))
}

fn build(
    children: &mut BTreeMap<String, Vec<Node>>,
    items: &mut HashMap<String, Vec<u64>>,
    parent: &str,
    depth: usize,
) -> Vec<PlaylistNode> {
    // A cycle in a damaged library must not recurse forever.
    if depth > 64 {
        return Vec::new();
    }
    let Some(nodes) = children.remove(parent) else {
        return Vec::new();
    };
    nodes
        .into_iter()
        .map(|node| {
            if node.folder {
                let mut attributes = named(&node.name, "0");
                // So a change made in openCrate reaches exactly this folder or playlist.
                attributes.insert(REKORDBOX_ID.into(), node.id.clone());
                PlaylistNode::Folder {
                    attributes,
                    children: build(children, items, &node.id, depth + 1),
                }
            } else {
                let mut attributes = named(&node.name, "1");
                attributes.insert(REKORDBOX_ID.into(), node.id.clone());
                if node.smart {
                    attributes.insert("Smart".into(), "1".into());
                }
                PlaylistNode::Playlist {
                    attributes,
                    track_ids: items.remove(&node.id).unwrap_or_default(),
                }
            }
        })
        .collect()
}
