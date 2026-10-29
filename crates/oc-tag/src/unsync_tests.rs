//! An ID3v2.4 tag whose header and every frame are marked unsynchronised, with data length
//! indicators: the layout Bandcamp's AIFF downloads carry. Built byte by byte here; no real file.

use std::fs;

use tempfile::tempdir;

use crate::fixtures::aiff;
use crate::{embedded_cover, read_file, rewrite_tags, write_tags, TagInfo, TextTag};

fn syncsafe(value: usize) -> [u8; 4] {
    let value = u32::try_from(value).unwrap();
    [
        ((value >> 21) & 0x7F) as u8,
        ((value >> 14) & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
        (value & 0x7F) as u8,
    ]
}

/// ID3 unsynchronisation: a zero byte after every 0xFF that is followed by a zero byte, a byte of
/// 0xE0 or more, or nothing.
fn unsynchronise(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 8);
    for (index, byte) in bytes.iter().enumerate() {
        out.push(*byte);
        if *byte == 0xFF
            && bytes
                .get(index + 1)
                .is_none_or(|next| *next == 0 || *next >= 0xE0)
        {
            out.push(0);
        }
    }
    out
}

/// A v2.4 frame flagged unsynchronised (0x02) with a data length indicator (0x01).
fn frame(id: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut body = syncsafe(payload.len()).to_vec();
    body.extend(unsynchronise(payload));
    let mut out = id.to_vec();
    out.extend(syncsafe(body.len()));
    out.extend([0x00, 0x03]);
    out.extend(body);
    out
}

fn latin1(id: [u8; 4], text: &str) -> Vec<u8> {
    let mut payload = vec![0];
    payload.extend(text.bytes());
    frame(id, &payload)
}

fn utf16(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend(unit.to_le_bytes());
    }
    out
}

/// The tag, its header flagged unsynchronised (0x80) as well as each frame.
fn bandcamp_tag() -> Vec<u8> {
    let mut comment = vec![1];
    comment.extend(b"eng");
    comment.extend(utf16(""));
    comment.extend([0, 0]);
    comment.extend(utf16("Visit the label's page"));
    let mut picture = vec![0];
    picture.extend(b"image/jpeg\0");
    picture.push(3);
    picture.extend(b"cover\0");
    picture.extend([
        0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 0xFF, 0xD9,
    ]);
    let mut frames = Vec::new();
    frames.extend(latin1(*b"TALB", "Between Spaces"));
    frames.extend(latin1(*b"TPE2", "Studio 001"));
    frames.extend(latin1(*b"TIT2", "Night Drive"));
    frames.extend(latin1(*b"TPE1", "Studio 001"));
    frames.extend(frame(*b"COMM", &comment));
    frames.extend(frame(*b"APIC", &picture));
    frames.extend(latin1(*b"TCON", "House"));
    let mut tag = b"ID3".to_vec();
    tag.extend([4, 0, 0x80]);
    tag.extend(syncsafe(frames.len()));
    tag.extend(frames);
    tag
}

/// An AIFF with the tag in an `ID3 ` chunk after the audio, as Bandcamp writes it.
fn bandcamp_aiff() -> Vec<u8> {
    let mut file = aiff(1);
    let tag = bandcamp_tag();
    file.extend(b"ID3 ");
    file.extend(u32::try_from(tag.len()).unwrap().to_be_bytes());
    file.extend(&tag);
    if tag.len() % 2 == 1 {
        file.push(0);
    }
    let form = u32::try_from(file.len() - 8).unwrap();
    file[4..8].copy_from_slice(&form.to_be_bytes());
    file
}

#[test]
fn a_tag_unsynchronised_both_as_a_whole_and_frame_by_frame_is_read() {
    let folder = tempdir().unwrap();
    let path = folder.path().join("01 Night Drive.aiff");
    fs::write(&path, bandcamp_aiff()).unwrap();
    let tags = read_file(&path).unwrap().tags;
    assert_eq!(tags.title.as_deref(), Some("Night Drive"));
    assert_eq!(tags.artist.as_deref(), Some("Studio 001"));
    assert_eq!(tags.album.as_deref(), Some("Between Spaces"));
    assert_eq!(tags.album_artist.as_deref(), Some("Studio 001"));
    assert_eq!(
        tags.genre.as_deref(),
        Some("House"),
        "the frame after the picture"
    );
    assert_eq!(
        embedded_cover(&path).map(|cover| cover.mime),
        Some("image/jpeg")
    );
}

#[test]
fn such_a_file_can_be_retagged_and_read_again() {
    let folder = tempdir().unwrap();
    let path = folder.path().join("01 Night Drive.aiff");
    fs::write(&path, bandcamp_aiff()).unwrap();
    rewrite_tags(&path, &[(TextTag::Genre, "Deep House".to_owned())]).unwrap();
    let tags = read_file(&path).unwrap().tags;
    assert_eq!(tags.genre.as_deref(), Some("Deep House"));
    assert_eq!(
        tags.album.as_deref(),
        Some("Between Spaces"),
        "the rest is kept"
    );

    let copy = folder.path().join("copy.aiff");
    fs::write(&copy, bandcamp_aiff()).unwrap();
    let values = TagInfo {
        year: Some(2026),
        ..TagInfo::default()
    };
    write_tags(&copy, &values, None).unwrap();
    assert_eq!(read_file(&copy).unwrap().tags.year, Some(2026));
}
