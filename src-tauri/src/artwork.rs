//! Album art for the window. The page asks for `artwork://localhost/<release id>` (or
//! `http://artwork.localhost/<release id>` on Windows); the answer is the cover found for that
//! release in its own files, or a 404 that the page turns into a drawn cover. The request carries
//! only a number, never a path, and only files the index lists are opened, read only. The editor
//! also asks for `/track-<track id>` (the picture in that track's first file) and `/picked` (an
//! image the owner chose and has not saved yet).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use oc_core_db::service::LibraryService;
use oc_tag::{embedded_cover, folder_cover, Cover};
use tauri::http::{header, Response, StatusCode};

/// How many files of a release are tried for an embedded picture before looking beside them.
const FILES_TRIED: usize = 6;
/// Covers kept in memory, then the cache starts over. Decoded size is the window's business.
const MAX_ENTRIES: usize = 300;
/// A release found to have no cover is not searched for again this soon, because a scan or an
/// import may have added one.
const MISS_TTL: Duration = Duration::from_secs(30);

enum Entry {
    Found(Arc<Cover>),
    Missing(Instant),
}

enum Lookup {
    Found(Arc<Cover>),
    KnownMissing,
    Unknown,
}

#[derive(Default)]
pub struct ArtCache {
    entries: Mutex<HashMap<i64, Entry>>,
    /// An image the owner picked in the editor, held until it is saved or another is picked.
    picked: Mutex<Option<Arc<Cover>>>,
}

impl ArtCache {
    fn lookup(&self, release_id: i64) -> Lookup {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        match entries.get(&release_id) {
            Some(Entry::Found(cover)) => Lookup::Found(Arc::clone(cover)),
            Some(Entry::Missing(at)) if at.elapsed() < MISS_TTL => Lookup::KnownMissing,
            _ => Lookup::Unknown,
        }
    }

    /// Forget a release after its files changed, so the next request reads them again.
    pub fn forget(&self, release_id: i64) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.remove(&release_id);
    }

    /// Hold a picked image until the editor saves it.
    pub fn pick(&self, cover: Cover) {
        *self.picked.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(cover));
    }

    /// The image picked last, if any.
    pub fn picked(&self) -> Option<Arc<Cover>> {
        self.picked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn store(&self, release_id: i64, cover: Option<Arc<Cover>>) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() >= MAX_ENTRIES {
            entries.clear();
        }
        entries.insert(
            release_id,
            cover.map_or_else(|| Entry::Missing(Instant::now()), Entry::Found),
        );
    }
}

/// The cover for a release: embedded in one of its files, else an image beside the first.
pub fn cover_for(
    service: &LibraryService,
    cache: &ArtCache,
    release_id: i64,
) -> Option<Arc<Cover>> {
    match cache.lookup(release_id) {
        Lookup::Found(cover) => return Some(cover),
        Lookup::KnownMissing => return None,
        Lookup::Unknown => {}
    }
    let files = service.release_files(release_id, FILES_TRIED).ok()?;
    let found = files
        .iter()
        .find_map(|path| embedded_cover(path))
        .or_else(|| {
            files
                .first()
                .and_then(|path| path.parent())
                .and_then(folder_cover)
        })
        .map(Arc::new);
    cache.store(release_id, found.clone());
    found
}

/// How many of an artist's releases are looked at for covers, and how many covers are shown.
const ARTIST_RELEASES_TRIED: usize = 12;
pub const ARTIST_COVERS: usize = 4;

/// Up to four of an artist's releases that have a cover, newest first, for a collage.
pub fn artist_covers(service: &LibraryService, cache: &ArtCache, artist_id: i64) -> Vec<i64> {
    service
        .artist_release_ids(artist_id, ARTIST_RELEASES_TRIED)
        .unwrap_or_default()
        .into_iter()
        .filter(|release| cover_for(service, cache, *release).is_some())
        .take(ARTIST_COVERS)
        .collect()
}

/// The release id in `/<id>`, and nothing else.
fn parse(path: &str) -> Option<i64> {
    number(path.strip_prefix('/')?)
}

fn number(id: &str) -> Option<i64> {
    if id.is_empty() || id.len() > 18 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}

fn plain(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Vec::new())
        .unwrap_or_default()
}

pub fn respond(
    service: Option<&LibraryService>,
    cache: &ArtCache,
    path: &str,
) -> Response<Vec<u8>> {
    let Some(service) = service else {
        return plain(StatusCode::NOT_FOUND);
    };
    // A track's picture and a picked one change while the editor is open, so neither is kept.
    let (cover, keep) = if path == "/picked" {
        (cache.picked(), false)
    } else if let Some(track_id) = path.strip_prefix("/track-").and_then(number) {
        (
            crate::art_fix::track_cover(service, track_id).map(Arc::new),
            false,
        )
    } else {
        match parse(path) {
            Some(release_id) => (cover_for(service, cache, release_id), true),
            None => (None, false),
        }
    };
    let Some(cover) = cover else {
        return plain(StatusCode::NOT_FOUND);
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, cover.mime)
        // The page sniffs nothing: the type above came from the picture's own bytes.
        .header("X-Content-Type-Options", "nosniff")
        .header(
            header::CACHE_CONTROL,
            if keep {
                "private, max-age=300"
            } else {
                "no-store"
            },
        )
        .body(cover.data.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use oc_tag::fixtures::{aiff, write_tags, write_tags_and_cover};
    use oc_tag::TagInfo;

    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];

    fn library() -> (tempfile::TempDir, LibraryService, tempfile::TempDir, i64) {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let music = tempfile::tempdir().unwrap();
        for (name, track) in [("a.aif", 1), ("b.aif", 2)] {
            let path = music.path().join("Studio/Between Spaces").join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, aiff(1)).unwrap();
            write_tags(
                &path,
                &TagInfo {
                    title: Some(format!("Track {track}")),
                    artist: Some("Studio".into()),
                    album: Some("Between Spaces".into()),
                    track_number: Some(track),
                    ..TagInfo::default()
                },
            );
        }
        let root = service.add_root(music.path(), "Music").unwrap();
        let summary = service
            .scan_root(root, &oc_core_db::service::ScanOptions::default())
            .unwrap();
        assert_eq!(summary.catalog.tracks_created, 2);
        let release = service
            .read_page(
                &oc_core_db::library::LibraryQuery::new(oc_core_db::library::View::Releases),
                0,
                5,
            )
            .unwrap()
            .items[0]
            .id;
        (data, service, music, release)
    }

    #[test]
    fn a_release_without_a_cover_is_a_404_and_one_with_a_folder_image_is_served() {
        let (_data, service, music, release) = library();
        let cache = ArtCache::default();
        let path = format!("/{release}");
        assert_eq!(
            respond(Some(&service), &cache, &path).status(),
            StatusCode::NOT_FOUND
        );
        // The miss is remembered briefly, so the page asking again costs nothing.
        fs::write(music.path().join("Studio/Between Spaces/cover.png"), PNG).unwrap();
        assert_eq!(
            respond(Some(&service), &cache, &path).status(),
            StatusCode::NOT_FOUND
        );
        let fresh = ArtCache::default();
        let served = respond(Some(&service), &fresh, &path);
        assert_eq!(served.status(), StatusCode::OK);
        assert_eq!(served.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(served.headers()["X-Content-Type-Options"], "nosniff");
        assert_eq!(served.body(), PNG);
        // Now it is cached, so the answer no longer depends on the disk.
        fs::remove_file(music.path().join("Studio/Between Spaces/cover.png")).unwrap();
        assert_eq!(respond(Some(&service), &fresh, &path).body(), PNG);
    }

    #[test]
    fn only_a_plain_release_number_is_accepted_and_no_file_is_changed() {
        let (_data, service, music, release) = library();
        let cache = ArtCache::default();
        let before = fs::read_dir(music.path().join("Studio/Between Spaces"))
            .unwrap()
            .count();
        for bad in [
            "",
            "/",
            "/release/1",
            "/abc",
            "/-1",
            "/1/../2",
            "/1.png",
            "/99999999999999999999",
            "/etc/passwd",
            "/%2e%2e",
            "1",
        ] {
            assert_eq!(
                respond(Some(&service), &cache, bad).status(),
                StatusCode::NOT_FOUND,
                "{bad:?}"
            );
        }
        assert_eq!(
            respond(Some(&service), &cache, "/999999").status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            respond(None, &cache, &format!("/{release}")).status(),
            StatusCode::NOT_FOUND,
            "no library, no art"
        );
        assert_eq!(
            fs::read_dir(music.path().join("Studio/Between Spaces"))
                .unwrap()
                .count(),
            before
        );
    }

    #[test]
    fn an_embedded_picture_wins_over_a_folder_image() {
        let (_data, service, music, release) = library();
        let folder = music.path().join("Studio/Between Spaces");
        fs::write(folder.join("cover.png"), [PNG, &[9]].concat()).unwrap();
        write_tags_and_cover(
            &folder.join("a.aif"),
            &TagInfo {
                title: Some("Track 1".into()),
                artist: Some("Studio".into()),
                album: Some("Between Spaces".into()),
                track_number: Some(1),
                ..TagInfo::default()
            },
            Some(PNG),
        );
        let cache = ArtCache::default();
        let served = respond(Some(&service), &cache, &format!("/{release}"));
        assert_eq!(served.body(), PNG);
    }

    #[test]
    fn an_artist_collage_uses_only_releases_that_have_a_cover() {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let music = tempfile::tempdir().unwrap();
        for release in 0..6 {
            let folder = music.path().join(format!("Studio/Release {release}"));
            fs::create_dir_all(&folder).unwrap();
            let path = folder.join("01.aif");
            fs::write(&path, aiff(1)).unwrap();
            write_tags(
                &path,
                &TagInfo {
                    title: Some("Track".into()),
                    artist: Some("Studio".into()),
                    album: Some(format!("Release {release}")),
                    track_number: Some(1),
                    ..TagInfo::default()
                },
            );
            // Releases 1 and 3 have no cover.
            if release != 1 && release != 3 {
                fs::write(folder.join("cover.png"), PNG).unwrap();
            }
        }
        let root = service.add_root(music.path(), "Music").unwrap();
        service
            .scan_root(root, &oc_core_db::service::ScanOptions::default())
            .unwrap();
        let artists = service
            .read_page(
                &oc_core_db::library::LibraryQuery::new(oc_core_db::library::View::Artists),
                0,
                5,
            )
            .unwrap();
        let artist = artists.items[0].id;
        let cache = ArtCache::default();
        let covers = artist_covers(&service, &cache, artist);
        assert_eq!(covers.len(), ARTIST_COVERS, "four of the five with covers");
        for release in &covers {
            assert_eq!(
                respond(Some(&service), &cache, &format!("/{release}")).status(),
                StatusCode::OK
            );
        }
        assert_eq!(artist_covers(&service, &cache, 9_999).len(), 0);
    }
}
