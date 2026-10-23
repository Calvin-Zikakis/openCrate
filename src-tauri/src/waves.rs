//! Waveforms coloured by sound. The overview (whole track) is stored in the library; the
//! close-up (100 slices a second) is made when a track is played and kept in memory for the
//! last few, so the overview and the close-up share one decode.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use oc_core_db::service::LibraryService;
use oc_player::Bands;
use serde::Serialize;

/// Overview slices across the whole track; stored as low, mid and high per slice.
pub const OVERVIEW: usize = 800;
/// Close-ups kept in memory.
const KEEP: usize = 4;

type Key = (PathBuf, u64, Option<SystemTime>);

#[derive(Default)]
pub struct DetailCache {
    entries: Mutex<VecDeque<(Key, Arc<Bands>)>>,
    /// One decode at a time, so a second request for the same track waits and finds it.
    computing: Mutex<()>,
}

impl DetailCache {
    /// The close-up for `path` while the file is unchanged, made with `compute` otherwise.
    pub fn get_with(
        &self,
        path: &Path,
        compute: impl FnOnce(&Path) -> Option<Bands>,
    ) -> Option<Arc<Bands>> {
        let metadata = std::fs::metadata(path).ok()?;
        let key = (path.to_path_buf(), metadata.len(), metadata.modified().ok());
        let _turn = self
            .computing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(found) = self.find(&key) {
            return Some(found);
        }
        let made = Arc::new(compute(path)?);
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.push_front((key, Arc::clone(&made)));
        entries.truncate(KEEP);
        Some(made)
    }

    fn find(&self, key: &Key) -> Option<Arc<Bands>> {
        let entries = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries
            .iter()
            .find(|(found, _)| found == key)
            .map(|(_, bands)| Arc::clone(bands))
    }

    pub fn get(&self, path: &Path) -> Option<Arc<Bands>> {
        self.get_with(path, |path| oc_player::bands(path).ok())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailDto {
    pub per_second: u32,
    /// Low, mid and high per slice, 0 to 255.
    pub values: Vec<u8>,
}

/// The stored overview, made from the close-up when it is missing or stale.
pub fn overview(
    service: &LibraryService,
    cache: &DetailCache,
    track_id: i64,
) -> Result<Option<Vec<u8>>, String> {
    service
        .waveform(track_id, OVERVIEW * 3, |path| {
            cache
                .get(path)
                .map(|detail| oc_player::overview(&detail, OVERVIEW))
                .filter(|values| !values.is_empty())
        })
        .map_err(|error| error.to_string())
}

pub fn detail(
    service: &LibraryService,
    cache: &DetailCache,
    track_id: i64,
) -> Result<Option<DetailDto>, String> {
    let Some(path) = service
        .track_file(track_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    Ok(cache.get(&path).map(|bands| DetailDto {
        per_second: bands.per_second,
        values: bands.slices.iter().flatten().copied().collect(),
    }))
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
        let close = detail(&service, &cache, track).unwrap().unwrap();
        assert_eq!(close.per_second, 100);
        assert_eq!(close.values.len() % 3, 0);
        assert!(
            (597..=603).contains(&close.values.len()),
            "two seconds: {}",
            close.values.len()
        );
        assert_eq!(
            detail(&service, &cache, 999_999)
                .unwrap()
                .map(|d| d.per_second),
            None
        );
    }
}
