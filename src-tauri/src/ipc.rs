//! The messages between the UI and the library service. Strict on the way in (unknown
//! fields are refused) and camelCase on the way out, matching `src/library/types.ts`.

use oc_core_db::library::{Format, LibraryItem, LibraryPage, LibraryQuery, Sort, View, MAX_WINDOW};
use oc_core_db::service::LibraryService;
use serde::{Deserialize, Serialize};

/// Longer text cannot be a useful search; refuse it before it reaches the database.
const MAX_SEARCH_CHARS: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ViewDto {
    Tracks,
    Releases,
    Artists,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDto {
    Recent,
    Title,
    Artist,
    Bpm,
    Playlist,
    Crate,
    Custom,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum FormatDto {
    Aiff,
    Mp3,
    Flac,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryDto {
    pub view: ViewDto,
    pub search: String,
    pub sort: SortDto,
    pub format: Option<FormatDto>,
    pub artist_id: Option<i64>,
    pub release_id: Option<i64>,
    pub playlist_id: Option<i64>,
    #[serde(default)]
    pub crate_id: Option<i64>,
}

impl SortDto {
    pub const fn to_sort(self) -> Sort {
        match self {
            Self::Recent => Sort::Recent,
            Self::Title => Sort::Title,
            Self::Artist => Sort::Artist,
            Self::Bpm => Sort::Bpm,
            Self::Playlist => Sort::Playlist,
            Self::Crate => Sort::Crate,
            Self::Custom => Sort::Custom,
        }
    }
}

impl QueryDto {
    pub(crate) fn to_query(&self) -> LibraryQuery {
        let mut query = LibraryQuery::new(match self.view {
            ViewDto::Tracks => View::Tracks,
            ViewDto::Releases => View::Releases,
            ViewDto::Artists => View::Artists,
        });
        query.search.clone_from(&self.search);
        query.sort = self.sort.to_sort();
        query.format = self.format.map(|format| match format {
            FormatDto::Aiff => Format::Aiff,
            FormatDto::Mp3 => Format::Mp3,
            FormatDto::Flac => Format::Flac,
        });
        query.artist_id = self.artist_id;
        query.release_id = self.release_id;
        query.playlist_id = self.playlist_id;
        query.crate_id = self.crate_id;
        query
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    pub id: i64,
    pub kind: ViewDto,
    pub title: String,
    pub artist: String,
    pub artist_id: i64,
    pub release: String,
    pub release_id: i64,
    pub bpm: Option<f64>,
    pub musical_key: String,
    pub duration_ms: i64,
    pub format: Option<String>,
    pub track_count: i64,
    pub missing: bool,
    pub analysed: bool,
    pub rekordbox_key: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDto {
    pub items: Vec<ItemDto>,
    pub total: u64,
    pub start: usize,
}

impl From<LibraryItem> for ItemDto {
    fn from(item: LibraryItem) -> Self {
        Self {
            id: item.id,
            kind: match item.kind {
                View::Tracks => ViewDto::Tracks,
                View::Releases => ViewDto::Releases,
                View::Artists => ViewDto::Artists,
            },
            title: item.title,
            artist: item.artist,
            artist_id: item.artist_id,
            release: item.release,
            release_id: item.release_id,
            bpm: item.bpm,
            musical_key: item.musical_key,
            duration_ms: item.duration_ms,
            format: item.format,
            track_count: item.track_count,
            missing: item.missing,
            analysed: item.analysed,
            rekordbox_key: item.rekordbox_key,
        }
    }
}

impl From<LibraryPage> for PageDto {
    fn from(page: LibraryPage) -> Self {
        Self {
            items: page.items.into_iter().map(ItemDto::from).collect(),
            total: page.total,
            start: page.start,
        }
    }
}

/// Run one library read for the UI. Errors are plain text the UI can show.
pub fn read(
    service: &LibraryService,
    query: &QueryDto,
    start: usize,
    count: usize,
) -> Result<PageDto, String> {
    if query.search.chars().count() > MAX_SEARCH_CHARS {
        return Err(format!(
            "Search text is limited to {MAX_SEARCH_CHARS} characters."
        ));
    }
    if count == 0 || count > MAX_WINDOW {
        return Err(format!(
            "Request a non-negative start and 1-{MAX_WINDOW} rows."
        ));
    }
    service
        .read_page(&query.to_query(), start, count)
        .map(PageDto::from)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    fn query(value: serde_json::Value) -> Result<QueryDto, serde_json::Error> {
        serde_json::from_value(value)
    }

    fn base() -> serde_json::Value {
        json!({"view": "tracks", "search": "", "sort": "recent", "format": null,
               "artistId": null, "releaseId": null, "playlistId": null})
    }

    #[test]
    fn the_query_the_ui_sends_is_understood() {
        let sent = query(json!({"view": "releases", "search": "night", "sort": "bpm",
                                "format": "AIFF", "artistId": 4, "releaseId": null,
                                "playlistId": 9}))
        .unwrap();
        let mapped = sent.to_query();
        assert_eq!(mapped.view, View::Releases);
        assert_eq!(mapped.sort, Sort::Bpm);
        assert_eq!(mapped.format, Some(Format::Aiff));
        assert_eq!(
            (mapped.search.as_str(), mapped.artist_id, mapped.release_id),
            ("night", Some(4), None)
        );
        assert_eq!(mapped.playlist_id, Some(9));
        let mut playlist = base();
        playlist["sort"] = json!("playlist");
        assert_eq!(query(playlist).unwrap().to_query().sort, Sort::Playlist);
        for (text, format) in [("MP3", Format::Mp3), ("FLAC", Format::Flac)] {
            let mut value = base();
            value["format"] = json!(text);
            assert_eq!(query(value).unwrap().to_query().format, Some(format));
        }
    }

    #[test]
    fn anything_else_is_refused() {
        let mut extra = base();
        extra["path"] = json!("/Users/someone/Music");
        assert!(query(extra).is_err(), "unknown fields are refused");
        for (field, value) in [
            ("view", json!("playlists")),
            ("sort", json!("RECENT")),
            ("format", json!("all")),
            ("format", json!("aiff")),
            ("artistId", json!("3")),
            ("search", json!(5)),
        ] {
            let mut bad = base();
            bad[field] = value;
            assert!(query(bad).is_err(), "{field}");
        }
        let mut missing = base();
        missing.as_object_mut().unwrap().remove("sort");
        assert!(query(missing).is_err());
    }

    #[test]
    fn a_page_goes_out_with_the_names_the_ui_expects() {
        let directory = tempdir().unwrap();
        let service = LibraryService::open(directory.path(), 2).unwrap();
        service.seed_demo(200).unwrap();
        let page = read(&service, &query(base()).unwrap(), 0, 3).unwrap();
        let value = serde_json::to_value(&page).unwrap();
        assert_eq!(
            (value["total"].as_u64(), value["start"].as_u64()),
            (Some(200), Some(0))
        );
        let first = &value["items"][0];
        for key in [
            "id",
            "kind",
            "title",
            "artist",
            "artistId",
            "release",
            "releaseId",
            "bpm",
            "musicalKey",
            "durationMs",
            "format",
            "trackCount",
            "missing",
            "analysed",
            "rekordboxKey",
        ] {
            assert!(first.get(key).is_some(), "missing {key}");
        }
        assert_eq!(first["kind"], "tracks");
        assert_eq!(first.as_object().unwrap().len(), 15, "no stray fields");
        assert!(first["format"].is_string());
        assert_eq!(value["items"].as_array().unwrap().len(), 3);
        service.shutdown().unwrap();
    }

    #[test]
    fn reading_searches_filters_and_refuses_bad_requests() {
        let directory = tempdir().unwrap();
        let service = LibraryService::open(directory.path(), 2).unwrap();
        service.seed_demo(300).unwrap();
        let mut search = base();
        search["search"] = json!("night drive");
        search["format"] = json!("MP3");
        let found = read(&service, &query(search).unwrap(), 0, 50).unwrap();
        assert!(
            found.total > 0
                && found
                    .items
                    .iter()
                    .all(|i| i.format.as_deref() == Some("MP3"))
        );
        let tracks = query(base()).unwrap();
        assert!(read(&service, &tracks, 0, 0).is_err());
        assert!(read(&service, &tracks, 0, MAX_WINDOW + 1).is_err());
        let mut long = base();
        long["search"] = json!("x".repeat(MAX_SEARCH_CHARS + 1));
        assert!(read(&service, &query(long).unwrap(), 0, 10)
            .unwrap_err()
            .contains("limited"));
        assert_eq!(read(&service, &tracks, 10_000, 10).unwrap().items.len(), 0);
        service.shutdown().unwrap();
    }
}
