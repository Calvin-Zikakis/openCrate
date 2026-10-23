//! What a name in a download folder is, by name alone.
//!
//! Browsers and Soulseek clients write to a temporary name and rename when the download ends,
//! so a name that looks temporary means "not yet". The check is deliberately generous: leaving
//! a finished file for one more poll costs seconds, importing half a file costs a bad import.

use oc_paths::container_for;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// An audio file; the label is the container, as `oc_paths::container_for` names it.
    Audio(&'static str),
    /// A zip, rar or 7z, which is how Bandcamp delivers a purchase.
    Archive,
    /// Cover art that travels with a release.
    Artwork,
    /// A download that has not finished, or a folder holding one.
    Partial,
    /// Operating-system and client litter that never belongs to a release.
    Ignored,
    /// Anything else (a cue sheet, a PDF booklet); kept with the release, not imported.
    Other,
}

/// Extensions that mean "still downloading" or "scratch file", lower case.
const PARTIAL_EXTENSIONS: [&[u8]; 14] = [
    b"part",
    b"crdownload",
    b"download",
    b"tmp",
    b"temp",
    b"partial",
    b"incomplete",
    b"opdownload",
    b"filepart",
    b"!ut",
    b"!qb",
    b"aria2",
    b"unconfirmed",
    b"bc!",
];

/// Prefixes that mean the same. Soulseek clients keep unfinished downloads under `INCOMPLETE~`.
const PARTIAL_PREFIXES: [&[u8]; 2] = [b"incomplete~", b"incomplete_"];

const IGNORED_NAMES: [&[u8]; 4] = [b"thumbs.db", b"desktop.ini", b".ds_store", b"icon\r"];

fn extension(name: &[u8]) -> Option<Vec<u8>> {
    let dot = name.iter().rposition(|byte| *byte == b'.')?;
    // A leading dot alone is a hidden file, not an extension.
    if dot == 0 {
        return None;
    }
    Some(name[dot + 1..].to_ascii_lowercase())
}

/// Classify one path component (a file or a folder name) given as raw bytes.
#[must_use]
pub fn classify(name: &[u8]) -> EntryKind {
    let lower = name.to_ascii_lowercase();
    if lower.is_empty()
        || lower.starts_with(b".")
        || lower.starts_with(b"~$")
        || lower.ends_with(b"~")
        || IGNORED_NAMES.contains(&lower.as_slice())
    {
        // A trailing tilde is an editor backup; `INCOMPLETE~x` is caught below first.
        if PARTIAL_PREFIXES.iter().any(|p| lower.starts_with(p)) {
            return EntryKind::Partial;
        }
        return EntryKind::Ignored;
    }
    if PARTIAL_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return EntryKind::Partial;
    }
    let Some(extension) = extension(name) else {
        return EntryKind::Other;
    };
    if PARTIAL_EXTENSIONS.contains(&extension.as_slice()) {
        return EntryKind::Partial;
    }
    if let Some(container) = container_for(name) {
        return EntryKind::Audio(container);
    }
    match extension.as_slice() {
        b"zip" | b"rar" | b"7z" => EntryKind::Archive,
        b"jpg" | b"jpeg" | b"png" | b"webp" | b"gif" => EntryKind::Artwork,
        _ => EntryKind::Other,
    }
}

/// A path is unfinished if any component is: a folder named `x.download` (Safari) holds a
/// finished-looking file inside it.
#[must_use]
pub fn path_has_partial(components: &[Vec<u8>]) -> bool {
    components
        .iter()
        .any(|component| classify(component) == EntryKind::Partial)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(name: &str) -> EntryKind {
        classify(name.as_bytes())
    }

    #[test]
    fn finished_audio_is_audio_whatever_the_case() {
        assert_eq!(kind("01 Night Drive.AIF"), EntryKind::Audio("AIFF"));
        assert_eq!(kind("a.flac"), EntryKind::Audio("FLAC"));
        assert_eq!(kind("a.b.c.mp3"), EntryKind::Audio("MP3"));
        assert_eq!(kind("x.wav"), EntryKind::Audio("WAV"));
    }

    #[test]
    fn a_temporary_name_wins_over_the_audio_extension_inside_it() {
        for name in [
            "a.mp3.part",
            "a.flac.crdownload",
            "a.wav.PART",
            "a.mp3.!ut",
            "a.mp3.!qB",
            "a.mp3.aria2",
            "Unconfirmed 123.crdownload",
            "a.mp3.tmp",
            "INCOMPLETE~a.mp3",
            "incomplete_b.flac",
            "a.mp3.opdownload",
        ] {
            assert_eq!(kind(name), EntryKind::Partial, "{name}");
        }
    }

    #[test]
    fn archives_artwork_and_other_files_are_told_apart() {
        assert_eq!(kind("Between Spaces.zip"), EntryKind::Archive);
        assert_eq!(kind("pack.7Z"), EntryKind::Archive);
        assert_eq!(kind("cover.JPG"), EntryKind::Artwork);
        assert_eq!(kind("booklet.pdf"), EntryKind::Other);
        assert_eq!(kind("README"), EntryKind::Other);
        assert_eq!(kind("release.cue"), EntryKind::Other);
    }

    #[test]
    fn system_litter_is_ignored_and_never_counted() {
        for name in [
            ".DS_Store",
            "._01 Night Drive.mp3",
            "Thumbs.db",
            "desktop.ini",
            "~$notes.docx",
            "notes.txt~",
            "",
            ".hidden",
        ] {
            assert_eq!(kind(name), EntryKind::Ignored, "{name:?}");
        }
    }

    #[test]
    fn a_partial_prefix_is_not_mistaken_for_litter_by_its_tilde() {
        assert_eq!(kind("INCOMPLETE~x.mp3"), EntryKind::Partial);
        assert_eq!(kind("INCOMPLETE~"), EntryKind::Partial);
    }

    #[test]
    fn non_utf8_names_do_not_break_classification() {
        assert_eq!(classify(b"caf\xe9.mp3"), EntryKind::Audio("MP3"));
        assert_eq!(classify(b"caf\xe9.mp3.part"), EntryKind::Partial);
    }

    #[test]
    fn a_folder_that_is_still_downloading_taints_what_is_inside_it() {
        let finished = vec![b"Release".to_vec(), b"01.mp3".to_vec()];
        assert!(!path_has_partial(&finished));
        let inside_safari = vec![b"x.download".to_vec(), b"01.mp3".to_vec()];
        assert!(path_has_partial(&inside_safari));
    }
}
