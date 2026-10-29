//! A fallback for files whose tags are missing: read the library's own layout.
//! `Artist/Release/NN - Artist - Title.ext` is how the owner's tidier files look
//! This only reads names; it never touches the file.

use crate::TagInfo;

/// Text of a path component, lossy; invalid bytes become the replacement character.
fn name(component: &[u8]) -> String {
    String::from_utf8_lossy(component).trim().to_owned()
}

fn stem(file_name: &str) -> &str {
    match file_name.rfind('.') {
        Some(dot) if dot > 0 => &file_name[..dot],
        _ => file_name,
    }
}

/// A leading "03 " that is a track number rather than part of a name: two digits, then a
/// space, and no " - " after it (so "50 Cent - Title" is an artist). Above 40 only a zero-padded
/// number counts, so "99 Problems" stays a title.
fn bare_number(stem: &str) -> Option<(u32, &str)> {
    let digits = stem.get(..2)?;
    let rest = stem.get(2..)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) || !rest.starts_with(' ') {
        return None;
    }
    let rest = rest.trim_start();
    let number: u32 = digits.parse().ok()?;
    let plausible = digits.starts_with('0') || number <= 40;
    (plausible && number > 0 && !rest.is_empty() && !rest.contains(" - ")).then_some((number, rest))
}

/// A web shop's `Artist - Album - 03 Title`.
fn artist_album_number_title(stem: &str, info: &mut TagInfo) -> bool {
    let Some((head, last)) = stem.rsplit_once(" - ") else {
        return false;
    };
    let Some((artist, album)) = head.split_once(" - ") else {
        return false;
    };
    let Some((number, title)) = bare_number(last) else {
        return false;
    };
    if artist.trim().is_empty() || album.trim().is_empty() {
        return false;
    }
    info.artist = Some(artist.trim().to_owned());
    info.album = Some(album.trim().to_owned());
    info.track_number = Some(number);
    info.title = Some(title.trim().to_owned());
    true
}

/// `01 - Artist - Title`, `1. Title`, `03 Title`, `Artist - Album - 03 Title` (web shops),
/// `Artist - Title` or just `Title`.
fn from_file_stem(stem: &str, info: &mut TagInfo) {
    if artist_album_number_title(stem, info) {
        return;
    }
    let digits = stem.chars().take_while(char::is_ascii_digit).count();
    let (number, rest) = if (1..=3).contains(&digits) {
        let after = stem[digits..].trim_start();
        match after.strip_prefix(['-', '.', ')']) {
            Some(rest) => (stem[..digits].parse::<u32>().ok(), rest.trim_start()),
            None => match bare_number(stem) {
                Some((number, rest)) => (Some(number), rest),
                None => (None, stem),
            },
        }
    } else {
        (None, stem)
    };
    info.track_number = number;
    match rest.split_once(" - ") {
        Some((artist, title)) if !artist.trim().is_empty() && !title.trim().is_empty() => {
            info.artist = Some(artist.trim().to_owned());
            info.title = Some(title.trim().to_owned());
        }
        _ if !rest.trim().is_empty() => info.title = Some(rest.trim().to_owned()),
        _ => info.title = Some(stem.to_owned()),
    }
}

/// Guess from a root-relative path, as components. With three or more, the last three are
/// artist folder, release folder and file name; with two, a release folder and file name.
/// Empty fields stay `None`. A guess is a fallback, never a replacement for a tag.
#[must_use]
pub fn guess_from_path(components: &[&[u8]]) -> TagInfo {
    let mut info = TagInfo::default();
    let Some((file, folders)) = components.split_last() else {
        return info;
    };
    from_file_stem(stem(&name(file)), &mut info);
    match folders {
        [.., artist, release] => {
            info.album_artist = Some(name(artist)).filter(|text| !text.is_empty());
            if info.album.is_none() {
                info.album = Some(name(release)).filter(|text| !text.is_empty());
            }
            if info.artist.is_none() {
                info.artist.clone_from(&info.album_artist);
            }
        }
        [release] if info.album.is_none() => {
            info.album = Some(name(release)).filter(|text| !text.is_empty());
        }
        [_] | [] => {}
    }
    info
}
