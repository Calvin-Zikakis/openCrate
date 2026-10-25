//! Looking a release up when the owner asks, in MusicBrainz and, with the owner's own token, in
//! Discogs, and turning the answer into tag suggestions for the release's files. Nothing is
//! looked up on its own, and nothing is written: the page shows the suggestions, and the owner's
//! accepted values travel with the import.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use oc_core_db::service::LibraryService;
use oc_lookup::{
    discogs_identity_url, discogs_release_url, discogs_search_url, parse_discogs_release,
    parse_discogs_search, parse_release, parse_search, pick, release_url, search_url, suggest,
    FileFacts, Release,
};
use oc_tag::TagInfo;
use serde::{Deserialize, Serialize};

use crate::plan::{plan_release, ImportOptions};

/// MusicBrainz allows about one request per second per address; Discogs 60 a minute per token.
const SPACING: Duration = Duration::from_millis(1_100);
const BUSY_RETRIES: u32 = 2;
/// Discogs results tried before giving up on it.
const DISCOGS_TRIES: usize = 3;

/// One lookup service, never asked faster than it allows.
pub struct Throttled {
    name: &'static str,
    agent: ureq::Agent,
    last: Mutex<Option<Instant>>,
}

impl Throttled {
    fn new(name: &'static str) -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(oc_lookup::USER_AGENT)
            .timeout_global(Some(Duration::from_secs(15)))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            name,
            agent,
            last: Mutex::new(None),
        }
    }

    fn wait_turn(&self) {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(left) = (*last).and_then(|previous| SPACING.checked_sub(previous.elapsed())) {
            std::thread::sleep(left);
        }
        *last = Some(Instant::now());
    }

    /// One GET, with the owner's token where the service takes one. A busy answer (503, or 429
    /// from Discogs) is tried again after a pause, twice at most.
    pub(crate) fn get(&self, url: &str, token: Option<&str>) -> Result<String, String> {
        let mut text = String::new();
        self.fetch(url, token, |body| {
            text = body
                .with_config()
                .limit(4 << 20)
                .read_to_string()
                .map_err(|error| error.to_string())?;
            Ok(())
        })?;
        Ok(text)
    }

    /// One GET answered with bytes (an image), at most 12 MB.
    pub(crate) fn get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        self.fetch(url, None, |body| {
            bytes = body
                .with_config()
                .limit(12 << 20)
                .read_to_vec()
                .map_err(|error| error.to_string())?;
            Ok(())
        })?;
        Ok(bytes)
    }

    fn fetch(
        &self,
        url: &str,
        token: Option<&str>,
        mut read: impl FnMut(&mut ureq::Body) -> Result<(), String>,
    ) -> Result<(), String> {
        let name = self.name;
        for attempt in 0..=BUSY_RETRIES {
            self.wait_turn();
            let mut request = self.agent.get(url);
            if let Some(token) = token {
                request = request.header("Authorization", &format!("Discogs token={token}"));
            }
            let mut response = request.call().map_err(|_| {
                format!("{name} could not be reached. Check the internet connection.")
            })?;
            let status = response.status().as_u16();
            if (status == 503 || status == 429) && attempt < BUSY_RETRIES {
                std::thread::sleep(Duration::from_secs(2 << attempt));
                continue;
            }
            return match status {
                200 => read(response.body_mut()),
                503 | 429 => Err(format!("{name} is busy right now. Try again in a minute.")),
                401 | 403 => Err(format!(
                    "{name} did not accept the token. Check it in Settings."
                )),
                404 => Err(format!("{name} no longer has that release.")),
                other => Err(format!("{name} answered with an error ({other}).")),
            };
        }
        Err(format!("{name} is busy right now. Try again in a minute."))
    }
}

/// The lookup services, each with its own pace.
pub struct Lookups {
    pub musicbrainz: Throttled,
    pub discogs: Throttled,
    pub archive: Throttled,
}

impl Default for Lookups {
    fn default() -> Self {
        Self {
            musicbrainz: Throttled::new("MusicBrainz"),
            discogs: Throttled::new("Discogs"),
            archive: Throttled::new("The Cover Art Archive"),
        }
    }
}

/// The owner's own keys for lookup services, kept in openCrate's local settings.
const KEYS: &str = "lookup_keys";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Keys {
    discogs_token: Option<String>,
    acoustid_key: Option<String>,
}

fn keys(service: &LibraryService) -> Result<Keys, String> {
    Ok(service
        .setting(KEYS)
        .map_err(|error| error.to_string())?
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

/// Whether a key is saved, and its last four characters to recognise it by. The key itself
/// never goes back to the page.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyDto {
    pub saved: bool,
    pub ending: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeysDto {
    pub discogs: KeyDto,
    pub acoustid: KeyDto,
    /// What saving found, in plain words.
    pub note: Option<String>,
}

fn shown(key: Option<&String>) -> KeyDto {
    KeyDto {
        saved: key.is_some(),
        // The last four characters, and only of a key long enough that they do not give it away.
        ending: key.and_then(|key| {
            let chars: Vec<char> = key.chars().collect();
            (chars.len() > 8).then(|| chars[chars.len() - 4..].iter().collect())
        }),
    }
}

pub fn keys_status(service: &LibraryService) -> Result<KeysDto, String> {
    let saved = keys(service)?;
    Ok(KeysDto {
        discogs: shown(saved.discogs_token.as_ref()),
        acoustid: shown(saved.acoustid_key.as_ref()),
        note: None,
    })
}

/// Save or clear one key. A Discogs token is checked with Discogs first; one it refuses is not
/// saved.
pub fn set_key(
    service: &LibraryService,
    lookups: &Lookups,
    which: &str,
    value: Option<String>,
) -> Result<KeysDto, String> {
    let value = value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let mut saved = keys(service)?;
    let mut note = None;
    match which {
        "discogs" => {
            if let Some(token) = &value {
                match lookups.discogs.get(&discogs_identity_url(), Some(token)) {
                    Ok(_) => note = Some("Discogs accepted the token.".to_owned()),
                    Err(error) if error.contains("token") => {
                        return Err("Discogs did not accept that token. Copy it again from \
                                    Discogs, Settings, Developers."
                            .into())
                    }
                    Err(error) => note = Some(format!("Saved, but not checked: {error}")),
                }
            }
            saved.discogs_token = value;
        }
        "acoustid" => saved.acoustid_key = value,
        other => return Err(format!("{other} is not a lookup service openCrate knows.")),
    }
    let text = serde_json::to_string(&saved).map_err(|error| error.to_string())?;
    service
        .set_setting(KEYS, Some(text))
        .map_err(|error| error.to_string())?;
    let mut state = keys_status(service)?;
    state.note = note;
    Ok(state)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FoundDto {
    /// Where the release was found: "MusicBrainz" or "Discogs".
    pub source: &'static str,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub tracks: usize,
    /// The release's page on the source's website.
    pub url: String,
    /// A genre or style from the source, when it has one.
    pub genre: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDto {
    /// As the preview names it: "Title", "Album artist"...
    pub field: &'static str,
    pub from: Option<String>,
    pub to: String,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileSuggestionDto {
    pub source: String,
    /// How the file was matched to a track; `None` when it was not.
    pub matched_by: Option<&'static str>,
    pub changes: Vec<ChangeDto>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LookupDto {
    pub found: Option<FoundDto>,
    pub files: Vec<FileSuggestionDto>,
    /// Why nothing was found, in plain words.
    pub note: Option<String>,
}

/// The most common value among the files, for naming the release in a search.
fn most_common<'a>(values: impl Iterator<Item = Option<&'a String>>) -> Option<String> {
    let mut counts: HashMap<&String, usize> = HashMap::new();
    for value in values.flatten() {
        *counts.entry(value).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(value, _)| value.clone())
}

fn changes(current: &TagInfo, wanted: &TagInfo) -> Vec<ChangeDto> {
    let text = |value: Option<&String>| value.cloned();
    let number = |value: Option<u32>| value.map(|n| n.to_string());
    let mut found = Vec::new();
    let mut add = |field, from: Option<String>, to: Option<String>| {
        if let Some(to) = to {
            found.push(ChangeDto { field, from, to });
        }
    };
    add(
        "Artist",
        text(current.artist.as_ref()),
        text(wanted.artist.as_ref()),
    );
    add(
        "Title",
        text(current.title.as_ref()),
        text(wanted.title.as_ref()),
    );
    add(
        "Album",
        text(current.album.as_ref()),
        text(wanted.album.as_ref()),
    );
    add(
        "Album artist",
        text(current.album_artist.as_ref()),
        text(wanted.album_artist.as_ref()),
    );
    add(
        "Track",
        number(current.track_number),
        number(wanted.track_number),
    );
    add(
        "Disc",
        number(current.disc_number),
        number(wanted.disc_number),
    );
    add(
        "Year",
        current.year.map(|y| y.to_string()),
        wanted.year.map(|y| y.to_string()),
    );
    add(
        "Label",
        text(current.label.as_ref()),
        text(wanted.label.as_ref()),
    );
    add(
        "Genre",
        text(current.genre.as_ref()),
        text(wanted.genre.as_ref()),
    );
    found
}

/// What the files say, ready to compare with a release. Pure, for tests.
pub fn facts_of(plan: &crate::plan::PlanDto) -> Vec<FileFacts> {
    plan.files
        .iter()
        .filter_map(|file| {
            let (tags, duration_ms) = file.facts.clone()?;
            Some(FileFacts {
                source: file.source.clone(),
                tags,
                duration_ms,
            })
        })
        .collect()
}

/// Compare a fetched release with the files. Pure, for tests.
pub fn compare(source: &'static str, release: &Release, files: &[FileFacts]) -> LookupDto {
    let suggestions = suggest(release, files);
    LookupDto {
        found: Some(FoundDto {
            source,
            genre: release.genre.clone(),
            title: release.title.clone(),
            artist: release.artist.clone(),
            year: release.year,
            label: release.label.clone(),
            tracks: release.track_count(),
            url: release.web_page(),
        }),
        files: suggestions
            .iter()
            .zip(files)
            .map(|(suggestion, file)| FileSuggestionDto {
                source: suggestion.source.clone(),
                matched_by: suggestion.matched_by,
                changes: changes(&file.tags, &suggestion.values),
            })
            .collect(),
        note: None,
    }
}

fn nothing(note: String) -> LookupDto {
    LookupDto {
        found: None,
        files: Vec::new(),
        note: Some(note),
    }
}

/// The first of Discogs's results whose tracks match at least half the files, if any.
fn discogs_find(
    client: &Throttled,
    token: &str,
    artist: &str,
    album: &str,
    files: &[FileFacts],
) -> Result<Option<Release>, String> {
    let ids = parse_discogs_search(&client.get(&discogs_search_url(artist, album), Some(token))?)
        .map_err(|error| error.to_string())?;
    for id in ids.into_iter().take(DISCOGS_TRIES) {
        let release = parse_discogs_release(&client.get(&discogs_release_url(id), Some(token))?)
            .map_err(|error| error.to_string())?;
        if matches_enough(&release, files) {
            return Ok(Some(release));
        }
    }
    Ok(None)
}

/// A release matches when at least half the files find a track in it.
fn matches_enough(release: &Release, files: &[FileFacts]) -> bool {
    let matched = suggest(release, files)
        .iter()
        .filter(|suggestion| suggestion.matched_by.is_some())
        .count();
    !files.is_empty() && matched * 2 >= files.len()
}

/// MusicBrainz first. With a Discogs token, Discogs adds a genre or style to what MusicBrainz
/// found, and is asked on its own when MusicBrainz finds nothing.
pub fn lookup_release(
    service: &LibraryService,
    lookups: &Lookups,
    folder_id: i64,
    name: &str,
    root_id: i64,
    options: ImportOptions,
) -> Result<LookupDto, String> {
    let plan = plan_release(service, folder_id, name, root_id, options)?;
    let files = facts_of(&plan);
    let artist = most_common(files.iter().map(|file| {
        file.tags
            .album_artist
            .as_ref()
            .or(file.tags.artist.as_ref())
    }));
    let album = most_common(files.iter().map(|file| file.tags.album.as_ref()));
    let (Some(artist), Some(album)) = (artist, album) else {
        return Ok(nothing(
            "A release is looked up by its artist and album name, and these files have none."
                .to_owned(),
        ));
    };
    let token = keys(service)?.discogs_token;
    let found = (|| -> Result<Option<Release>, String> {
        let candidates = parse_search(
            &lookups
                .musicbrainz
                .get(&search_url(&artist, &album), None)?,
        )
        .map_err(|error| error.to_string())?;
        let Some(best) = pick(&candidates, files.len()) else {
            return Ok(None);
        };
        let release = parse_release(&lookups.musicbrainz.get(&release_url(&best.id), None)?)
            .map_err(|error| error.to_string())?;
        Ok(Some(release))
    })();
    match (found, token) {
        (Ok(Some(mut release)), token) => {
            if let Some(token) = token {
                // The style is a nicety: a Discogs failure leaves MusicBrainz's answer as it is.
                if let Ok(Some(more)) =
                    discogs_find(&lookups.discogs, &token, &artist, &album, &files)
                {
                    release.genre = more.genre;
                }
            }
            Ok(compare("MusicBrainz", &release, &files))
        }
        (Ok(None) | Err(_), Some(token)) => {
            match discogs_find(&lookups.discogs, &token, &artist, &album, &files)? {
                Some(release) => Ok(compare("Discogs", &release, &files)),
                None => Ok(nothing(format!(
                    "Neither MusicBrainz nor Discogs has a release that matches \"{album}\" by \
                     {artist} closely enough."
                ))),
            }
        }
        (Ok(None), None) => Ok(nothing(format!(
            "MusicBrainz has no release that matches \"{album}\" by {artist} closely enough. \
             With a Discogs token in Settings, Discogs is asked too."
        ))),
        (Err(error), None) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE: &str = r#"{
        "id": "0b1d1c3e-0000-4000-8000-000000000001", "title": "Between Spaces", "date": "2019-05-03",
        "artist-credit": [{"name": "Studio 001", "joinphrase": ""}],
        "label-info": [{"label": {"name": "Test Label"}}],
        "media": [{"position": 1, "tracks": [
            {"position": 1, "title": "Night Drive", "length": 225000, "artist-credit": [{"name": "Studio 001", "joinphrase": ""}]},
            {"position": 2, "title": "Soft Focus", "length": 301000, "artist-credit": [{"name": "Studio 001", "joinphrase": ""}]}
        ]}]
    }"#;

    fn file(source: &str, title: &str, track: Option<u32>) -> FileFacts {
        FileFacts {
            source: source.into(),
            tags: TagInfo {
                artist: Some("Studio 001".into()),
                title: Some(title.into()),
                album: Some("Between Spaces".into()),
                track_number: track,
                ..TagInfo::default()
            },
            duration_ms: 0,
        }
    }

    #[test]
    fn suggestions_name_each_field_with_what_it_was_and_would_be() {
        let release = parse_release(RELEASE).unwrap();
        let found = compare(
            "MusicBrainz",
            &release,
            &[
                file("R/a.flac", "Night Drive", Some(1)),
                file("R/b.flac", "soft focus", None),
            ],
        );
        let about = found.found.as_ref().unwrap();
        assert_eq!(
            (about.year, about.tracks, about.url.as_str()),
            (
                Some(2019),
                2,
                "https://musicbrainz.org/release/0b1d1c3e-0000-4000-8000-000000000001"
            )
        );
        let second = &found.files[1];
        assert_eq!(second.matched_by, Some("title"));
        let names: Vec<_> = second
            .changes
            .iter()
            .map(|c| (c.field, c.from.as_deref(), c.to.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("Title", Some("soft focus"), "Soft Focus"),
                ("Album artist", None, "Studio 001"),
                ("Track", None, "2"),
                ("Year", None, "2019"),
                ("Label", None, "Test Label"),
            ]
        );
    }

    #[test]
    fn a_key_is_never_handed_back_whole_and_can_be_cleared() {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let lookups = Lookups::default();
        let empty = keys_status(&service).unwrap();
        assert!(!empty.discogs.saved && !empty.acoustid.saved);
        let saved = set_key(
            &service,
            &lookups,
            "acoustid",
            Some("  abcdEFGH1234 ".into()),
        )
        .unwrap();
        assert_eq!(
            saved.acoustid,
            KeyDto {
                saved: true,
                ending: Some("1234".into())
            }
        );
        let text = serde_json::to_string(&saved).unwrap();
        assert!(!text.contains("abcdEFGH"), "{text}");
        // A short key shows no ending at all: four of its few characters would give it away.
        let short = set_key(&service, &lookups, "acoustid", Some("ab12".into())).unwrap();
        assert_eq!(
            short.acoustid,
            KeyDto {
                saved: true,
                ending: None
            }
        );
        let cleared = set_key(&service, &lookups, "acoustid", Some("   ".into())).unwrap();
        assert!(!cleared.acoustid.saved);
        assert!(set_key(&service, &lookups, "bandcamp", Some("x".into())).is_err());
    }

    #[test]
    fn a_release_matches_when_half_the_files_find_a_track() {
        let release = parse_release(RELEASE).unwrap();
        let night = file("R/a.flac", "Night Drive", Some(1));
        let other = file("R/c.flac", "Something Else", Some(9));
        assert!(matches_enough(&release, &[night.clone(), other.clone()]));
        assert!(!matches_enough(&release, &[other.clone(), other]));
        assert!(!matches_enough(&release, &[]));
    }

    #[test]
    fn the_release_is_named_by_what_most_files_say() {
        let a = "Between Spaces".to_owned();
        let b = "Between Spaces (Bonus)".to_owned();
        assert_eq!(
            most_common([Some(&a), Some(&b), Some(&a), None].into_iter()).as_deref(),
            Some("Between Spaces")
        );
        assert_eq!(most_common([None, None].into_iter()), None);
    }

    #[test]
    fn requests_are_spaced_a_second_apart() {
        let client = Throttled::new("MusicBrainz");
        let start = Instant::now();
        client.wait_turn();
        client.wait_turn();
        assert!(start.elapsed() >= SPACING, "{:?}", start.elapsed());
    }

    /// Asks the real MusicBrainz about a well-known public release (two requests). Run by hand:
    /// `cargo test -p opencrate-desktop live_lookup -- --ignored`.
    #[test]
    #[ignore = "uses the network"]
    fn live_lookup() {
        use std::fs;

        use oc_tag::fixtures::{aiff, write_tags};

        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let area = tempfile::tempdir().unwrap();
        let release = area.path().join("Downloads/Discovery");
        fs::create_dir_all(&release).unwrap();
        fs::create_dir_all(area.path().join("Music")).unwrap();
        for (number, title) in [(1, "one more time"), (2, "Aerodynamic")] {
            let path = release.join(format!("{number:02} - {title}.aif"));
            fs::write(&path, aiff(1)).unwrap();
            write_tags(
                &path,
                &TagInfo {
                    artist: Some("Daft Punk".into()),
                    album: Some("Discovery".into()),
                    title: Some(title.into()),
                    track_number: Some(number),
                    ..TagInfo::default()
                },
            );
        }
        let folder = service
            .add_watch_folder(&area.path().join("Downloads"), "Downloads")
            .unwrap();
        let root = service
            .add_root(&area.path().join("Music"), "Music")
            .unwrap();
        let found = lookup_release(
            &service,
            &Lookups::default(),
            folder,
            "Discovery",
            root,
            ImportOptions::default(),
        )
        .unwrap();
        eprintln!("{found:#?}");
        assert_eq!(found.found.as_ref().unwrap().title, "Discovery");
        assert!(found.files[0]
            .changes
            .iter()
            .any(|change| change.field == "Title" && change.to == "One More Time"));
    }
}
