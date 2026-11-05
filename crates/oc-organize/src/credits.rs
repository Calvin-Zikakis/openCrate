//! Who is credited on a track, from its artist field and title: the main artists, anyone
//! featured, and the remixer. Pure; the database decides which halves of an `&` count.

use unicode_normalization::UnicodeNormalization;

use crate::title::{featuring, parse_title, RemixKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Artist,
    Featured,
    Remixer(RemixKind),
}

impl Role {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Featured => "featured",
            Self::Remixer(_) => "remixer",
        }
    }
}

/// Whether a credit stands by itself or is part of an `&`-style split that the library
/// settles later: a `Half` counts only once that name is an artist on its own, and the
/// `Whole` text counts only while its halves do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    Sure,
    Half,
    Whole,
}

impl Standing {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sure => "sure",
            Self::Half => "half",
            Self::Whole => "whole",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credit {
    pub name: String,
    pub role: Role,
    pub standing: Standing,
    /// Which split a `Whole` and its `Half`s belong to, 1 upwards per track; 0 for `Sure`.
    pub group: u8,
}

/// Between two names in one field. `&` and `/` are taken as written; `and` and `x` in any
/// case. A comma is handled before these and always splits.
const SEPARATORS: [&str; 4] = [" & ", " and ", "/", " x "];

/// The credits on a track, in order: the main artists as the field lists them, then the
/// featured names (from a trailing `feat.` clause in the field and from the title), then the
/// remixer. A name list splits on commas; a piece with `&`, `and`, `/` or `x` in it gives the
/// whole text and each part, for the library to settle.
#[must_use]
pub fn credits(artist: &str, title: &str) -> Vec<Credit> {
    let mut found: Vec<Credit> = Vec::new();
    let mut group = 0u8;
    let (main, field_featured) = split_featuring(artist);
    push_names(&mut found, main, Role::Artist, &mut group);
    let parsed = parse_title(title);
    for text in field_featured.iter().chain(parsed.featured.iter()) {
        push_names(&mut found, text, Role::Featured, &mut group);
    }
    if let (Some(remixer), Some(kind)) = (parsed.remixer.as_deref(), parsed.kind) {
        if kind.moves_folder() {
            push_names(&mut found, remixer, Role::Remixer(kind), &mut group);
        }
    }
    found
}

/// Case-insensitive, Unicode-normalized, for telling one name from another.
fn fold(name: &str) -> String {
    name.trim().nfc().collect::<String>().to_lowercase()
}

fn starts_with_ignoring_ascii_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// The main-artist text and the text after a `feat.`-style lead, when the field has one
/// after a space and something before it. A field that is only a clause is left whole.
fn split_featuring(field: &str) -> (&str, Option<String>) {
    let mut previous_blank = false;
    for (index, c) in field.char_indices() {
        if previous_blank {
            if let Some(name) = featuring(&field[index..]) {
                let head = field[..index].trim_end();
                if !head.is_empty() {
                    return (head, Some(name));
                }
            }
        }
        previous_blank = c.is_whitespace();
    }
    (field, None)
}

/// A piece's parts around `&`, `and`, `/` and `x`, trimmed and non-empty; one part when none.
fn parts(piece: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut index) = (0, 0);
    while index < piece.len() {
        let rest = &piece[index..];
        if let Some(separator) = SEPARATORS
            .iter()
            .find(|separator| starts_with_ignoring_ascii_case(rest, separator))
        {
            parts.push(&piece[start..index]);
            index += separator.len();
            start = index;
        } else {
            index += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    parts.push(&piece[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn push_names(found: &mut Vec<Credit>, text: &str, role: Role, group: &mut u8) {
    for piece in text
        .split(',')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
    {
        let parts = parts(piece);
        if parts.len() > 1 {
            *group = group.saturating_add(1);
            push(found, piece, role, Standing::Whole, *group);
            for part in parts {
                push(found, part, role, Standing::Half, *group);
            }
        } else {
            push(found, piece, role, Standing::Sure, 0);
        }
    }
}

/// One credit per name and role; a featured name or remixer who is a main artist is not
/// credited again.
fn push(found: &mut Vec<Credit>, name: &str, role: Role, standing: Standing, group: u8) {
    let key = fold(name);
    if key.is_empty() {
        return;
    }
    let taken = found.iter().any(|credit| {
        fold(&credit.name) == key
            && (credit.role.name() == role.name()
                || (credit.role == Role::Artist && role != Role::Artist))
    });
    if taken {
        return;
    }
    // The text in the tags is the credit's name; a `/` piece is trimmed like the others.
    found.push(Credit {
        name: name.to_owned(),
        role,
        standing,
        group,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(artist: &str, title: &str) -> Vec<(String, &'static str, &'static str, u8)> {
        credits(artist, title)
            .into_iter()
            .map(|c| (c.name, c.role.name(), c.standing.name(), c.group))
            .collect()
    }

    #[test]
    fn a_comma_always_splits() {
        assert_eq!(
            names("Studio 001, Night Drive", "Set"),
            vec![
                ("Studio 001".into(), "artist", "sure", 0),
                ("Night Drive".into(), "artist", "sure", 0)
            ]
        );
    }

    #[test]
    fn an_ampersand_gives_a_whole_and_its_halves() {
        assert_eq!(
            names("Studio 001 & Night Drive", "Set"),
            vec![
                ("Studio 001 & Night Drive".into(), "artist", "whole", 1),
                ("Studio 001".into(), "artist", "half", 1),
                ("Night Drive".into(), "artist", "half", 1)
            ]
        );
        // " and ", "/" and " x " split the same way; "/" needs no spaces.
        assert_eq!(names("Studio 001/Night Drive", "Set").len(), 3);
        assert_eq!(names("Studio 001 x Night Drive", "Set").len(), 3);
        assert_eq!(names("Studio 001 and Night Drive", "Set").len(), 3);
        // Two splits on one track get their own groups.
        let two = names("Studio 001 & Night Drive, Guest / Second Guest", "Set");
        assert_eq!(
            two.iter().map(|c| c.3).collect::<Vec<_>>(),
            vec![1, 1, 1, 2, 2, 2]
        );
    }

    #[test]
    fn a_featuring_clause_in_the_artist_field_credits_a_guest() {
        assert_eq!(
            names("Studio 001 feat. Guest", "Set"),
            vec![
                ("Studio 001".into(), "artist", "sure", 0),
                ("Guest".into(), "featured", "sure", 0)
            ]
        );
        assert_eq!(names("Studio 001 ft Guest, Second Guest", "Set").len(), 3);
        assert_eq!(names("Studio 001 Featuring Guest", "Set")[1].0, "Guest");
    }

    #[test]
    fn a_featuring_group_in_the_title_credits_a_guest_once() {
        assert_eq!(
            names("Studio 001", "Set (feat. Guest)"),
            vec![
                ("Studio 001".into(), "artist", "sure", 0),
                ("Guest".into(), "featured", "sure", 0)
            ]
        );
        assert_eq!(
            names("Studio 001 feat. Guest", "Set (feat. Guest)").len(),
            2
        );
        assert_eq!(
            names("Studio 001", "Set (feat. Guest, Second Guest)").len(),
            3
        );
    }

    #[test]
    fn a_remix_credits_the_remixer_with_its_kind_but_a_vip_does_not() {
        let c = credits("Studio 001", "Set (Night Drive Remix)");
        assert_eq!(c[1].name, "Night Drive");
        assert!(matches!(c[1].role, Role::Remixer(RemixKind::Remix)));
        assert_eq!(
            names("Studio 001", "Set (Night Drive Re-Edit)")[1].1,
            "remixer"
        );
        assert_eq!(names("Studio 001", "Set (Studio 001 VIP)").len(), 1);
        assert_eq!(names("Studio 001", "Set (Extended Mix)").len(), 1);
    }

    #[test]
    fn a_name_already_a_main_artist_is_not_credited_again() {
        assert_eq!(names("Studio 001", "Set (Studio 001 Remix)").len(), 1);
        assert_eq!(names("Studio 001", "Set (feat. studio 001)").len(), 1);
        assert_eq!(names("Studio 001, Studio 001", "Set").len(), 1);
    }

    #[test]
    fn a_field_that_is_only_a_featuring_clause_is_the_main_artist() {
        assert_eq!(
            names("feat. Guest", "Set"),
            vec![("feat. Guest".into(), "artist", "sure", 0)]
        );
    }

    #[test]
    fn one_name_can_hold_two_roles() {
        assert_eq!(
            names("Studio 001", "Set (feat. Guest) (Guest Remix)").len(),
            3
        );
    }

    #[test]
    fn blank_pieces_are_dropped_and_an_empty_field_credits_nobody() {
        assert_eq!(names("Studio 001, , Night Drive", "Set").len(), 2);
        assert_eq!(credits("", "Set"), Vec::new());
    }

    #[test]
    fn kind_words_are_lower_case() {
        assert_eq!(RemixKind::ReEdit.word(), "re-edit");
        assert_eq!(RemixKind::Vip.word(), "vip");
    }
}
