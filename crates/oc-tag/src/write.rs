//! Writing tags. `write_tags` writes into a staged copy before it is filed and is never used on
//! a file in a download folder or in the library. `rewrite_tags` changes a file already in the
//! library, only for a change the owner ticked in the rekordbox sync, and never in place.

use std::path::Path;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::id3::v2::{Frame, FrameId, Id3v2Tag, TimestampFrame};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::items::Timestamp;
use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};
use lofty::TextEncoding;

use crate::{Cover, ReadError};

fn read_with(path: &Path, cover_art: bool) -> Result<TaggedFile, ReadError> {
    let file = std::fs::File::open(path)?;
    Probe::new(std::io::BufReader::new(file))
        .options(ParseOptions::new().read_cover_art(cover_art))
        .guess_file_type()?
        .read()
        .map_err(|error| ReadError::Parse(error.to_string()))
}

/// A file's tags whole, pictures included, so that writing them back keeps everything. When
/// only the picture fails to read, the file is refused as [`ReadError::DamagedPicture`].
fn read_whole(path: &Path) -> Result<TaggedFile, ReadError> {
    read_with(path, true).map_err(|error| match read_with(path, false) {
        Ok(_) => ReadError::DamagedPicture,
        Err(_) => error,
    })
}

/// lofty writes a picture with no description into `ID3v2.3` as UTF-16 with a stray one-byte
/// terminator, which no reader accepts afterwards (lofty included), and the whole tag then fails
/// to read. Such a picture is given a description before anything is saved.
fn describe_pictures(tag: &mut Tag) {
    for index in 0..tag.pictures().len() {
        let picture = &tag.pictures()[index];
        if picture.description().is_none_or(str::is_empty) {
            let mut described = picture.clone();
            described.set_description(Some("Cover".into()));
            tag.set_picture(index, described);
        }
    }
}

/// Set every field that `values` holds, replacing what the file had for those fields only;
/// other fields and other tags stay. `cover` is embedded only if the file has no picture yet.
/// `ID3v2` tags are written as version 2.3, which rekordbox reads most reliably (owner question 2).
pub fn write_tags(
    path: &Path,
    values: &crate::TagInfo,
    cover: Option<&Cover>,
) -> Result<(), ReadError> {
    let mut tagged = read_whole(path)?;
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
    }
    let Some(tag) = tagged.primary_tag_mut() else {
        return Err(ReadError::Parse("this file cannot hold tags".into()));
    };
    describe_pictures(tag);
    if let Some(value) = &values.title {
        tag.set_title(value.clone());
    }
    if let Some(value) = &values.artist {
        tag.set_artist(value.clone());
    }
    if let Some(value) = &values.album {
        tag.set_album(value.clone());
    }
    if let Some(value) = &values.genre {
        tag.set_genre(value.clone());
    }
    if let Some(value) = values.track_number {
        tag.set_track(value);
    }
    if let Some(value) = values.disc_number {
        tag.set_disk(value);
    }
    let year = values.year.and_then(|year| u16::try_from(year).ok());
    if let Some(value) = year {
        tag.set_year(u32::from(value));
    }
    for (key, value) in [
        (ItemKey::AlbumArtist, &values.album_artist),
        (ItemKey::Label, &values.label),
        (ItemKey::InitialKey, &values.key),
    ] {
        if let Some(value) = value {
            tag.insert_text(key, value.clone());
        }
    }
    if let Some(bpm) = values.bpm {
        tag.insert_text(ItemKey::IntegerBpm, format!("{}", bpm.round()));
    }
    if let Some(cover) = cover {
        if tag.picture_count() == 0 {
            tag.push_picture(Picture::new_unchecked(
                PictureType::CoverFront,
                Some(MimeType::from_str(cover.mime)),
                // With no description, lofty writes a one-byte terminator after UTF-16 text,
                // which readers (lofty included) reject.
                Some("Cover".into()),
                cover.data.clone(),
            ));
        }
    }
    let mut options = WriteOptions::default();
    let options = options.use_id3v23(true);
    let failed = |error: lofty::error::LoftyError| ReadError::Parse(error.to_string());
    if tag.tag_type() != TagType::Id3v2 {
        return tagged.save_to_path(path, options).map_err(failed);
    }
    // lofty writes ID3v2.3's TYER only from a timestamp frame and silently drops a plain year,
    // so the ID3 tag is saved on its own with the year as a timestamp.
    let mut id3 = Id3v2Tag::from(tag.clone());
    if let Some(year) = year {
        id3.insert(Frame::Timestamp(TimestampFrame::new(
            FrameId::Valid("TDRC".into()),
            TextEncoding::UTF8,
            Timestamp {
                year,
                ..Timestamp::default()
            },
        )));
    }
    id3.save_to_path(path, options).map_err(failed)
}

/// A field the rekordbox sync or the track editor can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextTag {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Genre,
    Year,
    Label,
    TrackNumber,
}

/// Change `changes` in a file already in the library; an empty value removes the field. The
/// file is never half written: the change goes into a copy beside it, which is flushed to disk
/// and then takes the file's place in one rename. On any failure the copy is removed and the
/// file is exactly as it was.
pub fn rewrite_tags(path: &Path, changes: &[(TextTag, String)]) -> Result<(), ReadError> {
    rewrite_through_copy(path, |copy| retag_copy(path, copy, changes))
}

/// Set a file's front cover to `cover`, replacing any picture it had; `None` removes every
/// picture. Through a copy, as [`rewrite_tags`].
pub fn rewrite_cover(path: &Path, cover: Option<&Cover>) -> Result<(), ReadError> {
    rewrite_through_copy(path, |copy| {
        std::fs::copy(path, copy)?;
        // Every picture is replaced, so a damaged one need not be read: it goes.
        let mut tagged = match read_whole(copy) {
            Err(ReadError::DamagedPicture) => read_with(copy, false)?,
            other => other?,
        };
        if tagged.primary_tag().is_none() {
            tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
        }
        let Some(tag) = tagged.primary_tag_mut() else {
            return Err(ReadError::Parse("this file cannot hold tags".into()));
        };
        while tag.picture_count() > 0 {
            tag.remove_picture(0);
        }
        if let Some(cover) = cover {
            tag.push_picture(Picture::new_unchecked(
                PictureType::CoverFront,
                Some(MimeType::from_str(cover.mime)),
                Some("Cover".into()),
                cover.data.clone(),
            ));
        }
        let mut options = WriteOptions::default();
        tagged
            .save_to_path(copy, options.use_id3v23(true))
            .map_err(|error| ReadError::Parse(error.to_string()))
    })
}

/// `edit` fills a copy beside the file; the copy is flushed and takes the file's place in one
/// rename. On any failure the copy is removed and the file is exactly as it was.
fn rewrite_through_copy(
    path: &Path,
    edit: impl FnOnce(&Path) -> Result<(), ReadError>,
) -> Result<(), ReadError> {
    let name = path
        .file_name()
        .ok_or_else(|| ReadError::Parse("not a file".into()))?;
    let copy = path.with_file_name(format!(".opencrate-retag-{}", name.to_string_lossy()));
    let result = edit(&copy).and_then(|()| {
        // Flushing needs write access on Windows.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&copy)?
            .sync_all()?;
        std::fs::rename(&copy, path)?;
        Ok(())
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&copy);
    }
    result
}

fn retag_copy(path: &Path, copy: &Path, changes: &[(TextTag, String)]) -> Result<(), ReadError> {
    std::fs::copy(path, copy)?;
    let cleared: Vec<TextTag> = changes
        .iter()
        .filter(|(_, value)| value.trim().is_empty())
        .map(|(tag, _)| *tag)
        .collect();
    if !cleared.is_empty() {
        clear(copy, &cleared)?;
    }
    let mut values = crate::TagInfo::default();
    for (tag, value) in changes.iter().filter(|(_, value)| !value.trim().is_empty()) {
        let value = value.trim().to_owned();
        match tag {
            TextTag::Title => values.title = Some(value),
            TextTag::Artist => values.artist = Some(value),
            TextTag::Album => values.album = Some(value),
            TextTag::Label => values.label = Some(value),
            TextTag::AlbumArtist => values.album_artist = Some(value),
            TextTag::Genre => values.genre = Some(value),
            TextTag::TrackNumber => {
                values.track_number =
                    Some(value.parse().map_err(|_| {
                        ReadError::Parse(format!("\"{value}\" is not a track number"))
                    })?);
            }
            TextTag::Year => {
                values.year = Some(
                    value
                        .parse()
                        .map_err(|_| ReadError::Parse(format!("\"{value}\" is not a year")))?,
                );
            }
        }
    }
    if values.is_empty() {
        return Ok(());
    }
    write_tags(copy, &values, None)
}

fn clear(path: &Path, cleared: &[TextTag]) -> Result<(), ReadError> {
    let mut tagged = read_whole(path)?;
    let Some(tag) = tagged.primary_tag_mut() else {
        return Ok(());
    };
    describe_pictures(tag);
    for field in cleared {
        match field {
            TextTag::Title => tag.remove_title(),
            TextTag::Artist => tag.remove_artist(),
            TextTag::Album => tag.remove_album(),
            TextTag::Year => tag.remove_year(),
            TextTag::Label => tag.remove_key(&ItemKey::Label),
            TextTag::AlbumArtist => tag.remove_key(&ItemKey::AlbumArtist),
            TextTag::Genre => tag.remove_genre(),
            TextTag::TrackNumber => tag.remove_track(),
        }
    }
    let mut options = WriteOptions::default();
    let options = options.use_id3v23(true);
    tagged
        .save_to_path(path, options)
        .map_err(|error| ReadError::Parse(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::fixtures::{aiff, mp3, write_tags as fixture_tags};
    use crate::{embedded_cover, read_file, TagInfo};

    use super::*;

    const PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
    ];

    #[test]
    fn a_library_file_is_retagged_through_a_copy_and_a_cleared_field_goes() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.mp3");
        fs::write(&path, mp3(20)).unwrap();
        fixture_tags(
            &path,
            &TagInfo {
                artist: Some("Studio 001".into()),
                title: Some("Night Drive".into()),
                label: Some("Old Label".into()),
                year: Some(2019),
                ..TagInfo::default()
            },
        );
        rewrite_tags(
            &path,
            &[
                (TextTag::Title, "Night Drive (Extended)".into()),
                (TextTag::Label, String::new()),
                (TextTag::Year, "2021".into()),
            ],
        )
        .unwrap();
        let tags = read_file(&path).unwrap().tags;
        assert_eq!(tags.title.as_deref(), Some("Night Drive (Extended)"));
        assert_eq!(tags.label, None);
        assert_eq!(tags.year, Some(2021));
        assert_eq!(
            tags.artist.as_deref(),
            Some("Studio 001"),
            "untouched fields stay"
        );
        let names: Vec<_> = fs::read_dir(folder.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["a.mp3"], "no copy is left behind");
    }

    #[test]
    fn genre_album_artist_and_track_number_are_set_and_cleared() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("b.aif");
        fs::write(&path, aiff(2)).unwrap();
        rewrite_tags(
            &path,
            &[
                (TextTag::Genre, "Dub Techno".into()),
                (TextTag::AlbumArtist, "Various".into()),
                (TextTag::TrackNumber, "7".into()),
            ],
        )
        .unwrap();
        let tags = read_file(&path).unwrap().tags;
        assert_eq!(
            (
                tags.genre.as_deref(),
                tags.album_artist.as_deref(),
                tags.track_number
            ),
            (Some("Dub Techno"), Some("Various"), Some(7))
        );
        rewrite_tags(
            &path,
            &[
                (TextTag::Genre, String::new()),
                (TextTag::TrackNumber, String::new()),
            ],
        )
        .unwrap();
        let tags = read_file(&path).unwrap().tags;
        assert_eq!((tags.genre, tags.track_number), (None, None));
        assert!(rewrite_tags(&path, &[(TextTag::TrackNumber, "seven".into())]).is_err());
    }

    #[test]
    fn a_cover_is_embedded_replaced_and_removed_through_a_copy() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("c.aif");
        fs::write(&path, aiff(2)).unwrap();
        fixture_tags(
            &path,
            &TagInfo {
                title: Some("Night Drive".into()),
                ..TagInfo::default()
            },
        );
        assert_eq!(embedded_cover(&path), None);
        let png = Cover {
            mime: "image/png",
            data: PNG.to_vec(),
        };
        rewrite_cover(&path, Some(&png)).unwrap();
        assert_eq!(embedded_cover(&path).unwrap().mime, "image/png");
        let jpeg = Cover {
            mime: "image/jpeg",
            data: vec![
                0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 1,
            ],
        };
        rewrite_cover(&path, Some(&jpeg)).unwrap();
        let found = embedded_cover(&path).unwrap();
        assert_eq!(
            found.mime, "image/jpeg",
            "the old picture is replaced, not kept beside"
        );
        assert_eq!(
            read_file(&path).unwrap().tags.title.as_deref(),
            Some("Night Drive"),
            "tags stay"
        );
        rewrite_cover(&path, None).unwrap();
        assert_eq!(embedded_cover(&path), None);
        assert_eq!(
            fs::read_dir(folder.path()).unwrap().count(),
            1,
            "no copy is left behind"
        );
    }

    #[test]
    fn a_file_that_cannot_be_retagged_is_left_exactly_as_it_was() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("broken.mp3");
        fs::write(&path, b"not audio").unwrap();
        assert!(rewrite_tags(&path, &[(TextTag::Title, "X".into())]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"not audio");
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[test]
    fn chosen_fields_are_replaced_and_the_rest_kept_as_id3v2_3() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.mp3");
        fs::write(&path, mp3(20)).unwrap();
        fixture_tags(
            &path,
            &TagInfo {
                artist: Some("Studio 001;".into()),
                title: Some("Night Drive".into()),
                genre: Some("House".into()),
                ..TagInfo::default()
            },
        );
        let cover = Cover {
            mime: "image/png",
            data: PNG.to_vec(),
        };
        write_tags(
            &path,
            &TagInfo {
                artist: Some("Studio 001".into()),
                track_number: Some(4),
                ..TagInfo::default()
            },
            Some(&cover),
        )
        .unwrap();
        let read = read_file(&path).unwrap().tags;
        assert_eq!(read.artist.as_deref(), Some("Studio 001"));
        assert_eq!(read.track_number, Some(4));
        assert_eq!(read.title.as_deref(), Some("Night Drive"), "untouched");
        assert_eq!(read.genre.as_deref(), Some("House"), "untouched");
        assert_eq!(&fs::read(&path).unwrap()[..4], b"ID3\x03", "ID3v2.3");
        assert_eq!(embedded_cover(&path).unwrap().data, PNG);
    }

    #[test]
    fn an_untagged_aiff_gets_tags_and_an_existing_cover_is_not_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.aif");
        fs::write(&path, aiff(1)).unwrap();
        let values = TagInfo {
            artist: Some("Studio 001".into()),
            title: Some("Night Drive".into()),
            album: Some("Between Spaces".into()),
            album_artist: Some("Studio 001".into()),
            label: Some("Test Label".into()),
            key: Some("8A".into()),
            bpm: Some(126.0),
            year: Some(2019),
            track_number: Some(1),
            disc_number: Some(1),
            genre: Some("House".into()),
        };
        let first = Cover {
            mime: "image/png",
            data: PNG.to_vec(),
        };
        write_tags(&path, &values, Some(&first)).unwrap();
        assert_eq!(read_file(&path).unwrap().tags, values);
        let bytes = fs::read(&path).unwrap();
        assert!(
            bytes.windows(4).any(|w| w == b"TYER"),
            "the year as ID3v2.3 writes it"
        );
        let mut other = PNG.to_vec();
        other.push(1);
        let second = Cover {
            mime: "image/png",
            data: other,
        };
        write_tags(&path, &TagInfo::default(), Some(&second)).unwrap();
        assert_eq!(embedded_cover(&path).unwrap().data, PNG, "kept the first");
    }

    /// A picture with no description, as many shops and taggers write it: every way openCrate
    /// writes tags keeps it readable (it once came out with a broken description).
    #[test]
    fn a_picture_with_no_description_survives_every_write() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.aif");
        let picture = PNG.to_vec();
        fs::write(&path, aiff(1)).unwrap();
        crate::fixtures::write_tags_and_cover(
            &path,
            &TagInfo {
                title: Some("Night Drive".into()),
                ..TagInfo::default()
            },
            Some(&picture),
        );
        assert_eq!(embedded_cover(&path).unwrap().data, picture);
        let readable = |path: &Path| {
            let file = std::fs::File::open(path).unwrap();
            Probe::new(std::io::BufReader::new(file))
                .options(ParseOptions::new().read_cover_art(true))
                .guess_file_type()
                .unwrap()
                .read()
                .map(|_| ())
                .map_err(|error| error.to_string())
        };
        write_tags(
            &path,
            &TagInfo {
                genre: Some("House".into()),
                ..TagInfo::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(readable(&path), Ok(()), "after writing tags");
        assert_eq!(embedded_cover(&path).unwrap().data, picture);
        rewrite_tags(&path, &[(TextTag::Genre, "Techno".into())]).unwrap();
        rewrite_tags(&path, &[(TextTag::Label, String::new())]).unwrap();
        assert_eq!(readable(&path), Ok(()), "after an edit and a cleared field");
        assert_eq!(embedded_cover(&path).unwrap().data, picture);
        assert_eq!(
            read_file(&path).unwrap().tags.genre.as_deref(),
            Some("Techno")
        );
    }

    /// A file already holding such a broken picture: tag edits refuse it and leave it as it was,
    /// and new album art replaces the picture, after which it edits like any other.
    #[test]
    fn a_damaged_picture_is_refused_by_edits_and_replaced_by_new_art() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.aif");
        fs::write(&path, aiff(1)).unwrap();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Night Drive".into());
        tag.push_picture(Picture::new_unchecked(
            PictureType::CoverFront,
            Some(MimeType::Png),
            None,
            PNG.to_vec(),
        ));
        // What lofty itself writes for a picture with no description in ID3v2.3.
        let mut options = WriteOptions::default();
        tag.save_to_path(&path, options.use_id3v23(true)).unwrap();
        assert_eq!(
            read_file(&path).unwrap().tags.title.as_deref(),
            Some("Night Drive")
        );
        assert!(embedded_cover(&path).is_none(), "the picture does not read");

        let before = fs::read(&path).unwrap();
        let error = rewrite_tags(&path, &[(TextTag::Genre, "House".into())]).unwrap_err();
        assert!(matches!(error, ReadError::DamagedPicture), "{error}");
        assert!(error.to_string().contains("picture is damaged"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), before, "left exactly as it was");

        let mut other = PNG.to_vec();
        other.push(7);
        rewrite_cover(
            &path,
            Some(&Cover {
                mime: "image/png",
                data: other.clone(),
            }),
        )
        .unwrap();
        assert_eq!(embedded_cover(&path).unwrap().data, other);
        assert_eq!(
            read_file(&path).unwrap().tags.title.as_deref(),
            Some("Night Drive")
        );
        rewrite_tags(&path, &[(TextTag::Genre, "House".into())]).unwrap();
        assert_eq!(
            read_file(&path).unwrap().tags.genre.as_deref(),
            Some("House")
        );
    }
}
