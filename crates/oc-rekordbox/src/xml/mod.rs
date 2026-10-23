//! Read a complete rekordbox XML document without filesystem or database access.

mod parser;
#[cfg(test)]
mod tests;
mod writer;

use std::collections::BTreeMap;
use std::io::BufRead;

use thiserror::Error;

use crate::location::LocationError;

pub use writer::{write_document, write_offer, Entry, NewTrack, OfferPlaylist};

pub type Attributes = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    pub attributes: Attributes,
    pub product: Option<Attributes>,
    pub collection_attributes: Attributes,
    pub tracks: Vec<Track>,
    pub playlists: Vec<PlaylistNode>,
    /// IDs referenced by playlists but absent from this export, not missing files.
    pub unresolved_track_ids: Vec<u64>,
    /// Unrecognized extension element roots skipped by the reader, sorted and deduplicated.
    pub ignored_elements: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: u64,
    pub location: String,
    /// Byte-preserving decoded path, not a resolved filesystem identity.
    pub location_bytes: Vec<u8>,
    pub attributes: Attributes,
    pub cues: Vec<Cue>,
    pub tempos: Vec<Tempo>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub kind: u8,
    pub number: Option<i32>,
    pub start_seconds: f64,
    pub end_seconds: Option<f64>,
    pub attributes: Attributes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tempo {
    pub start_seconds: f64,
    pub bpm: f64,
    pub meter: Option<String>,
    pub beat: Option<u32>,
    pub attributes: Attributes,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlaylistNode {
    Folder {
        attributes: Attributes,
        children: Vec<Self>,
    },
    Playlist {
        attributes: Attributes,
        track_ids: Vec<u64>,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub input_bytes: u64,
    pub tracks: usize,
    pub playlist_references: usize,
    pub depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            input_bytes: 128 * 1024 * 1024,
            tracks: 1_000_000,
            playlist_references: 1_000_000,
            depth: 128,
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("XML input exceeds the {0} limit")]
    Limit(&'static str),
    #[error("invalid XML syntax")]
    Syntax(#[from] quick_xml::Error),
    #[error("invalid or duplicate XML attributes")]
    Attributes,
    #[error("XML attributes must contain valid UTF-8")]
    Encoding,
    #[error("DTD declarations are not supported")]
    Doctype,
    #[error("unexpected XML structure: {0}")]
    Structure(&'static str),
    #[error("required attribute is missing: {0}")]
    Missing(&'static str),
    #[error("invalid numeric attribute: {0}")]
    Number(&'static str),
    #[error("duplicate collection TrackID")]
    DuplicateTrack,
    #[error("declared entry count does not match the document: {0}")]
    Count(&'static str),
    #[error("only TrackID-based playlist references (KeyType=0) are supported")]
    KeyType,
    #[error("invalid rekordbox Location: {0}")]
    Location(#[from] LocationError),
}

pub fn read(reader: impl BufRead) -> Result<Collection, Error> {
    read_with_limits(reader, Limits::default())
}

/// Custom bounds are intended for tests and callers with stricter resource budgets.
pub fn read_with_limits(reader: impl BufRead, limits: Limits) -> Result<Collection, Error> {
    parser::read(reader, limits)
}
