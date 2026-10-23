//! Cover art for display. Read only. A cover is the front picture embedded in an audio file, or
//! failing that an image such as `cover.jpg` beside it. Only JPEG, PNG, WebP and GIF are
//! returned, and nothing above [`MAX_COVER_BYTES`], so the window never has to decode something
//! unexpected or huge.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::PictureType;
use lofty::probe::Probe;

/// Larger pictures are ignored.
pub const MAX_COVER_BYTES: usize = 12 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cover {
    pub mime: &'static str,
    pub data: Vec<u8>,
}

/// The image type from its first bytes, never from a name or a claimed type.
fn sniff(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else {
        None
    }
}

fn cover_from(data: Vec<u8>) -> Option<Cover> {
    if data.is_empty() || data.len() > MAX_COVER_BYTES {
        return None;
    }
    Some(Cover {
        mime: sniff(&data)?,
        data,
    })
}

/// The front cover embedded in an audio file, or the first picture if none is marked front.
#[must_use]
pub fn embedded_cover(path: &Path) -> Option<Cover> {
    let file = File::open(path).ok()?;
    let tagged = Probe::new(BufReader::new(file))
        .options(ParseOptions::new().read_cover_art(true))
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;
    let mut first = None;
    for tag in tagged.tags() {
        for picture in tag.pictures() {
            if picture.pic_type() == PictureType::CoverFront {
                return cover_from(picture.data().to_vec());
            }
            first.get_or_insert(picture);
        }
    }
    first.and_then(|picture| cover_from(picture.data().to_vec()))
}

/// Names that mean "the cover", best first.
const FOLDER_NAMES: [&str; 5] = ["cover", "folder", "front", "artwork", "album"];
const FOLDER_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// A cover image sitting in `directory`. Links are not followed.
#[must_use]
pub fn folder_cover(directory: &Path) -> Option<Cover> {
    let mut best: Option<(usize, std::path::PathBuf)> = None;
    for entry in fs::read_dir(directory).ok()?.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        let Some((stem, extension)) = name.rsplit_once('.') else {
            continue;
        };
        let Some(rank) = FOLDER_NAMES.iter().position(|candidate| *candidate == stem) else {
            continue;
        };
        if FOLDER_EXTENSIONS.contains(&extension) && best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, entry.path()));
        }
    }
    let (_, path) = best?;
    let file = File::open(path).ok()?;
    let mut data = Vec::new();
    file.take(MAX_COVER_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .ok()?;
    cover_from(data)
}

#[cfg(test)]
mod tests {
    use lofty::picture::{MimeType, Picture};
    use lofty::tag::{Tag, TagExt, TagType};

    use super::*;
    use crate::fixtures::aiff;

    const JPEG: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0, 1,
    ];
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    fn embed(path: &Path, pictures: Vec<(PictureType, &[u8])>) {
        fs::write(path, aiff(1)).unwrap();
        let mut tag = Tag::new(TagType::Id3v2);
        for (kind, data) in pictures {
            tag.push_picture(Picture::new_unchecked(
                kind,
                Some(MimeType::Jpeg),
                None,
                data.to_vec(),
            ));
        }
        tag.save_to_path(path, lofty::config::WriteOptions::default())
            .unwrap();
    }

    #[test]
    fn the_front_cover_is_read_from_the_file_and_typed_by_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.aif");
        // The claimed type is JPEG for both; the bytes decide.
        embed(
            &path,
            vec![(PictureType::Artist, JPEG), (PictureType::CoverFront, PNG)],
        );
        let cover = embedded_cover(&path).unwrap();
        assert_eq!(cover.mime, "image/png");
        assert_eq!(cover.data, PNG);
    }

    #[test]
    fn without_a_front_cover_the_first_picture_is_used_and_no_picture_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.aif");
        embed(&path, vec![(PictureType::Artist, JPEG)]);
        assert_eq!(embedded_cover(&path).unwrap().mime, "image/jpeg");
        let bare = dir.path().join("b.aif");
        fs::write(&bare, aiff(1)).unwrap();
        assert_eq!(embedded_cover(&bare), None);
        assert_eq!(embedded_cover(&dir.path().join("missing.aif")), None);
    }

    #[test]
    fn something_that_is_not_an_image_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.aif");
        embed(
            &path,
            vec![(PictureType::CoverFront, b"<svg onload=alert(1)>")],
        );
        assert_eq!(embedded_cover(&path), None);
    }

    #[test]
    fn a_folder_image_is_found_by_name_best_first_and_by_content() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Artwork.png"), PNG).unwrap();
        fs::write(dir.path().join("Folder.JPG"), JPEG).unwrap();
        fs::write(dir.path().join("notes.jpg"), JPEG).unwrap();
        // "cover" beats "folder", "folder" beats "artwork".
        assert_eq!(folder_cover(dir.path()).unwrap().data, JPEG);
        fs::write(dir.path().join("cover.png"), PNG).unwrap();
        assert_eq!(folder_cover(dir.path()).unwrap().mime, "image/png");
        // A text file with a picture name is not a picture.
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("cover.jpg"), b"not an image").unwrap();
        assert_eq!(folder_cover(other.path()), None);
        assert_eq!(folder_cover(&other.path().join("gone")), None);
    }

    #[test]
    fn an_oversized_picture_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut big = JPEG.to_vec();
        big.resize(MAX_COVER_BYTES + 1, 0);
        fs::write(dir.path().join("cover.jpg"), &big).unwrap();
        assert_eq!(folder_cover(dir.path()), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_cover_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.jpg"), JPEG).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.jpg"),
            dir.path().join("cover.jpg"),
        )
        .unwrap();
        assert_eq!(folder_cover(dir.path()), None);
    }
}
