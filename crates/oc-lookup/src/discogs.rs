//! The Discogs API, with the owner's personal access token: a release search by artist and
//! title, and one release with its tracks. Discogs also has genres and styles, which MusicBrainz
//! lacks; a style is the more useful of the two for a DJ, so it is preferred.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::Deserialize;

use crate::musicbrainz::{Medium, Release, Track};
use crate::Error;

pub const DISCOGS_BASE: &str = "https://api.discogs.com";

const QUERY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Search releases by artist and title; the first few come back in Discogs's order.
#[must_use]
pub fn discogs_search_url(artist: &str, release: &str) -> String {
    format!(
        "{DISCOGS_BASE}/database/search?type=release&artist={}&release_title={}&per_page=5",
        utf8_percent_encode(artist, QUERY),
        utf8_percent_encode(release, QUERY)
    )
}

#[must_use]
pub fn discogs_release_url(id: u64) -> String {
    format!("{DISCOGS_BASE}/releases/{id}")
}

/// Asked with the token, it names the account: a cheap way to check a token works.
#[must_use]
pub fn discogs_identity_url() -> String {
    format!("{DISCOGS_BASE}/oauth/identity")
}

#[derive(Deserialize)]
struct SearchAnswer {
    #[serde(default)]
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    id: u64,
}

/// The release ids a search found, best first.
pub fn parse_discogs_search(json: &str) -> Result<Vec<u64>, Error> {
    let answer: SearchAnswer =
        serde_json::from_str(json).map_err(|error| Error::Unreadable(error.to_string()))?;
    Ok(answer.results.into_iter().map(|result| result.id).collect())
}

#[derive(Deserialize)]
struct ArtistAnswer {
    name: String,
    #[serde(default)]
    join: String,
}

#[derive(Deserialize)]
struct LabelAnswer {
    name: String,
}

#[derive(Deserialize)]
struct TrackAnswer {
    #[serde(default)]
    position: String,
    #[serde(rename = "type_", default)]
    kind: String,
    title: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    artists: Vec<ArtistAnswer>,
}

#[derive(Deserialize)]
struct ReleaseAnswer {
    id: u64,
    title: String,
    #[serde(default)]
    artists: Vec<ArtistAnswer>,
    #[serde(default)]
    year: i32,
    #[serde(default)]
    labels: Vec<LabelAnswer>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default)]
    tracklist: Vec<TrackAnswer>,
    #[serde(default)]
    uri: String,
}

/// Discogs tells same-named artists and labels apart with a number: "Oak (3)".
fn plain(name: &str) -> String {
    let name = name.trim();
    if let Some(open) = name.rfind(" (") {
        let inside = &name[open + 2..];
        if inside.ends_with(')')
            && inside[..inside.len() - 1]
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            return name[..open].to_owned();
        }
    }
    name.to_owned()
}

fn credited(artists: &[ArtistAnswer]) -> String {
    let mut text = String::new();
    for (index, artist) in artists.iter().enumerate() {
        text.push_str(&plain(&artist.name));
        if index + 1 < artists.len() {
            let join = artist.join.trim();
            if join.is_empty() || join == "," {
                text.push_str(", ");
            } else {
                text.push(' ');
                text.push_str(join);
                text.push(' ');
            }
        }
    }
    text
}

/// "5:31" or "1:02:03" in milliseconds.
fn duration_ms(text: &str) -> Option<u64> {
    let mut seconds = 0u64;
    for part in text.trim().split(':') {
        seconds = seconds * 60 + part.parse::<u64>().ok()?;
    }
    (seconds > 0).then_some(seconds * 1000)
}

/// Where a track sits: "1-2" is disc 1, track 2. Anything else (a number, or a vinyl side
/// such as "A1" and "B2") is on disc 1, numbered in order.
fn disc_of(position: &str) -> Option<(u32, u32)> {
    let (disc, track) = position.split_once('-')?;
    Some((disc.trim().parse().ok()?, track.trim().parse().ok()?))
}

pub fn parse_discogs_release(json: &str) -> Result<Release, Error> {
    let answer: ReleaseAnswer =
        serde_json::from_str(json).map_err(|error| Error::Unreadable(error.to_string()))?;
    let artist = credited(&answer.artists);
    let mut media: Vec<Medium> = Vec::new();
    let mut in_order = 0;
    for track in answer
        .tracklist
        .into_iter()
        .filter(|track| track.kind.is_empty() || track.kind == "track")
    {
        let (disc, position) = disc_of(&track.position).unwrap_or_else(|| {
            in_order += 1;
            (1, in_order)
        });
        let entry = Track {
            position,
            artist: if track.artists.is_empty() {
                artist.clone()
            } else {
                credited(&track.artists)
            },
            length_ms: duration_ms(&track.duration),
            title: track.title,
        };
        match media.iter_mut().find(|medium| medium.position == disc) {
            Some(medium) => medium.tracks.push(entry),
            None => media.push(Medium {
                position: disc,
                tracks: vec![entry],
            }),
        }
    }
    media.sort_by_key(|medium| medium.position);
    Ok(Release {
        id: answer.id.to_string(),
        title: answer.title,
        artist,
        year: (answer.year > 0).then_some(answer.year),
        label: answer.labels.first().map(|label| plain(&label.name)),
        genre: answer
            .styles
            .into_iter()
            .next()
            .or_else(|| answer.genres.into_iter().next()),
        page: if answer.uri.is_empty() {
            format!("https://www.discogs.com/release/{}", answer.id)
        } else {
            answer.uri
        },
        media,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic answer in the shape Discogs sends, for a two-sided vinyl single.
    const RELEASE: &str = r#"{
        "id": 1234567, "title": "No Light", "year": 2025,
        "artists": [{"name": "Studio 001 (2)", "join": "&"}, {"name": "Guest", "join": ""}],
        "labels": [{"name": "Test Label (4)", "catno": "TL 001"}],
        "genres": ["Electronic"], "styles": ["Techno", "Dub Techno"],
        "uri": "https://www.discogs.com/release/1234567-Studio-001-No-Light",
        "tracklist": [
            {"position": "", "type_": "heading", "title": "Side A", "duration": ""},
            {"position": "A", "type_": "track", "title": "Night Drive", "duration": "4:31"},
            {"position": "B", "type_": "track", "title": "Soft Focus", "duration": "4:33",
             "artists": [{"name": "Guest", "join": ""}]}
        ]
    }"#;

    #[test]
    fn a_vinyl_release_reads_as_one_disc_in_order() {
        let release = parse_discogs_release(RELEASE).unwrap();
        assert_eq!(release.title, "No Light");
        assert_eq!(release.artist, "Studio 001 & Guest");
        assert_eq!(release.year, Some(2025));
        assert_eq!(release.label.as_deref(), Some("Test Label"));
        assert_eq!(
            release.genre.as_deref(),
            Some("Techno"),
            "a style before a genre"
        );
        assert_eq!(
            release.web_page(),
            "https://www.discogs.com/release/1234567-Studio-001-No-Light"
        );
        assert_eq!(release.media.len(), 1);
        let tracks = &release.media[0].tracks;
        assert_eq!(tracks.len(), 2, "the side heading is not a track");
        assert_eq!(
            (tracks[0].position, tracks[0].title.as_str()),
            (1, "Night Drive")
        );
        assert_eq!(tracks[0].length_ms, Some(271_000));
        assert_eq!(
            (tracks[1].position, tracks[1].artist.as_str()),
            (2, "Guest")
        );
    }

    #[test]
    fn numbered_discs_are_kept_apart_and_missing_fields_are_none() {
        let release = parse_discogs_release(
            r#"{"id": 9, "title": "Double", "artists": [{"name": "Studio 001"}],
                "tracklist": [
                    {"position": "1-1", "title": "One", "duration": ""},
                    {"position": "2-1", "title": "Two", "duration": "1:02:03"}
                ]}"#,
        )
        .unwrap();
        assert_eq!(
            (release.year, &release.label, &release.genre),
            (None, &None, &None)
        );
        assert_eq!(release.web_page(), "https://www.discogs.com/release/9");
        assert_eq!(release.media.len(), 2);
        assert_eq!(release.media[1].position, 2);
        assert_eq!(release.media[0].tracks[0].length_ms, None);
        assert_eq!(release.media[1].tracks[0].length_ms, Some(3_723_000));
    }

    #[test]
    fn a_search_gives_ids_in_order_and_urls_are_escaped() {
        assert_eq!(
            parse_discogs_search(r#"{"results": [{"id": 3, "title": "x"}, {"id": 1}]}"#).unwrap(),
            [3, 1]
        );
        assert_eq!(parse_discogs_search("{}").unwrap(), Vec::<u64>::new());
        assert!(parse_discogs_search("nope").is_err());
        let url = discogs_search_url("Studio 001 & Guest", "No Light / Dub");
        assert_eq!(
            url,
            "https://api.discogs.com/database/search?type=release&artist=Studio%20001%20%26%20Guest&release_title=No%20Light%20%2F%20Dub&per_page=5"
        );
        assert_eq!(
            discogs_release_url(42),
            "https://api.discogs.com/releases/42"
        );
    }

    #[test]
    fn numbered_names_lose_only_the_number() {
        assert_eq!(plain("Oak (3)"), "Oak");
        assert_eq!(plain("Bassline (Dub Mix)"), "Bassline (Dub Mix)");
        assert_eq!(plain("Studio 001"), "Studio 001");
    }
}
