//! Listing a download folder, read only. This is the one module here that touches the file
//! system, and it only reads directory entries and metadata.
//!
//! Symbolic links are not followed, so a link cannot lead the listing out of the folder or
//! into a loop. A subfolder that cannot be read is skipped and counted; it never fails the
//! whole listing. The listing stops at a fixed number of entries and says so.

use std::fs;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use oc_paths::os_str_to_bytes;

use crate::release::Entry;
use crate::stability::Observation;

#[derive(Debug, Default)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Folders that could not be read and were skipped.
    pub unreadable_dirs: usize,
    /// The entry limit was reached, so the list is incomplete and no conclusion about
    /// anything missing from it may be drawn.
    pub truncated: bool,
}

/// Deeper folders are not listed; a release is rarely more than a disc folder deep.
pub const MAX_DEPTH: usize = 6;
pub const MAX_ENTRIES: usize = 100_000;

fn millis(time: io::Result<std::time::SystemTime>) -> i64 {
    match time.map(|t| t.duration_since(UNIX_EPOCH)) {
        Ok(Ok(since)) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
        _ => 0,
    }
}

/// List every file below `root`. An error means the root itself cannot be read.
pub fn list_folder(root: &Path) -> io::Result<Listing> {
    let mut listing = Listing::default();
    let mut pending = vec![(root.to_path_buf(), Vec::<Vec<u8>>::new())];
    let mut first = Some(fs::read_dir(root)?);
    while let Some((directory, prefix)) = pending.pop() {
        let Ok(reader) = first.take().map_or_else(|| fs::read_dir(&directory), Ok) else {
            listing.unreadable_dirs += 1;
            continue;
        };
        for item in reader {
            let Ok(item) = item else {
                listing.unreadable_dirs += 1;
                continue;
            };
            let Ok(kind) = item.file_type() else {
                continue;
            };
            let mut components = prefix.clone();
            components.push(os_str_to_bytes(&item.file_name()));
            if kind.is_dir() {
                if components.len() < MAX_DEPTH {
                    pending.push((item.path(), components));
                }
            } else if kind.is_file() {
                if listing.entries.len() >= MAX_ENTRIES {
                    listing.truncated = true;
                    return Ok(listing);
                }
                let Ok(metadata) = item.metadata() else {
                    continue;
                };
                listing.entries.push(Entry {
                    components,
                    observation: Observation {
                        size: metadata.len(),
                        modified_ms: millis(metadata.modified()),
                    },
                });
            }
        }
    }
    Ok(listing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn names(listing: &Listing) -> Vec<String> {
        let mut found: Vec<String> = listing
            .entries
            .iter()
            .map(|entry| {
                entry
                    .components
                    .iter()
                    .map(|part| String::from_utf8_lossy(part).into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn files_are_listed_relative_to_the_folder_with_size_and_time() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Release/CD1/01.flac"), b"12345");
        write(&dir.path().join("loose.mp3"), b"1");
        let listing = list_folder(dir.path()).unwrap();
        assert_eq!(names(&listing), ["Release/CD1/01.flac", "loose.mp3"]);
        let flac = listing
            .entries
            .iter()
            .find(|entry| entry.components.len() == 3)
            .unwrap();
        assert_eq!(flac.observation.size, 5);
        assert!(flac.observation.modified_ms > 0);
        assert!(!listing.truncated && listing.unreadable_dirs == 0);
    }

    #[test]
    fn an_empty_folder_lists_nothing_and_a_missing_one_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(list_folder(dir.path()).unwrap().entries.len(), 0);
        assert!(list_folder(&dir.path().join("gone")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(&outside.path().join("secret.mp3"), b"x");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.mp3"), dir.path().join("a.mp3"))
            .unwrap();
        write(&dir.path().join("real.mp3"), b"x");
        assert_eq!(names(&list_folder(dir.path()).unwrap()), ["real.mp3"]);
    }

    #[test]
    fn folders_below_the_depth_limit_are_not_listed() {
        let dir = tempfile::tempdir().unwrap();
        let deep = (0..=MAX_DEPTH).fold(dir.path().to_path_buf(), |path, level| {
            path.join(format!("d{level}"))
        });
        write(&deep.join("deep.mp3"), b"x");
        write(&dir.path().join("d0/top.mp3"), b"x");
        assert_eq!(names(&list_folder(dir.path()).unwrap()), ["d0/top.mp3"]);
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_subfolder_is_skipped_and_counted() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("ok/a.mp3"), b"x");
        write(&dir.path().join("locked/b.mp3"), b"x");
        let locked = dir.path().join("locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let listing = list_folder(dir.path()).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if fs::read_dir(&locked).is_ok() && listing.unreadable_dirs == 0 {
            // Running as a user who can read everything (root); nothing to assert.
            return;
        }
        assert_eq!(names(&listing), ["ok/a.mp3"]);
        assert_eq!(listing.unreadable_dirs, 1);
    }

    /// Timing, not a correctness test: `cargo test -p oc-ingest --release -- --ignored --nocapture`.
    #[test]
    #[ignore = "benchmark"]
    fn listing_time_for_a_big_synthetic_folder() {
        let dir = tempfile::tempdir().unwrap();
        for release in 0..400 {
            for track in 0..12 {
                write(
                    &dir.path()
                        .join(format!("Release {release}/{track:02}.flac")),
                    b"x",
                );
            }
            write(
                &dir.path().join(format!("Release {release}/cover.jpg")),
                b"x",
            );
        }
        let start = std::time::Instant::now();
        let listing = list_folder(dir.path()).unwrap();
        eprintln!(
            "listed {} files in {:?}",
            listing.entries.len(),
            start.elapsed()
        );
    }
}
