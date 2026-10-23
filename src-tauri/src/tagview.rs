//! The tags of one file as the person should see them before it is imported: each value, which
//! values were not in the file's tags and came from its name instead, the audio facts, and
//! sanity warnings for the mistakes downloads usually have. Pure: it only formats what was read.

use oc_tag::{FileInfo, ReadError, TagInfo};
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagFieldDto {
    pub name: &'static str,
    pub value: Option<String>,
    /// Not in the file's tags; taken from its folder and file names.
    pub guessed: bool,
    /// The value before stray separators and padding were tidied away.
    pub tidied_from: Option<String>,
    /// Taken from a release looked up online, as the owner accepted.
    pub looked_up: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagViewDto {
    pub fields: Vec<TagFieldDto>,
    /// The format in a few characters: `AIFF 16/44.1` when lossless, `MP3 320 kbps` when not.
    pub audio: Option<String>,
    /// `m:ss`.
    pub length: Option<String>,
    pub read_error: Option<String>,
    /// Things that look wrong. They do not stop an import; they are for the person to judge.
    pub warnings: Vec<String>,
}

fn clock(milliseconds: i64) -> String {
    let seconds = milliseconds / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn audio_line(info: &FileInfo) -> String {
    let rate = info
        .audio
        .sample_rate
        .map(|rate| format!("{}", f64::from(rate) / 1000.0));
    match (info.audio.bit_depth, rate, info.audio.bitrate_kbps) {
        (Some(depth), Some(rate), _) => format!("{} {depth}/{rate}", info.container),
        (_, _, Some(bitrate)) => format!("{} {bitrate} kbps", info.container),
        _ => info.container.to_owned(),
    }
}

const AUDIO_EXTENSIONS: [&str; 7] = [".mp3", ".flac", ".wav", ".aif", ".aiff", ".m4a", ".ogg"];

fn warnings(tags: &TagInfo, tagged: &TagInfo, current_year: i32) -> Vec<String> {
    let mut found = Vec::new();
    let artist = tags.artist.as_deref().unwrap_or("");
    let title = tags.title.as_deref().unwrap_or("");
    let (artist_l, title_l) = (artist.to_lowercase(), title.to_lowercase());
    if !artist.is_empty() && title_l.starts_with(&format!("{artist_l} - ")) {
        found.push(
            "The title starts with the artist's name (\"Artist - Title\"), so it would be filed that way."
                .to_owned(),
        );
    }
    if AUDIO_EXTENSIONS.iter().any(|ext| title_l.ends_with(ext)) {
        found.push("The title ends with a file extension.".to_owned());
    }
    if !title.is_empty() && artist_l == title_l {
        found.push("The artist and the title are the same.".to_owned());
    }
    if [artist, title, tags.album.as_deref().unwrap_or("")]
        .iter()
        .any(|text| {
            let text = text.to_lowercase();
            text.contains("http") || text.contains("www.")
        })
    {
        found.push("A tag contains a web address.".to_owned());
    }
    if tags.track_number == Some(0) {
        found.push("The track number is 0.".to_owned());
    }
    if tags
        .year
        .is_some_and(|year| year < 1900 || year > current_year + 1)
    {
        found.push("The year looks wrong.".to_owned());
    }
    if tagged.artist.is_none() || tagged.title.is_none() {
        found
            .push("The artist or title is not in the tags; it came from the file name.".to_owned());
    }
    found
}

/// Merge the file's own tags with what its name suggests, and describe the result. The merged
/// tags are what the destination is computed from.
pub fn tag_view(
    read: Result<FileInfo, ReadError>,
    guess: &TagInfo,
    current_year: i32,
    tidy: bool,
    lookup: Option<&TagInfo>,
) -> (TagInfo, TagViewDto) {
    let (tagged, audio, length, read_error) = match read {
        Ok(info) => {
            let line = audio_line(&info);
            let length = (info.audio.duration_ms > 0).then(|| clock(info.audio.duration_ms));
            (info.tags, Some(line), length, None)
        }
        Err(error) => (TagInfo::default(), None, None, Some(error.to_string())),
    };
    let mut merged = tagged.clone();
    merged.fill_from(guess);
    let untidy = merged.clone();
    let tidied = if tidy {
        oc_tag::tidy(&mut merged)
    } else {
        Vec::new()
    };
    let accepted = lookup.cloned().unwrap_or_default();
    let mut over = accepted.clone();
    over.fill_from(&merged);
    merged = over;
    macro_rules! field {
        ($name:literal, $get:expr) => {{
            let value = $get(&merged);
            let looked_up = $get(&accepted).is_some();
            TagFieldDto {
                name: $name,
                looked_up,
                guessed: !looked_up && value.is_some() && $get(&tagged).is_none(),
                tidied_from: if tidied.contains(&$name) {
                    $get(&untidy)
                } else {
                    None
                },
                value,
            }
        }};
    }
    let text = |value: &Option<String>| value.clone();
    let number = |value: Option<u32>| value.map(|n| n.to_string());
    let fields = vec![
        field!("Artist", |t: &TagInfo| text(&t.artist)),
        field!("Title", |t: &TagInfo| text(&t.title)),
        field!("Album", |t: &TagInfo| text(&t.album)),
        field!("Album artist", |t: &TagInfo| text(&t.album_artist)),
        field!("Track", |t: &TagInfo| number(t.track_number)),
        field!("Disc", |t: &TagInfo| number(t.disc_number)),
        field!("Year", |t: &TagInfo| t.year.map(|y| y.to_string())),
        field!("Label", |t: &TagInfo| text(&t.label)),
        field!("Genre", |t: &TagInfo| text(&t.genre)),
        field!("Key", |t: &TagInfo| text(&t.key)),
        field!("BPM", |t: &TagInfo| t.bpm.map(|b| format!("{b}"))),
    ];
    // A value accepted from a lookup is as good as one in the tags.
    let mut known = accepted.clone();
    known.fill_from(&tagged);
    let mut notes = warnings(&merged, &known, current_year);
    if let Some(error) = &read_error {
        notes.insert(
            0,
            format!("The tags could not be read ({error}); the file name was used."),
        );
    }
    (
        merged,
        TagViewDto {
            fields,
            audio,
            length,
            read_error,
            warnings: notes,
        },
    )
}

#[cfg(test)]
mod tests {
    use oc_tag::AudioInfo;

    use super::*;

    fn info(tags: TagInfo) -> FileInfo {
        FileInfo {
            container: "FLAC",
            audio: AudioInfo {
                duration_ms: 225_000,
                bitrate_kbps: Some(1411),
                sample_rate: Some(44_100),
                bit_depth: Some(16),
                channels: Some(2),
            },
            tags,
        }
    }

    fn tags() -> TagInfo {
        TagInfo {
            artist: Some("Studio 001".into()),
            title: Some("Night Drive".into()),
            album: Some("Between Spaces".into()),
            track_number: Some(3),
            year: Some(2019),
            bpm: Some(126.0),
            ..TagInfo::default()
        }
    }

    fn field<'a>(view: &'a TagViewDto, name: &str) -> &'a TagFieldDto {
        view.fields.iter().find(|f| f.name == name).unwrap()
    }

    #[test]
    fn clean_tags_show_their_values_and_audio_facts_without_warnings() {
        let (merged, view) = tag_view(Ok(info(tags())), &TagInfo::default(), 2026, false, None);
        assert_eq!(merged, tags());
        assert_eq!(field(&view, "Artist").value.as_deref(), Some("Studio 001"));
        assert_eq!(field(&view, "Track").value.as_deref(), Some("3"));
        assert_eq!(field(&view, "BPM").value.as_deref(), Some("126"));
        assert_eq!(field(&view, "Label").value, None);
        assert!(view.fields.iter().all(|f| !f.guessed));
        assert_eq!(view.audio.as_deref(), Some("FLAC 16/44.1"));
        assert_eq!(view.length.as_deref(), Some("3:45"));
        let mut lossy = info(tags());
        lossy.container = "MP3";
        lossy.audio.bit_depth = None;
        lossy.audio.bitrate_kbps = Some(320);
        let (_, lossy) = tag_view(Ok(lossy), &TagInfo::default(), 2026, false, None);
        assert_eq!(lossy.audio.as_deref(), Some("MP3 320 kbps"));
        assert_eq!(view.warnings.len(), 0, "{:?}", view.warnings);
    }

    #[test]
    fn values_taken_from_the_file_name_are_marked_and_explained() {
        let mut bare = tags();
        bare.track_number = None;
        bare.artist = None;
        let guess = TagInfo {
            artist: Some("Guessed Artist".into()),
            track_number: Some(7),
            ..TagInfo::default()
        };
        let (merged, view) = tag_view(Ok(info(bare)), &guess, 2026, false, None);
        assert_eq!(merged.artist.as_deref(), Some("Guessed Artist"));
        assert!(field(&view, "Artist").guessed && field(&view, "Track").guessed);
        assert!(!field(&view, "Title").guessed);
        assert!(view
            .warnings
            .iter()
            .any(|w| w.contains("came from the file name")));
    }

    #[test]
    fn the_usual_download_mistakes_are_flagged() {
        let mut bad = tags();
        bad.title = Some("Studio 001 - Night Drive.mp3".into());
        bad.track_number = Some(0);
        bad.year = Some(1850);
        bad.album = Some("Free at www.example.com".into());
        let (_, view) = tag_view(Ok(info(bad)), &TagInfo::default(), 2026, false, None);
        for needle in [
            "starts with the artist",
            "file extension",
            "track number is 0",
            "year looks wrong",
            "web address",
        ] {
            assert!(
                view.warnings.iter().any(|w| w.contains(needle)),
                "{needle}: {:?}",
                view.warnings
            );
        }
        let mut same = tags();
        same.title = same.artist.clone();
        let (_, view) = tag_view(Ok(info(same)), &TagInfo::default(), 2026, false, None);
        assert!(view.warnings.iter().any(|w| w.contains("same")));
        let mut future = tags();
        future.year = Some(2040);
        let (_, view) = tag_view(Ok(info(future)), &TagInfo::default(), 2026, false, None);
        assert!(view.warnings.iter().any(|w| w.contains("year")));
    }

    #[test]
    fn tidied_values_show_what_they_were() {
        let mut messy = tags();
        messy.artist = Some("Studio 001;".into());
        let (merged, view) = tag_view(
            Ok(info(messy.clone())),
            &TagInfo::default(),
            2026,
            true,
            None,
        );
        assert_eq!(merged.artist.as_deref(), Some("Studio 001"));
        let artist = field(&view, "Artist");
        assert_eq!(artist.tidied_from.as_deref(), Some("Studio 001;"));
        assert_eq!(field(&view, "Title").tidied_from, None);
        let (kept, _) = tag_view(Ok(info(messy)), &TagInfo::default(), 2026, false, None);
        assert_eq!(
            kept.artist.as_deref(),
            Some("Studio 001;"),
            "only when asked"
        );
    }

    #[test]
    fn accepted_lookup_values_win_and_are_marked() {
        let mut bare = tags();
        bare.track_number = None;
        let guess = TagInfo {
            track_number: Some(7),
            ..TagInfo::default()
        };
        let lookup = TagInfo {
            track_number: Some(3),
            label: Some("Test Label".into()),
            ..TagInfo::default()
        };
        let (merged, view) = tag_view(Ok(info(bare)), &guess, 2026, false, Some(&lookup));
        assert_eq!(
            (merged.track_number, merged.label.as_deref()),
            (Some(3), Some("Test Label"))
        );
        let track = field(&view, "Track");
        assert!(track.looked_up && !track.guessed);
        assert!(!field(&view, "Title").looked_up);
        assert!(
            !view.warnings.iter().any(|w| w.contains("file name")),
            "{:?}",
            view.warnings
        );
    }

    #[test]
    fn unreadable_tags_fall_back_to_the_name_and_say_so() {
        let guess = TagInfo {
            artist: Some("A".into()),
            title: Some("T".into()),
            ..TagInfo::default()
        };
        let error = ReadError::Parse("bad frame".into());
        let (merged, view) = tag_view(Err(error), &guess, 2026, false, None);
        assert_eq!(merged.title.as_deref(), Some("T"));
        assert!(view.read_error.is_some() && view.audio.is_none());
        assert!(view.warnings[0].starts_with("The tags could not be read"));
        assert!(field(&view, "Title").guessed);
    }
}
