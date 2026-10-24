//! Looking a release up in MusicBrainz or Discogs, without any network code: the URLs to ask, reading the
//! answers, and matching the release's tracks to the files of a download. The caller fetches.
//!
//! Decision 0002: local first, then one release lookup per release (not per track). MusicBrainz
//! allows about one request per second per address and asks for a User-Agent naming the app.

mod discogs;
mod matching;
mod musicbrainz;

pub use discogs::{
    discogs_identity_url, discogs_release_url, discogs_search_url, parse_discogs_release,
    parse_discogs_search, DISCOGS_BASE,
};

pub use matching::{normalized, suggest, FileFacts, Suggestion};
pub use musicbrainz::{
    parse_release, parse_search, pick, release_url, search_url, Candidate, Medium, Release, Track,
    BASE, USER_AGENT,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("The lookup service sent an answer that could not be read: {0}")]
    Unreadable(String),
}
