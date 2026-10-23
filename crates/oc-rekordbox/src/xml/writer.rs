//! A small rekordbox XML document offering new tracks: a collection and one playlist. It holds
//! only what openCrate knows for sure; rekordbox analyses the files itself.

use std::fmt::Write as _;

use crate::location::encode_location;

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

/// The tracks, numbered 1 up, and one playlist `playlist` under ROOT holding them in order.
pub fn write_offer(tracks: &[NewTrack], playlist: &str, product_version: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\">\n");
    let _ = writeln!(
        out,
        "  <PRODUCT Name=\"openCrate\" Version=\"{}\" Company=\"openCrate\"/>",
        escape(product_version)
    );
    let _ = writeln!(out, "  <COLLECTION Entries=\"{}\">", tracks.len());
    for (index, track) in tracks.iter().enumerate() {
        let _ = write!(
            out,
            "    <TRACK TrackID=\"{}\" Name=\"{}\" Artist=\"{}\" Album=\"{}\" Kind=\"{} File\" Size=\"{}\" TotalTime=\"{}\"",
            index + 1,
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
    out.push_str(
        "  </COLLECTION>\n  <PLAYLISTS>\n    <NODE Type=\"0\" Name=\"ROOT\" Count=\"1\">\n",
    );
    let _ = writeln!(
        out,
        "      <NODE Name=\"{}\" Type=\"1\" KeyType=\"0\" Entries=\"{}\">",
        escape(playlist),
        tracks.len()
    );
    for index in 1..=tracks.len() {
        let _ = writeln!(out, "        <TRACK Key=\"{index}\"/>");
    }
    out.push_str("      </NODE>\n    </NODE>\n  </PLAYLISTS>\n</DJ_PLAYLISTS>\n");
    out
}
