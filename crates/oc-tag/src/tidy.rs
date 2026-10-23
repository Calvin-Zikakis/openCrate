//! Cleaning up names before they become folders, file names and tags: stray separators left by
//! other taggers ("Artist;", "A;B"), padding and doubled spaces. Pure.

use crate::TagInfo;

/// Characters that end a name only by accident, such as an empty second artist after ";".
/// "+", "&" and "_" are left alone: "C++" and "+44" are names.
const TRAILING: &[char] = &[';', ',', '/', '\\', '|', '-'];
/// Only list separators are stripped from the start.
const LEADING: &[char] = &[';', ','];

/// Trim, collapse runs of white space, and strip separators left at the ends.
fn tidy_text(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_end_matches(|c: char| c.is_whitespace() || TRAILING.contains(&c))
        .trim_start_matches(|c: char| c.is_whitespace() || LEADING.contains(&c))
        .to_owned()
}

/// Artists separated with ";" (the usual stand-in for several values in ID3v2.3) become one
/// ", "-separated list, with empty and repeated names dropped. "/" and "&" stay, as in "AC/DC".
fn tidy_names(value: &str) -> String {
    let mut found: Vec<String> = Vec::new();
    for name in value.split(';').flat_map(|part| part.split(", ")) {
        let name = tidy_text(name);
        if !name.is_empty() && !found.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
            found.push(name);
        }
    }
    found.join(", ")
}

/// One name as "Tidy names" would leave it: list separators made ", ", stray ones removed.
/// Used to point out folder names that were made from untidy tags.
#[must_use]
pub fn tidy_name(value: &str) -> String {
    tidy_names(value)
}

fn apply(
    field: &mut Option<String>,
    tidy: fn(&str) -> String,
    changed: &mut Vec<&'static str>,
    name: &'static str,
) {
    if let Some(value) = field {
        let clean = tidy(value);
        if clean != *value {
            changed.push(name);
            *field = (!clean.is_empty()).then_some(clean);
        }
    }
}

/// Tidy every text field in place. Returns the names of the fields that changed, as the
/// preview labels them.
pub fn tidy(tags: &mut TagInfo) -> Vec<&'static str> {
    let mut changed = Vec::new();
    apply(&mut tags.artist, tidy_names, &mut changed, "Artist");
    apply(&mut tags.title, tidy_text, &mut changed, "Title");
    apply(&mut tags.album, tidy_text, &mut changed, "Album");
    apply(
        &mut tags.album_artist,
        tidy_names,
        &mut changed,
        "Album artist",
    );
    apply(&mut tags.label, tidy_text, &mut changed, "Label");
    apply(&mut tags.genre, tidy_text, &mut changed, "Genre");
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artist(value: &str) -> Option<String> {
        let mut tags = TagInfo {
            artist: Some(value.into()),
            ..TagInfo::default()
        };
        tidy(&mut tags);
        tags.artist
    }

    #[test]
    fn separators_left_by_other_taggers_are_cleaned_up() {
        assert_eq!(artist("Studio 001;").as_deref(), Some("Studio 001"));
        assert_eq!(
            artist("Studio 001;Guest Vocalist").as_deref(),
            Some("Studio 001, Guest Vocalist")
        );
        assert_eq!(
            artist(" Studio 001 ; studio 001;; Guest  Vocalist ;").as_deref(),
            Some("Studio 001, Guest Vocalist")
        );
        assert_eq!(artist("AC/DC").as_deref(), Some("AC/DC"));
        assert_eq!(
            artist("Simon & Garfunkel").as_deref(),
            Some("Simon & Garfunkel")
        );
        assert_eq!(artist("+44").as_deref(), Some("+44"));
        assert_eq!(artist("C++").as_deref(), Some("C++"));
        assert_eq!(artist("-Ziq").as_deref(), Some("-Ziq"));
        assert_eq!(artist(";").as_deref(), None, "nothing left means no value");
    }

    #[test]
    fn titles_lose_padding_but_keep_their_words_and_the_changes_are_named() {
        let mut tags = TagInfo {
            title: Some("Night  Drive (Dub Mix) -".into()),
            album: Some("Between Spaces".into()),
            album_artist: Some("Studio 001;".into()),
            ..TagInfo::default()
        };
        assert_eq!(tidy(&mut tags), ["Title", "Album artist"]);
        assert_eq!(tags.title.as_deref(), Some("Night Drive (Dub Mix)"));
        assert_eq!(tags.album.as_deref(), Some("Between Spaces"));
        let mut clean = tags.clone();
        assert_eq!(
            tidy(&mut clean),
            Vec::<&str>::new(),
            "tidy twice changes nothing"
        );
    }
}
