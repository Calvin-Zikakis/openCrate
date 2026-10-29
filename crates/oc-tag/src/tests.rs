//! Synthetic audio only: tiny files assembled here, never a real recording.

use std::fs;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};
use tempfile::{tempdir, TempDir};

use crate::fixtures::{aiff, flac, mp3, wav};
use crate::{guess_from_path, read_file, ReadError, TagInfo};

fn put(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn tag(path: &Path, kind: TagType) {
    let mut tag = Tag::new(kind);
    tag.set_title("  Night Drive  ".to_owned());
    tag.set_artist("Studio 001".to_owned());
    tag.set_album("Between Spaces".to_owned());
    tag.set_genre("Deep House".to_owned());
    tag.set_track(3);
    tag.set_disk(2);
    tag.set_date(lofty::tag::items::Timestamp {
        year: 2019,
        ..lofty::tag::items::Timestamp::default()
    });
    tag.insert_text(ItemKey::AlbumArtist, "Various".to_owned());
    tag.insert_text(ItemKey::Label, "Synthetic Records".to_owned());
    tag.insert_text(ItemKey::InitialKey, "8A".to_owned());
    tag.insert_text(ItemKey::IntegerBpm, "128".to_owned());
    tag.save_to_path(path, WriteOptions::default()).unwrap();
}

fn expected_tags() -> TagInfo {
    TagInfo {
        title: Some("Night Drive".into()),
        artist: Some("Studio 001".into()),
        album: Some("Between Spaces".into()),
        album_artist: Some("Various".into()),
        label: Some("Synthetic Records".into()),
        genre: Some("Deep House".into()),
        key: Some("8A".into()),
        bpm: Some(128.0),
        track_number: Some(3),
        disc_number: Some(2),
        year: Some(2019),
    }
}

#[test]
fn wav_properties_and_tags_are_read() {
    let dir = tempdir().unwrap();
    let path = put(&dir, "a.wav", &wav(2));
    tag(&path, TagType::Id3v2);
    let info = read_file(&path).unwrap();
    assert_eq!(info.container, "WAV");
    assert_eq!(info.audio.duration_ms, 2_000);
    assert_eq!(
        (
            info.audio.sample_rate,
            info.audio.channels,
            info.audio.bit_depth
        ),
        (Some(44_100), Some(2), Some(16))
    );
    assert_eq!(info.tags, expected_tags());
}

#[test]
fn aiff_properties_and_tags_are_read() {
    let dir = tempdir().unwrap();
    let path = put(&dir, "a.aif", &aiff(3));
    tag(&path, TagType::Id3v2);
    let info = read_file(&path).unwrap();
    assert_eq!(info.container, "AIFF");
    assert_eq!(info.audio.duration_ms, 3_000);
    assert_eq!(
        (
            info.audio.sample_rate,
            info.audio.channels,
            info.audio.bit_depth
        ),
        (Some(44_100), Some(2), Some(16))
    );
    assert_eq!(info.tags, expected_tags());
}

#[test]
fn flac_properties_and_vorbis_comments_are_read() {
    let dir = tempdir().unwrap();
    let comments = [
        ("TITLE", "  Night Drive  "),
        ("ARTIST", "Studio 001"),
        ("ALBUM", "Between Spaces"),
        ("ALBUMARTIST", "Various"),
        ("LABEL", "Synthetic Records"),
        ("GENRE", "Deep House"),
        ("INITIALKEY", "8A"),
        ("BPM", "128"),
        ("TRACKNUMBER", "3"),
        ("DISCNUMBER", "2"),
        ("DATE", "2019"),
    ];
    let info = read_file(&put(&dir, "a.flac", &flac(4, &comments))).unwrap();
    assert_eq!(info.container, "FLAC");
    assert_eq!(info.audio.duration_ms, 4_000);
    assert_eq!(
        (
            info.audio.sample_rate,
            info.audio.channels,
            info.audio.bit_depth
        ),
        (Some(44_100), Some(2), Some(16))
    );
    assert_eq!(info.tags, expected_tags());
    let plain = read_file(&put(&dir, "plain.flac", &flac(1, &[]))).unwrap();
    assert!(plain.tags.is_empty());
}

#[test]
fn mp3_properties_and_id3_are_read() {
    let dir = tempdir().unwrap();
    let path = put(&dir, "a.mp3", &mp3(40));
    tag(&path, TagType::Id3v2);
    let info = read_file(&path).unwrap();
    assert_eq!(info.container, "MP3");
    assert_eq!(info.audio.sample_rate, Some(44_100));
    assert_eq!(info.audio.bitrate_kbps, Some(128));
    assert!(
        (900..1_200).contains(&info.audio.duration_ms),
        "{}",
        info.audio.duration_ms
    );
    assert_eq!(info.tags, expected_tags());
}

#[test]
fn the_content_decides_the_format_not_the_extension() {
    let dir = tempdir().unwrap();
    let path = put(&dir, "really-a-wav.mp3", &wav(1));
    assert_eq!(read_file(&path).unwrap().container, "WAV");
}

#[test]
fn an_untagged_file_has_audio_and_no_tags() {
    let dir = tempdir().unwrap();
    let info = read_file(&put(&dir, "plain.wav", &wav(1))).unwrap();
    assert!(info.tags.is_empty());
    assert_eq!(info.audio.duration_ms, 1_000);
}

#[test]
fn unreadable_files_are_errors_not_panics() {
    let dir = tempdir().unwrap();
    assert!(matches!(
        read_file(&put(&dir, "notes.mp3", b"just some text, not audio at all")),
        Err(ReadError::Parse(_))
    ));
    assert!(matches!(
        read_file(&put(&dir, "empty.flac", b"")),
        Err(ReadError::Parse(_))
    ));
    let truncated = &wav(1)[..30];
    assert!(read_file(&put(&dir, "cut.wav", truncated)).is_err());
    assert!(matches!(
        read_file(&dir.path().join("missing.mp3")),
        Err(ReadError::Io(_))
    ));
}

#[test]
fn text_is_trimmed_bounded_and_tempos_are_sane() {
    let dir = tempdir().unwrap();
    for (bpm, want) in [
        ("128", Some(128.0)),
        ("128.00", Some(128.0)),
        ("127,5", Some(127.5)),
        ("0", None),
        ("0.00", None),
        ("-90", None),
        ("fast", None),
        ("99999", None),
    ] {
        let path = put(&dir, "t.wav", &wav(1));
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("x".repeat(2_000));
        tag.set_artist("   ".to_owned());
        tag.insert_text(ItemKey::IntegerBpm, bpm.to_owned());
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        let info = read_file(&path).unwrap();
        assert_eq!(info.tags.bpm, want, "{bpm:?}");
        assert_eq!(
            info.tags.title.as_deref().map(|t| t.chars().count()),
            Some(512)
        );
        assert_eq!(info.tags.artist, None, "blank text is no text");
    }
}

#[test]
fn reading_leaves_the_file_exactly_as_it_was() {
    let dir = tempdir().unwrap();
    let path = put(&dir, "keep.flac", &flac(1, &[("TITLE", "Keep")]));
    let before = (
        fs::read(&path).unwrap(),
        fs::metadata(&path).unwrap().modified().unwrap(),
    );
    read_file(&path).unwrap();
    let after = (
        fs::read(&path).unwrap(),
        fs::metadata(&path).unwrap().modified().unwrap(),
    );
    assert_eq!(before, after);
}

fn guess(parts: &[&str]) -> TagInfo {
    let bytes: Vec<&[u8]> = parts.iter().map(|part| part.as_bytes()).collect();
    guess_from_path(&bytes)
}

#[test]
fn names_in_the_library_layout_give_a_fallback() {
    let tidy = guess(&[
        "Studio 001",
        "Between Spaces",
        "03 - Studio 001 - Night Drive.aif",
    ]);
    assert_eq!(tidy.artist.as_deref(), Some("Studio 001"));
    assert_eq!(tidy.title.as_deref(), Some("Night Drive"));
    assert_eq!(tidy.track_number, Some(3));
    assert_eq!(tidy.album.as_deref(), Some("Between Spaces"));
    assert_eq!(tidy.album_artist.as_deref(), Some("Studio 001"));

    let numbered = guess(&["A", "R", "7. Just A Title.mp3"]);
    assert_eq!(
        (numbered.track_number, numbered.title.as_deref()),
        (Some(7), Some("Just A Title"))
    );
    assert_eq!(
        numbered.artist.as_deref(),
        Some("A"),
        "the folder stands in for the artist"
    );

    let bare = guess(&["A", "R", "No Number Here.flac"]);
    assert_eq!(
        (bare.track_number, bare.title.as_deref()),
        (None, Some("No Number Here"))
    );

    let split = guess(&["A", "R", "Other Artist - A Title.wav"]);
    assert_eq!(split.artist.as_deref(), Some("Other Artist"));
    assert_eq!(split.album_artist.as_deref(), Some("A"));
    assert_eq!(split.title.as_deref(), Some("A Title"));
}

#[test]
fn bare_and_artist_album_track_numbers_are_read_and_names_with_numbers_are_not() {
    let bare = guess(&["Between Spaces", "03 Night Drive.flac"]);
    assert_eq!(
        (
            bare.track_number,
            bare.title.as_deref(),
            bare.album.as_deref()
        ),
        (Some(3), Some("Night Drive"), Some("Between Spaces"))
    );
    let shop = guess(&[
        "Studio 001 - Between Spaces",
        "Studio 001 - Between Spaces - 11 Soft Focus (Dub Mix).flac",
    ]);
    assert_eq!(shop.artist.as_deref(), Some("Studio 001"));
    assert_eq!(
        shop.album.as_deref(),
        Some("Between Spaces"),
        "not the folder name"
    );
    assert_eq!(shop.track_number, Some(11));
    assert_eq!(shop.title.as_deref(), Some("Soft Focus (Dub Mix)"));
    for (file, artist, title) in [
        ("50 Cent - In Da Club.mp3", Some("50 Cent"), "In Da Club"),
        ("99 Problems.mp3", None, "99 Problems"),
        ("808 State - Pacific.mp3", Some("808 State"), "Pacific"),
    ] {
        let read = guess(&["R", file]);
        assert_eq!(read.track_number, None, "{file}");
        assert_eq!(
            (read.artist.as_deref(), read.title.as_deref()),
            (artist, Some(title)),
            "{file}"
        );
    }
    assert_eq!(guess(&["R", "09 Late.mp3"]).track_number, Some(9));
    assert_eq!(
        guess(&["R", "00 Intro.mp3"]).track_number,
        None,
        "no track 0"
    );
}

#[test]
fn odd_names_do_not_confuse_the_fallback() {
    let year = guess(&["A", "R", "2019 mix.aif"]);
    assert_eq!(
        (year.track_number, year.title.as_deref()),
        (None, Some("2019 mix"))
    );
    let dashes = guess(&["A", "R", "01 - - .mp3"]);
    assert_eq!(dashes.track_number, Some(1));
    assert!(dashes.title.is_some());
    let flat = guess(&["Lonely Release", "t.aif"]);
    assert_eq!(
        (flat.album.as_deref(), flat.artist.as_deref()),
        (Some("Lonely Release"), None)
    );
    let single = guess(&["only-a-file.mp3"]);
    assert_eq!(
        (single.title.as_deref(), single.album.as_deref()),
        (Some("only-a-file"), None)
    );
    assert!(guess_from_path(&[]).is_empty());
    let raw = guess_from_path(&[b"A", b"R", b"\xFF\xFE bad.mp3"]);
    assert!(raw.title.is_some(), "non-text names still give a title");
}

#[test]
fn every_artist_is_kept_and_shown_comma_separated() {
    use lofty::tag::{ItemValue, TagItem};
    let dir = tempdir().unwrap();
    // ID3v2.4 keeps several artists in one frame, separated by NUL.
    let mp3_path = put(&dir, "a.mp3", &mp3(40));
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title("Night Drive".to_owned());
    tag.set_artist("Studio 001\0Guest Vocalist".to_owned());
    tag.save_to_path(&mp3_path, WriteOptions::default())
        .unwrap();
    assert_eq!(
        read_file(&mp3_path).unwrap().tags.artist.as_deref(),
        Some("Studio 001, Guest Vocalist")
    );

    // Vorbis comments repeat the field instead.
    let comments = [
        ("TITLE", "Night Drive"),
        ("ARTIST", "Studio 001"),
        ("ARTIST", "Guest Vocalist"),
        ("ARTIST", "studio 001"),
        ("ALBUMARTIST", "Studio 001"),
        ("ALBUMARTIST", "Friends"),
    ];
    let info = read_file(&put(&dir, "b.flac", &flac(1, &comments))).unwrap();
    assert_eq!(
        info.tags.artist.as_deref(),
        Some("Studio 001, Guest Vocalist"),
        "a repeat that differs only in case is dropped"
    );
    assert_eq!(
        info.tags.album_artist.as_deref(),
        Some("Studio 001, Friends")
    );

    // Names with "/" or "&" are one name, not several.
    let wav_path = put(&dir, "c.wav", &wav(1));
    let mut tag = Tag::new(TagType::Id3v2);
    tag.push(TagItem::new(
        ItemKey::TrackArtist,
        ItemValue::Text("AC/DC & Friends".to_owned()),
    ));
    tag.save_to_path(&wav_path, WriteOptions::default())
        .unwrap();
    assert_eq!(
        read_file(&wav_path).unwrap().tags.artist.as_deref(),
        Some("AC/DC & Friends")
    );
}
