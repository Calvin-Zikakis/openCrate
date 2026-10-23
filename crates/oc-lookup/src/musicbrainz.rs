//! The MusicBrainz web service (`/ws/2`, JSON): a release search by artist and title, and one
//! release with its tracks.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::Deserialize;

use crate::Error;

pub const BASE: &str = "https://musicbrainz.org/ws/2";
/// MusicBrainz asks every client to name itself.
pub const USER_AGENT: &str = concat!(
    "openCrate/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/Calvin-Zikakis/openCrate )"
);

/// Everything but unreserved characters is escaped in a query value.
const QUERY: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// A phrase for the search syntax (Lucene): quoted, with quotes and backslashes escaped.
fn phrase(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Search releases by title and artist; the best few come back, each with a score.
#[must_use]
pub fn search_url(artist: &str, release: &str) -> String {
    let query = format!("release:{} AND artist:{}", phrase(release), phrase(artist));
    format!(
        "{BASE}/release/?query={}&fmt=json&limit=5",
        utf8_percent_encode(&query, QUERY)
    )
}

/// One release with its tracks, artists and labels.
#[must_use]
pub fn release_url(id: &str) -> String {
    let id: String = id
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect();
    format!("{BASE}/release/{id}?inc=recordings+artist-credits+labels&fmt=json")
}

#[derive(Deserialize)]
struct Credit {
    name: String,
    #[serde(default)]
    joinphrase: String,
}

fn credited(credits: &[Credit]) -> String {
    credits
        .iter()
        .fold(String::new(), |mut text, credit| {
            text.push_str(&credit.name);
            text.push_str(&credit.joinphrase);
            text
        })
        .trim()
        .to_owned()
}

#[derive(Deserialize)]
struct LabelName {
    name: String,
}

#[derive(Deserialize)]
struct LabelInfo {
    label: Option<LabelName>,
}

fn first_label(info: &[LabelInfo]) -> Option<String> {
    info.iter()
        .find_map(|item| item.label.as_ref().map(|label| label.name.clone()))
}

fn year(date: Option<&str>) -> Option<i32> {
    date?.get(..4)?.parse().ok()
}

/// One search result.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub id: String,
    /// 0 to 100: how well MusicBrainz thinks it matches the query.
    pub score: u32,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub tracks: usize,
}

/// The candidate to fetch: a score of at least 80, preferring one with as many tracks as there
/// are files, then the highest score, then the earliest year (the original over reissues), then
/// MusicBrainz's own order. `None` when nothing is close enough to trust.
#[must_use]
pub fn pick(candidates: &[Candidate], files: usize) -> Option<&Candidate> {
    let rank = |candidate: &Candidate| {
        (
            candidate.tracks == files,
            candidate.score,
            std::cmp::Reverse(candidate.year.unwrap_or(i32::MAX)),
        )
    };
    let mut best: Option<&Candidate> = None;
    for candidate in candidates.iter().filter(|candidate| candidate.score >= 80) {
        // Strictly better only, so the first of equals wins.
        if best.is_none_or(|kept| rank(candidate) > rank(kept)) {
            best = Some(candidate);
        }
    }
    best
}

#[derive(Deserialize)]
struct SearchAnswer {
    releases: Vec<SearchRelease>,
}

#[derive(Deserialize)]
struct SearchRelease {
    id: String,
    #[serde(default)]
    score: u32,
    title: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
    date: Option<String>,
    #[serde(rename = "track-count", default)]
    track_count: usize,
}

pub fn parse_search(json: &str) -> Result<Vec<Candidate>, Error> {
    let answer: SearchAnswer =
        serde_json::from_str(json).map_err(|error| Error::Unreadable(error.to_string()))?;
    Ok(answer
        .releases
        .into_iter()
        .map(|release| Candidate {
            year: year(release.date.as_deref()),
            artist: credited(&release.artist_credit),
            id: release.id,
            score: release.score,
            title: release.title,
            tracks: release.track_count,
        })
        .collect())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub position: u32,
    pub title: String,
    pub artist: String,
    pub length_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Medium {
    pub position: u32,
    pub tracks: Vec<Track>,
}

/// A release with what the files are matched against.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub media: Vec<Medium>,
}

impl Release {
    #[must_use]
    pub fn web_page(&self) -> String {
        format!("https://musicbrainz.org/release/{}", self.id)
    }

    #[must_use]
    pub fn track_count(&self) -> usize {
        self.media.iter().map(|medium| medium.tracks.len()).sum()
    }
}

#[derive(Deserialize)]
struct ReleaseAnswer {
    id: String,
    title: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
    date: Option<String>,
    #[serde(rename = "label-info", default)]
    label_info: Vec<LabelInfo>,
    #[serde(default)]
    media: Vec<MediumAnswer>,
}

#[derive(Deserialize)]
struct MediumAnswer {
    #[serde(default)]
    position: u32,
    #[serde(default)]
    tracks: Vec<TrackAnswer>,
}

#[derive(Deserialize)]
struct TrackAnswer {
    #[serde(default)]
    position: u32,
    title: String,
    length: Option<u64>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
}

pub fn parse_release(json: &str) -> Result<Release, Error> {
    let answer: ReleaseAnswer =
        serde_json::from_str(json).map_err(|error| Error::Unreadable(error.to_string()))?;
    let artist = credited(&answer.artist_credit);
    Ok(Release {
        id: answer.id,
        title: answer.title,
        year: year(answer.date.as_deref()),
        label: first_label(&answer.label_info),
        media: answer
            .media
            .into_iter()
            .map(|medium| Medium {
                position: medium.position,
                tracks: medium
                    .tracks
                    .into_iter()
                    .map(|track| Track {
                        position: track.position,
                        artist: if track.artist_credit.is_empty() {
                            artist.clone()
                        } else {
                            credited(&track.artist_credit)
                        },
                        title: track.title,
                        length_ms: track.length,
                    })
                    .collect(),
            })
            .collect(),
        artist,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A synthetic answer in the shape MusicBrainz sends.
    pub const RELEASE: &str = r#"{
        "id": "0b1d1c3e-0000-4000-8000-000000000001",
        "title": "Between Spaces",
        "date": "2019-05-03",
        "artist-credit": [{"name": "Studio 001", "joinphrase": "", "artist": {"name": "Studio 001"}}],
        "label-info": [{"catalog-number": "TL001", "label": {"name": "Test Label"}}],
        "media": [{"position": 1, "format": "Digital Media", "track-count": 3, "tracks": [
            {"position": 1, "number": "1", "title": "Night Drive", "length": 225000,
             "artist-credit": [{"name": "Studio 001", "joinphrase": ""}]},
            {"position": 2, "number": "2", "title": "Soft Focus (Dub Mix)", "length": 301000,
             "artist-credit": [{"name": "Studio 001", "joinphrase": " feat. "}, {"name": "Guest Vocalist", "joinphrase": ""}]},
            {"position": 3, "number": "3", "title": "Late Train", "length": null, "artist-credit": []}
        ]}]
    }"#;

    #[test]
    fn a_search_is_one_quoted_query_and_the_answer_lists_candidates() {
        let url = search_url("Studio \"001\"", "Between Spaces");
        assert!(url.starts_with("https://musicbrainz.org/ws/2/release/?query="));
        assert!(
            url.contains(
                "release%3A%22Between%20Spaces%22%20AND%20artist%3A%22Studio%20%5C%22001%5C%22%22"
            ),
            "{url}"
        );
        assert!(url.ends_with("&fmt=json&limit=5"));
        let answer = r#"{"created":"x","count":1,"offset":0,"releases":[
            {"id":"r1","score":100,"title":"Between Spaces","date":"2019","track-count":3,
             "artist-credit":[{"name":"Studio 001","joinphrase":" & "},{"name":"Guest","joinphrase":""}]},
            {"id":"r2","title":"Between Spaces (Remixes)","track-count":6}
        ]}"#;
        let found = parse_search(answer).unwrap();
        assert_eq!(found[0].artist, "Studio 001 & Guest");
        assert_eq!(
            (found[0].score, found[0].year, found[0].tracks),
            (100, Some(2019), 3)
        );
        assert_eq!((found[1].score, found[1].year), (0, None));
        assert!(parse_search("{}").is_err());
        assert!(parse_search("<html>").is_err());
    }

    #[test]
    fn the_candidate_with_matching_tracks_wins_and_weak_ones_are_not_trusted() {
        let candidate = |id: &str, score: u32, tracks: usize| Candidate {
            id: id.into(),
            score,
            title: "Between Spaces".into(),
            artist: "Studio 001".into(),
            year: None,
            tracks,
        };
        let found = [
            candidate("deluxe", 100, 14),
            candidate("standard", 95, 10),
            candidate("weak", 60, 10),
        ];
        assert_eq!(pick(&found, 10).unwrap().id, "standard");
        assert_eq!(pick(&found, 12).unwrap().id, "deluxe");
        assert_eq!(pick(&found[2..], 10), None);
        let mut reissue = candidate("reissue", 100, 14);
        reissue.year = Some(2014);
        let mut original = candidate("original", 100, 14);
        original.year = Some(2001);
        let undated = candidate("undated", 100, 14);
        assert_eq!(
            pick(&[reissue.clone(), original, undated.clone()], 2)
                .unwrap()
                .id,
            "original"
        );
        assert_eq!(
            pick(&[undated, reissue], 2).unwrap().id,
            "reissue",
            "a known year wins"
        );
    }

    #[test]
    fn a_release_reads_with_its_tracks_and_credits() {
        let release = parse_release(RELEASE).unwrap();
        assert_eq!(release.year, Some(2019));
        assert_eq!(release.label.as_deref(), Some("Test Label"));
        assert_eq!(release.track_count(), 3);
        let tracks = &release.media[0].tracks;
        assert_eq!(tracks[1].artist, "Studio 001 feat. Guest Vocalist");
        assert_eq!(
            tracks[2].artist, "Studio 001",
            "no credit means the release artist"
        );
        assert_eq!(tracks[2].length_ms, None);
        assert_eq!(
            release_url("0b1d1c3e-0000-4000-8000-000000000001/../x?y"),
            "https://musicbrainz.org/ws/2/release/0b1d1c3e-0000-4000-8000-000000000001?inc=recordings+artist-credits+labels&fmt=json"
        );
    }
}
