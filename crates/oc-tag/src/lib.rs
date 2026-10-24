//! Read audio properties and tags from one file. Reading never writes; [`write_tags`] exists
//! only for the importer's own staged copies. Cover art is loaded only by the separate [`cover`]
//! functions. Text is trimmed and bounded, empty values become `None`, and
//! several tags in one file are merged with the primary tag winning.

mod cover;
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
mod guess;
#[cfg(test)]
mod tests;
mod tidy;
mod write;

use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use thiserror::Error;

pub use cover::{embedded_cover, folder_cover, Cover, MAX_COVER_BYTES};
pub use guess::guess_from_path;
pub use tidy::{tidy, tidy_name};
pub use write::{rewrite_cover, rewrite_tags, write_tags, TextTag};

/// Longest text kept from any one tag field.
const MAX_TEXT_CHARS: usize = 512;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioInfo {
    pub duration_ms: i64,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TagInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub label: Option<String>,
    pub genre: Option<String>,
    pub key: Option<String>,
    pub bpm: Option<f64>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<i32>,
}

impl TagInfo {
    /// Fill every field this one lacks from `other`.
    pub fn fill_from(&mut self, other: &Self) {
        macro_rules! fill {
            ($($field:ident),+) => { $(if self.$field.is_none() { self.$field.clone_from(&other.$field); })+ };
        }
        fill!(
            title,
            artist,
            album,
            album_artist,
            label,
            genre,
            key,
            bpm,
            track_number,
            disc_number,
            year
        );
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileInfo {
    /// What the content is, not what the extension says: `AIFF`, `MP3`, `FLAC`, `WAV`...
    pub container: &'static str,
    pub audio: AudioInfo,
    pub tags: TagInfo,
}

#[derive(Debug, Error)]
pub enum ReadError {
    #[error("file could not be opened: {0}")]
    Io(#[from] io::Error),
    #[error("not readable as audio: {0}")]
    Parse(String),
}

#[must_use]
pub fn container_label(file_type: FileType) -> &'static str {
    match file_type {
        FileType::Aiff => "AIFF",
        FileType::Mpeg => "MP3",
        FileType::Flac => "FLAC",
        FileType::Wav => "WAV",
        FileType::Mp4 => "M4A",
        FileType::Aac => "AAC",
        FileType::Opus | FileType::Vorbis | FileType::Speex => "OGG",
        FileType::Ape => "APE",
        FileType::Mpc => "MPC",
        FileType::WavPack => "WV",
        _ => "OTHER",
    }
}

fn text(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(MAX_TEXT_CHARS).collect())
}

/// `128`, `128.00` and `128,5` are tempos; zero, negative and nonsense are not.
fn tempo(value: Option<&str>) -> Option<f64> {
    let parsed = value?.trim().replace(',', ".").parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0 && parsed < 1_000.0).then_some(parsed)
}

/// Every value of a field that may hold several names: separate items, or one value holding
/// ID3v2.4's NUL separator. Joined with ", " in their order, duplicates dropped. Names are
/// never split on "/" or "&", which belong to names such as "AC/DC".
fn names(tag: &Tag, key: &ItemKey) -> Option<String> {
    let mut found: Vec<String> = Vec::new();
    for value in tag.get_strings(key) {
        for name in value.split('\0') {
            if let Some(name) = text(Some(name)) {
                if !found.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
                    found.push(name);
                }
            }
        }
    }
    text(Some(&found.join(", ")))
}

fn one_tag(tag: &Tag) -> TagInfo {
    let item = |key: &ItemKey| text(tag.get_string(key));
    TagInfo {
        title: text(tag.title().as_deref()),
        artist: names(tag, &ItemKey::TrackArtist).or_else(|| text(tag.artist().as_deref())),
        album: text(tag.album().as_deref()),
        album_artist: names(tag, &ItemKey::AlbumArtist),
        label: item(&ItemKey::Label),
        genre: text(tag.genre().as_deref()),
        key: item(&ItemKey::InitialKey),
        bpm: tempo(
            tag.get_string(&ItemKey::IntegerBpm)
                .or_else(|| tag.get_string(&ItemKey::Bpm)),
        ),
        track_number: tag.track(),
        disc_number: tag.disk(),
        year: tag.year().and_then(|year| i32::try_from(year).ok()),
    }
}

/// Open `path` read-only, detect the format from its content and read what it holds.
pub fn read_file(path: &Path) -> Result<FileInfo, ReadError> {
    let options = ParseOptions::new().read_cover_art(false);
    let file = File::open(path)?;
    let tagged = Probe::new(BufReader::new(file))
        .options(options)
        .guess_file_type()?
        .read()
        .map_err(|error| ReadError::Parse(error.to_string()))?;
    let properties = tagged.properties();
    let audio = AudioInfo {
        duration_ms: i64::try_from(properties.duration().as_millis()).unwrap_or(i64::MAX),
        bitrate_kbps: properties.audio_bitrate().or(properties.overall_bitrate()),
        sample_rate: properties.sample_rate(),
        bit_depth: properties.bit_depth(),
        channels: properties.channels(),
    };
    let mut tags = TagInfo::default();
    if let Some(primary) = tagged.primary_tag() {
        tags.fill_from(&one_tag(primary));
    }
    for other in tagged.tags() {
        tags.fill_from(&one_tag(other));
    }
    Ok(FileInfo {
        container: container_label(tagged.file_type()),
        audio,
        tags,
    })
}
