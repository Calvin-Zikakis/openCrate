//! Matching a release's tracks to the files of a download, and the tag values that follow.

use oc_tag::TagInfo;

use crate::{Release, Track};

/// What is known about one file before the lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct FileFacts {
    /// Relative to the download folder; how the page names the file.
    pub source: String,
    pub tags: TagInfo,
    pub duration_ms: i64,
}

/// What the release says a file's tags should be, where that differs from what it has.
#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub source: String,
    /// Only the fields that would change.
    pub values: TagInfo,
    /// How the file was matched to a track, in plain words; `None` when it was not.
    pub matched_by: Option<&'static str>,
}

/// Lower case, letters and digits only, single spaces: "Night Drive (Dub Mix)" and
/// "night drive - dub mix" compare equal.
#[must_use]
pub fn normalized(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn same_title(file: Option<&str>, track: &str) -> bool {
    let Some(file) = file else {
        return false;
    };
    let (a, b) = (normalized(file), normalized(track));
    a == b || (a.len() >= 4 && b.len() >= 4 && (a.contains(&b) || b.contains(&a)))
}

/// Lengths within four seconds, which absorbs encoder padding and rounding.
fn same_length(file_ms: i64, track: &Track) -> bool {
    track
        .length_ms
        .and_then(|length| i64::try_from(length).ok())
        .is_some_and(|length| file_ms > 0 && (length - file_ms).abs() <= 4_000)
}

/// Every track with its disc and position, in release order.
fn tracks(release: &Release) -> Vec<(u32, &Track)> {
    release
        .media
        .iter()
        .flat_map(|medium| {
            medium
                .tracks
                .iter()
                .map(move |track| (medium.position.max(1), track))
        })
        .collect()
}

fn find(file: &FileFacts, all: &[(u32, &Track)], taken: &[bool]) -> Option<(usize, &'static str)> {
    let free = |index: &usize| !taken[*index];
    if let Some(number) = file.tags.track_number {
        let disc = file.tags.disc_number.unwrap_or(1);
        let at = (0..all.len()).filter(free).find(|&index| {
            let (medium, track) = all[index];
            medium == disc && track.position == number
        });
        if let Some(index) = at {
            let track = all[index].1;
            if same_title(file.tags.title.as_deref(), &track.title) {
                return Some((index, "track number and title"));
            }
            if same_length(file.duration_ms, track) {
                return Some((index, "track number and length"));
            }
        }
    }
    (0..all.len())
        .filter(free)
        .find(|&index| same_title(file.tags.title.as_deref(), &all[index].1.title))
        .map(|index| (index, "title"))
}

/// The release's values for each file, where they differ from the file's. Each track is given
/// to at most one file; files that match no track get an empty suggestion with no match.
#[must_use]
pub fn suggest(release: &Release, files: &[FileFacts]) -> Vec<Suggestion> {
    let all = tracks(release);
    let several_discs = release.media.len() > 1;
    let mut taken = vec![false; all.len()];
    files
        .iter()
        .map(|file| {
            let Some((index, how)) = find(file, &all, &taken) else {
                return Suggestion {
                    source: file.source.clone(),
                    values: TagInfo::default(),
                    matched_by: None,
                };
            };
            taken[index] = true;
            let (disc, track) = all[index];
            let wanted = TagInfo {
                title: Some(track.title.clone()),
                artist: Some(track.artist.clone()),
                album: Some(release.title.clone()),
                album_artist: Some(release.artist.clone()),
                label: release.label.clone(),
                genre: release.genre.clone(),
                year: release.year,
                track_number: Some(track.position),
                disc_number: several_discs.then_some(disc),
                ..TagInfo::default()
            };
            Suggestion {
                source: file.source.clone(),
                values: changes(&wanted, &file.tags),
                matched_by: Some(how),
            }
        })
        .collect()
}

/// The fields of `wanted` that are set and differ from `current`.
fn changes(wanted: &TagInfo, current: &TagInfo) -> TagInfo {
    macro_rules! keep {
        ($($field:ident),+) => {
            TagInfo { $($field: wanted.$field.clone().filter(|value| current.$field.as_ref() != Some(value)),)+ ..TagInfo::default() }
        };
    }
    keep!(
        title,
        artist,
        album,
        album_artist,
        label,
        genre,
        year,
        track_number,
        disc_number
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::musicbrainz::tests::RELEASE;
    use crate::parse_release;

    fn file(source: &str, title: Option<&str>, track: Option<u32>, duration_ms: i64) -> FileFacts {
        FileFacts {
            source: source.into(),
            tags: TagInfo {
                title: title.map(Into::into),
                artist: Some("Studio 001".into()),
                album: Some("Between Spaces".into()),
                track_number: track,
                ..TagInfo::default()
            },
            duration_ms,
        }
    }

    #[test]
    fn files_match_by_number_and_title_or_length_then_by_title_alone() {
        let release = parse_release(RELEASE).unwrap();
        let found = suggest(
            &release,
            &[
                file("a.flac", Some("Night Drive"), Some(1), 0),
                // Wrong title, right number and length: the length confirms it.
                file("b.flac", Some("Track 02"), Some(2), 302_500),
                // No number: found by title, punctuation aside.
                file("c.flac", Some("late train"), None, 0),
                file("d.flac", Some("Bonus Beat"), Some(9), 0),
            ],
        );
        let how: Vec<_> = found.iter().map(|s| s.matched_by).collect();
        assert_eq!(
            how,
            [
                Some("track number and title"),
                Some("track number and length"),
                Some("title"),
                None
            ]
        );
        assert_eq!(found[0].values.title, None, "already right");
        assert_eq!(found[0].values.year, Some(2019));
        assert_eq!(found[0].values.label.as_deref(), Some("Test Label"));
        assert_eq!(
            found[1].values.title.as_deref(),
            Some("Soft Focus (Dub Mix)")
        );
        assert_eq!(
            found[1].values.artist.as_deref(),
            Some("Studio 001 feat. Guest Vocalist")
        );
        assert_eq!(found[2].values.track_number, Some(3));
        assert_eq!(
            found[2].values.disc_number, None,
            "one disc: no disc number"
        );
        assert_eq!(found[3].values, TagInfo::default());
    }

    #[test]
    fn a_track_goes_to_one_file_only_and_a_wrong_number_alone_is_not_trusted() {
        let release = parse_release(RELEASE).unwrap();
        let found = suggest(
            &release,
            &[
                file("a.flac", Some("Night Drive"), Some(1), 0),
                file("copy.flac", Some("Night Drive"), Some(1), 0),
                file("x.flac", Some("Something Else"), Some(3), 10_000),
            ],
        );
        assert!(found[0].matched_by.is_some());
        assert_eq!(found[1].matched_by, None, "the track is already taken");
        assert_eq!(found[2].matched_by, None, "number without title or length");
    }

    #[test]
    fn titles_compare_without_case_or_punctuation() {
        assert_eq!(normalized("Night Drive (Dub Mix)!"), "night drive dub mix");
        assert!(same_title(
            Some("Night Drive - Dub Mix"),
            "Night Drive (Dub Mix)"
        ));
        assert!(same_title(
            Some("Night Drive"),
            "Night Drive (Original Mix)"
        ));
        assert!(!same_title(Some("Go"), "Go West"), "too short to contain");
        assert!(!same_title(None, "Night Drive"));
    }
}
