//! A small rekordbox XML document offering new tracks: a collection and one playlist. It holds
//! only what openCrate knows for sure; rekordbox analyses the files itself.

use std::fmt::Write as _;

use crate::location::encode_location;
use crate::xml::{Attributes, Track};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTrack {
    /// The absolute path, as bytes.
    pub path: Vec<u8>,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    pub total_seconds: u32,
    pub size: u64,
    /// The container, such as `AIFF` or `MP3`; written as rekordbox's `Kind`, "AIFF File".
    pub kind: String,
}

/// Text for an attribute value. Characters XML 1.0 cannot hold are dropped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if c < ' ' || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

/// One track in a document: one openCrate offers, or an entry rekordbox exported, written back
/// exactly (every attribute, beat grid and cue) so importing it can change nothing.
pub enum Entry {
    New(NewTrack),
    Echo(Track),
}

/// A playlist of `entries` (indexes into the document's entries), at the top or in a folder.
pub struct OfferPlaylist {
    pub folder: Option<String>,
    pub name: String,
    pub entries: Vec<usize>,
}

fn attributes(out: &mut String, element: &str, values: &Attributes, indent: &str) {
    let _ = write!(out, "{indent}<{element}");
    for (key, value) in values {
        let _ = write!(out, " {key}=\"{}\"", escape(value));
    }
}

fn write_entry(out: &mut String, id: usize, entry: &Entry) {
    match entry {
        Entry::New(track) => {
            let _ = write!(
                out,
                "    <TRACK TrackID=\"{id}\" Name=\"{}\" Artist=\"{}\" Album=\"{}\" Kind=\"{} File\" Size=\"{}\" TotalTime=\"{}\"",
                escape(&track.name),
                escape(&track.artist),
                escape(&track.album),
                escape(&track.kind),
                track.size,
                track.total_seconds,
            );
            if let Some(year) = track.year {
                let _ = write!(out, " Year=\"{year}\"");
            }
            let _ = writeln!(
                out,
                " Location=\"{}\"/>",
                escape(&encode_location(&track.path))
            );
        }
        Entry::Echo(track) => {
            let mut values = track.attributes.clone();
            values.insert("TrackID".into(), id.to_string());
            // A database read has no Location attribute; spell it from the path.
            values
                .entry("Location".into())
                .or_insert_with(|| encode_location(&track.location_bytes));
            attributes(out, "TRACK", &values, "    ");
            if track.tempos.is_empty() && track.cues.is_empty() {
                out.push_str("/>\n");
                return;
            }
            out.push_str(">\n");
            for tempo in &track.tempos {
                attributes(out, "TEMPO", &tempo.attributes, "      ");
                out.push_str("/>\n");
            }
            for cue in &track.cues {
                attributes(out, "POSITION_MARK", &cue.attributes, "      ");
                out.push_str("/>\n");
            }
            out.push_str("    </TRACK>\n");
        }
    }
}

fn write_playlist(out: &mut String, playlist: &OfferPlaylist, indent: &str) {
    let _ = writeln!(
        out,
        "{indent}<NODE Name=\"{}\" Type=\"1\" KeyType=\"0\" Entries=\"{}\">",
        escape(&playlist.name),
        playlist.entries.len()
    );
    for index in &playlist.entries {
        let _ = writeln!(out, "{indent}  <TRACK Key=\"{}\"/>", index + 1);
    }
    let _ = writeln!(out, "{indent}</NODE>");
}

/// A whole document: `entries` numbered 1 up, and `playlists` under ROOT, those with a folder
/// grouped in it, in the order first named.
pub fn write_document(
    entries: &[Entry],
    playlists: &[OfferPlaylist],
    product_version: &str,
) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\">\n");
    let _ = writeln!(
        out,
        "  <PRODUCT Name=\"openCrate\" Version=\"{}\" Company=\"openCrate\"/>",
        escape(product_version)
    );
    let _ = writeln!(out, "  <COLLECTION Entries=\"{}\">", entries.len());
    for (index, entry) in entries.iter().enumerate() {
        write_entry(&mut out, index + 1, entry);
    }
    out.push_str("  </COLLECTION>\n  <PLAYLISTS>\n");
    // Top-level nodes in order: a playlist, or a folder the first time it is named.
    let mut top: Vec<(Option<&str>, Vec<&OfferPlaylist>)> = Vec::new();
    for playlist in playlists {
        match playlist.folder.as_deref() {
            None => top.push((None, vec![playlist])),
            Some(folder) => {
                if let Some((_, members)) = top.iter_mut().find(|(name, _)| *name == Some(folder)) {
                    members.push(playlist);
                } else {
                    top.push((Some(folder), vec![playlist]));
                }
            }
        }
    }
    let _ = writeln!(
        out,
        "    <NODE Type=\"0\" Name=\"ROOT\" Count=\"{}\">",
        top.len()
    );
    for (folder, members) in &top {
        match folder {
            None => {
                for playlist in members {
                    write_playlist(&mut out, playlist, "      ");
                }
            }
            Some(name) => {
                let _ = writeln!(
                    out,
                    "      <NODE Type=\"0\" Name=\"{}\" Count=\"{}\">",
                    escape(name),
                    members.len()
                );
                for playlist in members {
                    write_playlist(&mut out, playlist, "        ");
                }
                out.push_str("      </NODE>\n");
            }
        }
    }
    out.push_str("    </NODE>\n  </PLAYLISTS>\n</DJ_PLAYLISTS>\n");
    out
}

/// The tracks, numbered 1 up, and one playlist `playlist` under ROOT holding them in order.
pub fn write_offer(tracks: &[NewTrack], playlist: &str, product_version: &str) -> String {
    let entries: Vec<Entry> = tracks.iter().cloned().map(Entry::New).collect();
    let playlists = [OfferPlaylist {
        folder: None,
        name: playlist.to_owned(),
        entries: (0..tracks.len()).collect(),
    }];
    write_document(&entries, &playlists, product_version)
}
