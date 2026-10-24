//! Waveforms coloured by sound. The overview (whole track) is stored in the library; the
//! close-up (100 slices a second) is made when a track is played and kept in memory for the
//! last few, so the overview and the close-up share one decode.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use oc_core_db::service::LibraryService;
use oc_player::Bands;

/// Overview slices across the whole track; stored as low, mid and high per slice.
pub const OVERVIEW: usize = 800;
/// Close-ups kept in memory.
const KEEP: usize = 4;

type Key = (PathBuf, u64, Option<SystemTime>);

/// The stored overview's shape: 800 slices of low, mid and high. Bump when the shape changes.
pub const OVERVIEW_FORMAT: i64 = 1;

#[derive(Default)]
struct Inner {
    entries: VecDeque<(Key, Arc<Bands>)>,
    /// Tracks being decoded right now; a second request for one waits for it.
    decoding: Vec<Key>,
}

#[derive(Default)]
pub struct DetailCache {
    inner: Mutex<Inner>,
    finished: std::sync::Condvar,
}

impl DetailCache {
    /// The close-up for `path` while the file is unchanged, made with `compute` otherwise.
    /// Different tracks decode at the same time; a track already being decoded is waited for.
    pub fn get_with(
        &self,
        path: &Path,
        compute: impl FnOnce(&Path) -> Option<Bands>,
    ) -> Option<Arc<Bands>> {
        let metadata = std::fs::metadata(path).ok()?;
        let key = (path.to_path_buf(), metadata.len(), metadata.modified().ok());
        {
            let mut inner = self.lock();
            loop {
                if let Some(found) = Self::find(&inner, &key) {
                    return Some(found);
                }
                if !inner.decoding.contains(&key) {
                    inner.decoding.push(key.clone());
                    break;
                }
                inner = self
                    .finished
                    .wait(inner)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        let made = compute(path).map(Arc::new);
        let mut inner = self.lock();
        inner.decoding.retain(|decoding| *decoding != key);
        if let Some(made) = &made {
            inner.entries.push_front((key, Arc::clone(made)));
            inner.entries.truncate(KEEP);
        }
        drop(inner);
        self.finished.notify_all();
        made
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn find(inner: &Inner, key: &Key) -> Option<Arc<Bands>> {
        inner
            .entries
            .iter()
            .find(|(found, _)| found == key)
            .map(|(_, bands)| Arc::clone(bands))
    }

    pub fn get(&self, path: &Path) -> Option<Arc<Bands>> {
        self.get_with(path, |path| oc_player::bands(path).ok())
    }
}

/// The stored overview, made from the close-up when it is missing or stale.
pub fn overview(
    service: &LibraryService,
    cache: &DetailCache,
    track_id: i64,
) -> Result<Option<Vec<u8>>, String> {
    service
        .waveform(track_id, OVERVIEW_FORMAT, |path| {
            cache
                .get(path)
                .map(|detail| oc_player::overview(&detail, OVERVIEW))
                .filter(|values| !values.is_empty())
        })
        .map_err(|error| error.to_string())
}

/// The close-up as bytes for the page: slices a second (two bytes, little-endian), then low,
/// mid and high per slice. Empty when the track has no reachable file. Bytes, not a list of
/// numbers, because a long track has hundreds of thousands of them.
pub fn detail(
    service: &LibraryService,
    cache: &DetailCache,
    track_id: i64,
) -> Result<Vec<u8>, String> {
    let Some(path) = service
        .track_file(track_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(Vec::new());
    };
    let Some(bands) = cache.get(&path) else {
        return Ok(Vec::new());
    };
    let rate = u16::try_from(bands.per_second).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity(2 + bands.slices.len() * 3);
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend(bands.slices.iter().flatten());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn made(level: u8) -> Bands {
        Bands {
            per_second: 100,
            slices: vec![[level, 0, 0]; 3],
        }
    }

    #[test]
    fn a_close_up_is_made_once_while_the_file_is_unchanged() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.aif");
        std::fs::write(&path, b"one").unwrap();
        let cache = DetailCache::default();
        let count = Cell::new(0);
        let compute = |_: &Path| {
            count.set(count.get() + 1);
            Some(made(count.get()))
        };
        assert_eq!(cache.get_with(&path, compute).unwrap().slices[0][0], 1);
        assert_eq!(cache.get_with(&path, compute).unwrap().slices[0][0], 1);
        assert_eq!(count.get(), 1);
        std::fs::write(&path, b"longer").unwrap();
        assert_eq!(
            cache.get_with(&path, compute).unwrap().slices[0][0],
            2,
            "a changed file is made again"
        );
        assert!(cache
            .get_with(&folder.path().join("missing.aif"), compute)
            .is_none());
    }

    #[test]
    fn another_track_is_not_held_up_by_a_decode_in_progress() {
        use std::sync::mpsc;
        use std::time::Duration;
        let folder = tempfile::tempdir().unwrap();
        let (a, b) = (folder.path().join("a.aif"), folder.path().join("b.aif"));
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let cache = Arc::new(DetailCache::default());
        let (release, held) = mpsc::channel::<()>();
        let slow = Arc::clone(&cache);
        let slow_a = a.clone();
        let decoding_a = std::thread::spawn(move || {
            slow.get_with(&slow_a, |_| {
                held.recv().unwrap();
                Some(made(1))
            })
        });
        std::thread::sleep(Duration::from_millis(50));
        // B is a different track: it must not wait for A's decode.
        let started = std::time::Instant::now();
        assert!(cache.get_with(&b, |_| Some(made(2))).is_some());
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "B waited for A"
        );
        // A second request for A waits for the one in progress and shares its result.
        let same = Arc::clone(&cache);
        let same_a = a.clone();
        let waiting_a = std::thread::spawn(move || same.get_with(&same_a, |_| Some(made(9))));
        std::thread::sleep(Duration::from_millis(50));
        release.send(()).unwrap();
        let first = decoding_a.join().unwrap().unwrap();
        let second = waiting_a.join().unwrap().unwrap();
        assert!(Arc::ptr_eq(&first, &second), "decoded once, shared");
        assert_eq!(first.slices[0][0], 1);
    }

    #[test]
    fn only_the_last_few_are_kept() {
        let folder = tempfile::tempdir().unwrap();
        let cache = DetailCache::default();
        let paths: Vec<PathBuf> = (0..=KEEP)
            .map(|index| {
                let path = folder.path().join(format!("{index}.aif"));
                std::fs::write(&path, b"x").unwrap();
                cache.get_with(&path, |_| Some(made(1))).unwrap();
                path
            })
            .collect();
        let count = Cell::new(0);
        let again = |_: &Path| {
            count.set(count.get() + 1);
            Some(made(2))
        };
        cache.get_with(&paths[KEEP], again).unwrap();
        assert_eq!(count.get(), 0, "the newest is kept");
        cache.get_with(&paths[0], again).unwrap();
        assert_eq!(count.get(), 1, "the oldest was let go");
    }

    #[test]
    fn a_track_file_gives_a_stored_overview_and_a_close_up() {
        let music = tempfile::tempdir().unwrap();
        let file = music.path().join("A/R/01 - Night Drive.aif");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, oc_tag::fixtures::aiff(2)).unwrap();
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let root = service.add_root(music.path(), "Music").unwrap();
        service
            .scan_root(root, &oc_core_db::service::ScanOptions::default())
            .unwrap();
        let track = service
            .read_page(
                &oc_core_db::library::LibraryQuery::new(oc_core_db::library::View::Tracks),
                0,
                1,
            )
            .unwrap()
            .items[0]
            .id;
        let cache = DetailCache::default();
        let stored = overview(&service, &cache, track).unwrap().unwrap();
        assert_eq!(stored.len(), OVERVIEW * 3);
        let close = detail(&service, &cache, track).unwrap();
        assert_eq!(u16::from_le_bytes([close[0], close[1]]), 300);
        let values = close.len() - 2;
        assert_eq!(values % 3, 0);
        assert!((1797..=1803).contains(&values), "two seconds: {values}");
        assert_eq!(detail(&service, &cache, 999_999).unwrap(), Vec::<u8>::new());
    }
}
