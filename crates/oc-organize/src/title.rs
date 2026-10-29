//! Reading a title's version and credits: `Night Drive (Studio 001 Remix) (feat. Guest)`.
//!
//! The version is part of a track's identity: no version means "Original Mix", and
//! "Original Mix" and "Extended Mix" stay different. A remix credit is parsed into the
//! remixer, and that field decides the folder. Which kinds of credit move a track to
//! the remixer's folder is a policy table below.

use std::fmt::Write as _;

use unicode_normalization::UnicodeNormalization;

pub const ORIGINAL_MIX: &str = "Original Mix";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemixKind {
    Remix,
    Rework,
    Bootleg,
    Flip,
    Dub,
    Edit,
    ReEdit,
    Vip,
}

impl RemixKind {
    /// Suffix words, longest first, with the kind each means.
    const SUFFIXES: [(&'static str, Self); 9] = [
        ("re-edit", Self::ReEdit),
        ("remix", Self::Remix),
        ("rework", Self::Rework),
        ("bootleg", Self::Bootleg),
        ("flip", Self::Flip),
        ("edit", Self::Edit),
        ("dub", Self::Dub),
        ("vip", Self::Vip),
        ("reedit", Self::ReEdit),
    ];

    /// A credited remix or edit goes in that artist's folder, so their work is found under
    /// their name (owner, 2026-10-23); a VIP is the original artist's own version and stays.
    #[must_use]
    pub const fn moves_folder(self) -> bool {
        !matches!(self, Self::Vip)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedTitle {
    /// The title without its version and featuring groups.
    pub title: String,
    /// As written, for example `Studio 001 Remix` or `Extended Mix`.
    pub version: Option<String>,
    /// The name in a `<name> <kind>` version.
    pub remixer: Option<String>,
    pub kind: Option<RemixKind>,
    pub featured: Option<String>,
}

/// Versions that describe a mix and credit nobody.
const GENERIC: [&str; 18] = [
    "original mix",
    "original version",
    "extended mix",
    "extended version",
    "extended",
    "radio edit",
    "radio mix",
    "radio version",
    "club mix",
    "club edit",
    "club version",
    "album version",
    "album mix",
    "dub mix",
    "instrumental",
    "acapella",
    "a cappella",
    "vocal mix",
];

/// Words that describe a cut rather than name an artist, so "Short Edit" or "Extended Club
/// Dub" credits nobody.
const DESCRIPTIVE: [&str; 21] = [
    "original",
    "extended",
    "radio",
    "club",
    "album",
    "single",
    "short",
    "long",
    "clean",
    "dirty",
    "explicit",
    "vocal",
    "instrumental",
    "main",
    "intro",
    "outro",
    "dj",
    "mix",
    "version",
    "special",
    "alternate",
];

fn descriptive(name: &str) -> bool {
    name.split_whitespace()
        .all(|word| DESCRIPTIVE.contains(&word.to_lowercase().as_str()))
}

/// The text of a trailing `(...)` or `[...]` group and where it starts.
fn trailing_group(text: &str) -> Option<(usize, &str)> {
    let close = text.chars().last()?;
    let open = match close {
        ')' => '(',
        ']' => '[',
        _ => return None,
    };
    let mut depth = 0usize;
    for (index, c) in text.char_indices().rev() {
        if c == close {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                let inner = &text[index + c.len_utf8()..text.len() - close.len_utf8()];
                return Some((index, inner));
            }
        }
    }
    None
}

fn featuring(group: &str) -> Option<String> {
    let lower = group.to_lowercase();
    for lead in ["feat.", "feat ", "ft.", "ft ", "featuring "] {
        if lower.starts_with(lead) {
            let rest = group[lead.len()..].trim();
            return (!rest.is_empty()).then(|| rest.to_owned());
        }
    }
    None
}

struct Version {
    text: String,
    remixer: Option<String>,
    kind: Option<RemixKind>,
}

fn version(group: &str) -> Option<Version> {
    let text = group.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = text.to_lowercase();
    if lower.is_empty() {
        return None;
    }
    if GENERIC.contains(&lower.as_str()) {
        return Some(Version {
            text,
            remixer: None,
            kind: None,
        });
    }
    for (suffix, kind) in RemixKind::SUFFIXES {
        let Some(head) = lower.strip_suffix(suffix) else {
            continue;
        };
        if !head.is_empty() && !head.ends_with(char::is_whitespace) {
            continue;
        }
        let name = text[..head.len()].trim().to_owned();
        if !name.is_empty() && descriptive(&name) {
            return Some(Version {
                text,
                remixer: None,
                kind: None,
            });
        }
        return Some(Version {
            remixer: (!name.is_empty()).then_some(name),
            kind: Some(kind),
            text,
        });
    }
    None
}

#[must_use]
pub fn parse_title(raw: &str) -> ParsedTitle {
    let mut rest = raw.trim();
    let mut versions: Vec<Version> = Vec::new();
    let mut featured = None;
    while let Some((start, group)) = trailing_group(rest) {
        let head = rest[..start].trim_end();
        if head.is_empty() {
            break;
        }
        if let Some(name) = featuring(group) {
            featured.get_or_insert(name);
        } else if let Some(found) = version(group) {
            versions.push(found);
        } else {
            break;
        }
        rest = head;
    }
    versions.reverse();
    // The first version that names a kind gives the credit; a bare `(Remix)` has a kind and no name.
    let remix = versions.iter().find(|found| found.kind.is_some());
    ParsedTitle {
        title: rest.to_owned(),
        remixer: remix.and_then(|found| found.remixer.clone()),
        kind: remix.and_then(|found| found.kind),
        version: (!versions.is_empty()).then(|| {
            versions
                .iter()
                .map(|found| found.text.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        }),
        featured,
    }
}

/// The title as it is filed: the title, then `(feat. X)`, then `(version)`.
#[must_use]
pub fn full_title(parsed: &ParsedTitle) -> String {
    let mut text = parsed.title.clone();
    if let Some(name) = &parsed.featured {
        let _ = write!(text, " (feat. {name})");
    }
    if let Some(version) = &parsed.version {
        let _ = write!(text, " ({version})");
    }
    text
}

/// Compare versions without caring about case, spacing or Unicode form; no version is
/// "Original Mix".
#[must_use]
pub fn version_key(version: Option<&str>) -> String {
    let text = version
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or(ORIGINAL_MIX);
    text.nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[must_use]
pub fn same_version(left: Option<&str>, right: Option<&str>) -> bool {
    version_key(left) == version_key(right)
}

/// The artist folder: the remixer when the credit moves the folder, else the album
/// artist, else the artist. A compilation's "Various Artists" counts as no album artist, so
/// each track is filed under the act that made it.
#[must_use]
pub fn folder_artist(parsed: &ParsedTitle, album_artist: Option<&str>, artist: &str) -> String {
    let remixer = parsed
        .remixer
        .as_deref()
        .filter(|_| parsed.kind.is_some_and(RemixKind::moves_folder));
    remixer
        .or_else(|| {
            album_artist
                .map(str::trim)
                .filter(|name| !name.is_empty() && !is_various(name))
        })
        .unwrap_or(artist)
        .to_owned()
}

/// The placeholder album artists stores and taggers put on compilations.
fn is_various(name: &str) -> bool {
    const PLACEHOLDERS: [&str; 6] = [
        "various artists",
        "various artist",
        "various",
        "va",
        "v.a.",
        "v/a",
    ];
    PLACEHOLDERS.contains(&name.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::RemixKind::{Bootleg, Dub, Edit, Flip, ReEdit, Remix, Rework, Vip};
    use super::*;

    struct Case {
        input: &'static str,
        title: &'static str,
        version: Option<&'static str>,
        remixer: Option<&'static str>,
        kind: Option<RemixKind>,
        featured: Option<&'static str>,
    }

    const fn case(
        input: &'static str,
        title: &'static str,
        version: Option<&'static str>,
        remixer: Option<&'static str>,
        kind: Option<RemixKind>,
        featured: Option<&'static str>,
    ) -> Case {
        Case {
            input,
            title,
            version,
            remixer,
            kind,
            featured,
        }
    }

    /// Synthetic titles only: the corpus the plan review asked for.
    const CORPUS: &[Case] = &[
        case("Night Drive", "Night Drive", None, None, None, None),
        case(
            "Night Drive (Original Mix)",
            "Night Drive",
            Some("Original Mix"),
            None,
            None,
            None,
        ),
        case(
            "Night Drive (Extended Mix)",
            "Night Drive",
            Some("Extended Mix"),
            None,
            None,
            None,
        ),
        case(
            "Night Drive (Radio Edit)",
            "Night Drive",
            Some("Radio Edit"),
            None,
            None,
            None,
        ),
        case(
            "Night Drive (Studio 001 Remix)",
            "Night Drive",
            Some("Studio 001 Remix"),
            Some("Studio 001"),
            Some(Remix),
            None,
        ),
        case(
            "Night Drive [Studio 001 Remix]",
            "Night Drive",
            Some("Studio 001 Remix"),
            Some("Studio 001"),
            Some(Remix),
            None,
        ),
        case(
            "Night Drive (X & Y Remix)",
            "Night Drive",
            Some("X & Y Remix"),
            Some("X & Y"),
            Some(Remix),
            None,
        ),
        case(
            "Night Drive (studio 001 REMIX)",
            "Night Drive",
            Some("studio 001 REMIX"),
            Some("studio 001"),
            Some(Remix),
            None,
        ),
        case(
            "Night Drive (Studio 001 Rework)",
            "Night Drive",
            Some("Studio 001 Rework"),
            Some("Studio 001"),
            Some(Rework),
            None,
        ),
        case(
            "Night Drive (Studio 001 Bootleg)",
            "Night Drive",
            Some("Studio 001 Bootleg"),
            Some("Studio 001"),
            Some(Bootleg),
            None,
        ),
        case(
            "Night Drive (Studio 001 Flip)",
            "Night Drive",
            Some("Studio 001 Flip"),
            Some("Studio 001"),
            Some(Flip),
            None,
        ),
        case(
            "Night Drive (Studio 001 Dub)",
            "Night Drive",
            Some("Studio 001 Dub"),
            Some("Studio 001"),
            Some(Dub),
            None,
        ),
        case(
            "Night Drive (Studio 001 Edit)",
            "Night Drive",
            Some("Studio 001 Edit"),
            Some("Studio 001"),
            Some(Edit),
            None,
        ),
        case(
            "Night Drive (Studio 001 Re-Edit)",
            "Night Drive",
            Some("Studio 001 Re-Edit"),
            Some("Studio 001"),
            Some(ReEdit),
            None,
        ),
        case(
            "Night Drive (Studio 001 VIP)",
            "Night Drive",
            Some("Studio 001 VIP"),
            Some("Studio 001"),
            Some(Vip),
            None,
        ),
        case(
            "Night Drive (Dub Mix)",
            "Night Drive",
            Some("Dub Mix"),
            None,
            None,
            None,
        ),
        case(
            "Night Drive (feat. Guest)",
            "Night Drive",
            None,
            None,
            None,
            Some("Guest"),
        ),
        case(
            "Night Drive (ft. A & B)",
            "Night Drive",
            None,
            None,
            None,
            Some("A & B"),
        ),
        case(
            "Night Drive (feat. Guest) (Studio 001 Remix)",
            "Night Drive",
            Some("Studio 001 Remix"),
            Some("Studio 001"),
            Some(Remix),
            Some("Guest"),
        ),
        case(
            "Night Drive (Studio 001 Remix) (feat. Guest)",
            "Night Drive",
            Some("Studio 001 Remix"),
            Some("Studio 001"),
            Some(Remix),
            Some("Guest"),
        ),
        case(
            "Song (Part 1) (X Remix)",
            "Song (Part 1)",
            Some("X Remix"),
            Some("X"),
            Some(Remix),
            None,
        ),
        case(
            "Night Drive (Live at Somewhere)",
            "Night Drive (Live at Somewhere)",
            None,
            None,
            None,
            None,
        ),
        case(
            "Night Drive (Studio 001 Remix",
            "Night Drive (Studio 001 Remix",
            None,
            None,
            None,
            None,
        ),
        case(
            "Night Drive Studio 001 Remix)",
            "Night Drive Studio 001 Remix)",
            None,
            None,
            None,
            None,
        ),
        case(
            "(Studio 001 Remix)",
            "(Studio 001 Remix)",
            None,
            None,
            None,
            None,
        ),
        case(
            "Night Drive (Remix)",
            "Night Drive",
            Some("Remix"),
            None,
            Some(Remix),
            None,
        ),
        case(
            "Caf\u{e9} (Zo\u{eb} Remix)",
            "Caf\u{e9}",
            Some("Zo\u{eb} Remix"),
            Some("Zo\u{eb}"),
            Some(Remix),
            None,
        ),
        case(
            "Night Drive  (  Studio 001   Remix  )",
            "Night Drive",
            Some("Studio 001 Remix"),
            Some("Studio 001"),
            Some(Remix),
            None,
        ),
        case(
            "Remixer (Original Mix)",
            "Remixer",
            Some("Original Mix"),
            None,
            None,
            None,
        ),
        case(
            "Vipers (Original Mix)",
            "Vipers",
            Some("Original Mix"),
            None,
            None,
            None,
        ),
        case(
            "Night Drive (Studio 001 Remixes)",
            "Night Drive (Studio 001 Remixes)",
            None,
            None,
            None,
            None,
        ),
    ];

    #[test]
    fn the_corpus_parses_as_written() {
        for c in CORPUS {
            let parsed = parse_title(c.input);
            assert_eq!(parsed.title, c.title, "{:?}", c.input);
            assert_eq!(parsed.version.as_deref(), c.version, "{:?}", c.input);
            assert_eq!(parsed.remixer.as_deref(), c.remixer, "{:?}", c.input);
            assert_eq!(parsed.kind, c.kind, "{:?}", c.input);
            assert_eq!(parsed.featured.as_deref(), c.featured, "{:?}", c.input);
        }
    }

    #[test]
    fn parsing_a_filed_title_gives_the_same_result() {
        for c in CORPUS {
            let parsed = parse_title(c.input);
            let filed = full_title(&parsed);
            assert_eq!(parse_title(&filed), parsed, "{:?} -> {filed:?}", c.input);
        }
        assert_eq!(
            full_title(&parse_title("Night Drive (Studio 001 Remix) (feat. Guest)")),
            "Night Drive (feat. Guest) (Studio 001 Remix)"
        );
    }

    #[test]
    fn no_version_is_original_mix_and_other_versions_stay_different() {
        assert!(same_version(None, Some("Original Mix")));
        assert!(same_version(Some(""), Some("  original   MIX ")));
        assert!(same_version(Some("Cafe\u{301} Mix"), Some("Caf\u{e9} mix")));
        assert!(!same_version(Some("Original Mix"), Some("Extended Mix")));
        assert!(!same_version(None, Some("Extended Mix")));
        assert!(!same_version(Some("X Remix"), Some("Y Remix")));
    }

    #[test]
    fn a_compilation_track_goes_to_its_own_artist() {
        let plain = parse_title("Night Drive (Original Mix)");
        for various in [
            "Various Artists",
            "various artists",
            "Various",
            "VA",
            "V.A.",
            "V/A",
            " Various Artist ",
        ] {
            assert_eq!(
                folder_artist(&plain, Some(various), "Studio 001"),
                "Studio 001",
                "{various}"
            );
        }
        assert_eq!(
            folder_artist(&plain, Some("Various Production"), "Studio 001"),
            "Various Production",
            "an act whose name only starts with Various is a real album artist"
        );
    }

    #[test]
    fn the_folder_follows_the_credit_the_album_artist_then_the_artist() {
        let remix = parse_title("Night Drive (Studio 001 Remix)");
        assert_eq!(
            folder_artist(&remix, Some("Various"), "Original Act"),
            "Studio 001"
        );
        let edit = parse_title("Night Drive (Studio 001 Edit)");
        assert_eq!(
            folder_artist(&edit, None, "Original Act"),
            "Studio 001",
            "a credited edit goes to the editor"
        );
        for generic in [
            "Night Drive (Short Edit)",
            "Night Drive (Extended Club Edit)",
            "Night Drive (Clean Radio Edit)",
            "Night Drive (Club Dub)",
        ] {
            let parsed = parse_title(generic);
            assert_eq!(parsed.remixer, None, "{generic}");
            assert_eq!(
                folder_artist(&parsed, Some("Album Artist"), "Original Act"),
                "Album Artist",
                "{generic}"
            );
        }
        let plain = parse_title("Night Drive");
        assert_eq!(
            folder_artist(&plain, Some("  "), "Original Act"),
            "Original Act"
        );
        assert_eq!(
            folder_artist(&plain, Some("Album Artist"), "Original Act"),
            "Album Artist"
        );
        let vip = parse_title("Night Drive (Studio 001 VIP)");
        assert_eq!(folder_artist(&vip, None, "Original Act"), "Original Act");
        for kind in [
            RemixKind::Remix,
            RemixKind::Rework,
            RemixKind::Bootleg,
            RemixKind::Flip,
            RemixKind::Dub,
            RemixKind::Edit,
            RemixKind::ReEdit,
        ] {
            assert!(kind.moves_folder(), "{kind:?}");
        }
        assert!(!RemixKind::Vip.moves_folder());
    }

    #[test]
    fn odd_input_never_panics_and_keeps_text() {
        for raw in [
            "",
            " ",
            "(",
            ")",
            "()",
            "(())",
            "((a)",
            "[(]",
            "a)(",
            "\u{1F3B5} (\u{1F3B5} Remix)",
            "x (feat.)",
            "x (feat. )",
        ] {
            let parsed = parse_title(raw);
            assert!(raw.trim().is_empty() || !parsed.title.is_empty(), "{raw:?}");
        }
    }
}
