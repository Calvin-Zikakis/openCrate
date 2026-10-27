//! Tiny synthetic audio files for tests: valid enough to be read, silent, and never a real
//! recording. Enabled for this crate's own tests and, with the `fixtures` feature, for others.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support: a fixture that cannot be built should fail the test loudly"
)]

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};

use crate::TagInfo;

pub const RATE: u32 = 44_100;

pub fn wav(seconds: u32) -> Vec<u8> {
    wav_frames(RATE * seconds)
}

/// A stereo 16-bit WAV of exactly `frames` sample frames, for tests that need many small files.
pub fn wav_frames(frames: u32) -> Vec<u8> {
    let data = frames * 4;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(RATE.to_le_bytes());
    bytes.extend((RATE * 4).to_le_bytes());
    bytes.extend(4u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data.to_le_bytes());
    bytes.resize(bytes.len() + usize::try_from(data).unwrap(), 0);
    bytes
}

pub fn aiff(seconds: u32) -> Vec<u8> {
    let frames = RATE * seconds;
    let data = frames * 4;
    let ssnd = 8 + data;
    let mut bytes = b"FORM".to_vec();
    bytes.extend((4 + 8 + 18 + 8 + ssnd).to_be_bytes());
    bytes.extend(b"AIFFCOMM");
    bytes.extend(18u32.to_be_bytes());
    bytes.extend(2i16.to_be_bytes());
    bytes.extend(frames.to_be_bytes());
    bytes.extend(16i16.to_be_bytes());
    bytes.extend([0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]); // 44100 as an 80-bit float
    bytes.extend(b"SSND");
    bytes.extend(ssnd.to_be_bytes());
    bytes.extend([0u8; 8]);
    bytes.resize(bytes.len() + usize::try_from(data).unwrap(), 0);
    bytes
}

/// STREAMINFO, then a Vorbis comment block holding `comments`. Built by hand because
/// lofty's writer cannot add tags to a FLAC that has no audio frames.
pub fn flac(seconds: u32, comments: &[(&str, &str)]) -> Vec<u8> {
    let packed: u64 =
        (u64::from(RATE) << 44) | (1u64 << 41) | (15u64 << 36) | u64::from(RATE * seconds);
    let mut bytes = b"fLaC".to_vec();
    let last = if comments.is_empty() { 0x80 } else { 0 };
    bytes.extend([last, 0, 0, 34]);
    bytes.extend(4096u16.to_be_bytes());
    bytes.extend(4096u16.to_be_bytes());
    bytes.extend([0u8; 6]);
    bytes.extend(packed.to_be_bytes());
    bytes.extend([0u8; 16]);
    if !comments.is_empty() {
        let vendor = b"oc-tag test";
        let mut block = Vec::new();
        block.extend(u32::try_from(vendor.len()).unwrap().to_le_bytes());
        block.extend(vendor);
        block.extend(u32::try_from(comments.len()).unwrap().to_le_bytes());
        for (key, value) in comments {
            let entry = format!("{key}={value}");
            block.extend(u32::try_from(entry.len()).unwrap().to_le_bytes());
            block.extend(entry.as_bytes());
        }
        let length = u32::try_from(block.len()).unwrap().to_be_bytes();
        bytes.extend([0x80 | 4, length[1], length[2], length[3]]);
        bytes.extend(block);
    }
    bytes
}

pub fn mp3(frames: usize) -> Vec<u8> {
    let mut frame = vec![0xFF, 0xFB, 0x90, 0x00]; // MPEG-1 Layer III, 128 kbps, 44.1 kHz
    frame.resize(417, 0);
    frame.repeat(frames)
}

/// Write `info` into the file's own tag format (`ID3v2` for WAV, AIFF and MP3). FLAC cannot be
/// tagged this way when it has no audio frames; give [`flac`] its comments instead.
pub fn write_tags(path: &Path, info: &TagInfo) {
    write_tags_and_cover(path, info, None);
}

/// Like [`write_tags`], with a front-cover picture embedded when `cover` is given.
pub fn write_tags_and_cover(path: &Path, info: &TagInfo, cover: Option<&[u8]>) {
    let mut tag = Tag::new(TagType::Id3v2);
    if let Some(data) = cover {
        tag.push_picture(
            lofty::picture::Picture::unchecked(data.to_vec())
                .pic_type(lofty::picture::PictureType::CoverFront)
                .mime_type(lofty::picture::MimeType::Png)
                .build(),
        );
    }
    if let Some(value) = &info.title {
        tag.set_title(value.clone());
    }
    if let Some(value) = &info.artist {
        tag.set_artist(value.clone());
    }
    if let Some(value) = &info.album {
        tag.set_album(value.clone());
    }
    if let Some(value) = &info.genre {
        tag.set_genre(value.clone());
    }
    if let Some(value) = info.track_number {
        tag.set_track(value);
    }
    if let Some(value) = info.disc_number {
        tag.set_disk(value);
    }
    if let Some(year) = info.year.and_then(|year| u16::try_from(year).ok()) {
        tag.set_date(lofty::tag::items::Timestamp {
            year,
            ..lofty::tag::items::Timestamp::default()
        });
    }
    for (key, value) in [
        (ItemKey::AlbumArtist, &info.album_artist),
        (ItemKey::Label, &info.label),
        (ItemKey::InitialKey, &info.key),
    ] {
        if let Some(value) = value {
            tag.insert_text(key, value.clone());
        }
    }
    if let Some(bpm) = info.bpm {
        tag.insert_text(ItemKey::IntegerBpm, format!("{bpm}"));
    }
    tag.save_to_path(path, WriteOptions::default())
        .expect("fixture tags can be written");
}
