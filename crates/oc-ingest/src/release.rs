//! Group a watch folder's entries into releases and decide which are ready.
//!
//! A release is the first folder below the watch folder, however deeply its files are
//! nested (multi-disc releases keep a `CD1` folder). A file that sits directly in the download
//! folder is a release of its own. A release is ready when nothing in it looks unfinished and
//! every file that matters has stopped changing; until then the answer says what it waits for.

use std::collections::{BTreeMap, HashSet};

use crate::classify::{classify, path_has_partial, EntryKind};
use crate::stability::{Observation, Stability, StabilityTracker};

/// One file as the caller saw it. `components` are relative to the watch folder.
#[derive(Clone, Debug)]
pub struct Entry {
    pub components: Vec<Vec<u8>>,
    pub observation: Observation,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReleaseKey {
    /// The first folder below the watch folder.
    Folder(Vec<u8>),
    /// A file directly in the watch folder.
    Loose(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waiting {
    /// A temporary name is present somewhere in the release.
    Partial,
    /// Files are still changing; the value is the shortest time any has been unchanged.
    Changing { shortest_quiet_ms: i64 },
    /// Nothing importable yet (only artwork, notes or litter so far).
    NothingToImport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseState {
    Ready,
    Waiting(Waiting),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseStatus {
    pub key: ReleaseKey,
    pub state: ReleaseState,
    pub audio_files: usize,
    pub archives: usize,
    /// The latest modified time among the files that count (not litter), in milliseconds.
    /// A release whose files are all older than the day the folder was added was already there.
    pub newest_modified_ms: i64,
}

fn key_of(components: &[Vec<u8>]) -> Option<ReleaseKey> {
    match components {
        [] => None,
        [file] => Some(ReleaseKey::Loose(file.clone())),
        [folder, ..] => Some(ReleaseKey::Folder(folder.clone())),
    }
}

/// NUL cannot occur in a name on any supported system, so joining with it is unambiguous.
fn tracker_key(components: &[Vec<u8>]) -> Vec<u8> {
    components.join(&0u8)
}

/// Evaluate every release in one listing of a watch folder at `now_ms`. Every entry is
/// reported to `tracker` (litter and partial files too, which costs nothing and keeps the
/// bookkeeping uniform), and tracker entries for files that are gone are dropped. Releases
/// come back in a stable order.
pub fn evaluate(
    tracker: &mut StabilityTracker<Vec<u8>>,
    entries: &[Entry],
    now_ms: i64,
) -> Vec<ReleaseStatus> {
    struct Tally {
        audio: usize,
        archives: usize,
        partial: bool,
        shortest_quiet: Option<i64>,
        any_unstable: bool,
        newest_modified_ms: i64,
    }
    let mut releases: BTreeMap<ReleaseKey, Tally> = BTreeMap::new();
    let mut present = HashSet::new();
    for entry in entries {
        let Some(key) = key_of(&entry.components) else {
            continue;
        };
        let tally = releases.entry(key).or_insert(Tally {
            audio: 0,
            archives: 0,
            partial: false,
            shortest_quiet: None,
            any_unstable: false,
            newest_modified_ms: i64::MIN,
        });
        let name = tracker_key(&entry.components);
        present.insert(name.clone());
        let stability = tracker.observe(name, entry.observation, now_ms);
        if path_has_partial(&entry.components) {
            tally.partial = true;
            continue;
        }
        let kind = entry
            .components
            .last()
            .map_or(EntryKind::Ignored, |name| classify(name));
        match kind {
            EntryKind::Audio(_) => tally.audio += 1,
            EntryKind::Archive => tally.archives += 1,
            // A half-written cover still holds the release; a stray note (even an empty one) never does.
            EntryKind::Artwork => {}
            EntryKind::Ignored | EntryKind::Other | EntryKind::Partial => continue,
        }
        tally.newest_modified_ms = tally.newest_modified_ms.max(entry.observation.modified_ms);
        if let Stability::Settling { quiet_for_ms } = stability {
            tally.any_unstable = true;
            tally.shortest_quiet = Some(
                tally
                    .shortest_quiet
                    .map_or(quiet_for_ms, |shortest| shortest.min(quiet_for_ms)),
            );
        }
    }
    tracker.retain_present(&present);
    releases
        .into_iter()
        .map(|(key, tally)| {
            let state = if tally.partial {
                ReleaseState::Waiting(Waiting::Partial)
            } else if tally.any_unstable {
                ReleaseState::Waiting(Waiting::Changing {
                    shortest_quiet_ms: tally.shortest_quiet.unwrap_or(0),
                })
            } else if tally.audio + tally.archives == 0 {
                ReleaseState::Waiting(Waiting::NothingToImport)
            } else {
                ReleaseState::Ready
            };
            ReleaseStatus {
                key,
                state,
                audio_files: tally.audio,
                archives: tally.archives,
                newest_modified_ms: tally.newest_modified_ms,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stability::StabilityConfig;

    fn entry(path: &str, size: u64) -> Entry {
        Entry {
            components: path
                .split('/')
                .map(|part| part.as_bytes().to_vec())
                .collect(),
            observation: Observation {
                size,
                modified_ms: 1,
            },
        }
    }

    fn tracker() -> StabilityTracker<Vec<u8>> {
        StabilityTracker::new(StabilityConfig {
            quiet_ms: 1_000,
            min_sightings: 2,
            trust_old_mtime_ms: None,
        })
    }

    fn state_of(statuses: &[ReleaseStatus], folder: &str) -> ReleaseState {
        statuses
            .iter()
            .find(|status| status.key == ReleaseKey::Folder(folder.as_bytes().to_vec()))
            .map_or(ReleaseState::Waiting(Waiting::NothingToImport), |status| {
                status.state
            })
    }

    #[test]
    fn a_release_becomes_ready_once_every_file_has_settled() {
        let mut t = tracker();
        let listing = [
            entry("Between Spaces/01.flac", 100),
            entry("Between Spaces/02.flac", 100),
            entry("Between Spaces/cover.jpg", 10),
        ];
        let first = evaluate(&mut t, &listing, 0);
        assert!(matches!(
            state_of(&first, "Between Spaces"),
            ReleaseState::Waiting(Waiting::Changing {
                shortest_quiet_ms: 0
            })
        ));
        let second = evaluate(&mut t, &listing, 2_000);
        assert_eq!(state_of(&second, "Between Spaces"), ReleaseState::Ready);
        assert_eq!(second[0].audio_files, 2);
    }

    #[test]
    fn a_temporary_name_anywhere_holds_the_whole_release() {
        let mut t = tracker();
        let listing = [entry("R/01.flac", 100), entry("R/02.flac.part", 50)];
        evaluate(&mut t, &listing, 0);
        let later = evaluate(&mut t, &listing, 5_000);
        assert_eq!(
            state_of(&later, "R"),
            ReleaseState::Waiting(Waiting::Partial)
        );
        let finished = [entry("R/01.flac", 100), entry("R/02.flac", 100)];
        evaluate(&mut t, &finished, 6_000);
        assert_eq!(
            state_of(&evaluate(&mut t, &finished, 8_000), "R"),
            ReleaseState::Ready
        );
    }

    #[test]
    fn nested_disc_folders_belong_to_the_release_and_loose_files_stand_alone() {
        let mut t = tracker();
        let listing = [
            entry("Box/CD1/01.flac", 10),
            entry("Box/CD2/01.flac", 10),
            entry("single.mp3", 10),
            entry("pack.zip", 10),
        ];
        evaluate(&mut t, &listing, 0);
        let done = evaluate(&mut t, &listing, 2_000);
        assert_eq!(done.len(), 3);
        assert_eq!(state_of(&done, "Box"), ReleaseState::Ready);
        let box_release = done
            .iter()
            .find(|status| status.key == ReleaseKey::Folder(b"Box".to_vec()))
            .map(|status| status.audio_files);
        assert_eq!(box_release, Some(2));
        let zip = done
            .iter()
            .find(|status| status.key == ReleaseKey::Loose(b"pack.zip".to_vec()));
        assert_eq!(zip.map(|status| status.archives), Some(1));
        assert!(done
            .iter()
            .all(|status| status.state == ReleaseState::Ready));
    }

    #[test]
    fn a_folder_with_no_music_yet_is_not_ready_and_litter_does_not_count() {
        let mut t = tracker();
        let listing = [entry("Art/cover.jpg", 10), entry("Art/.DS_Store", 10)];
        evaluate(&mut t, &listing, 0);
        assert_eq!(
            state_of(&evaluate(&mut t, &listing, 5_000), "Art"),
            ReleaseState::Waiting(Waiting::NothingToImport)
        );
    }

    #[test]
    fn litter_that_keeps_changing_does_not_hold_a_finished_release() {
        let mut t = tracker();
        let mut listing = vec![entry("R/01.flac", 100), entry("R/.DS_Store", 1)];
        evaluate(&mut t, &listing, 0);
        listing[1] = entry("R/.DS_Store", 2);
        let later = evaluate(&mut t, &listing, 2_000);
        assert_eq!(state_of(&later, "R"), ReleaseState::Ready);
    }

    #[test]
    fn the_newest_modified_time_ignores_litter_and_notes() {
        let mut t = tracker();
        let at = |path: &str, modified_ms: i64| Entry {
            components: path
                .split('/')
                .map(|part| part.as_bytes().to_vec())
                .collect(),
            observation: Observation {
                size: 5,
                modified_ms,
            },
        };
        let listing = [
            at("R/01.flac", 1_000),
            at("R/cover.jpg", 3_000),
            at("R/.DS_Store", 9_000),
            at("R/notes.txt", 8_000),
        ];
        let statuses = evaluate(&mut t, &listing, 0);
        assert_eq!(statuses[0].newest_modified_ms, 3_000);
    }

    #[test]
    fn an_empty_note_does_not_hold_a_finished_release() {
        let mut t = tracker();
        let listing = [entry("R/01.flac", 100), entry("R/notes.txt", 0)];
        evaluate(&mut t, &listing, 0);
        assert_eq!(
            state_of(&evaluate(&mut t, &listing, 2_000), "R"),
            ReleaseState::Ready
        );
    }

    #[test]
    fn a_file_that_keeps_growing_keeps_its_release_waiting() {
        let mut t = tracker();
        for (now, size) in [(0, 10), (900, 20), (1_800, 30)] {
            let statuses = evaluate(&mut t, &[entry("R/01.flac", size)], now);
            assert!(matches!(
                state_of(&statuses, "R"),
                ReleaseState::Waiting(Waiting::Changing { .. })
            ));
        }
    }

    #[test]
    fn files_that_vanish_are_forgotten_and_an_empty_listing_is_fine() {
        let mut t = tracker();
        evaluate(&mut t, &[entry("R/01.flac", 100)], 0);
        assert_eq!(t.len(), 1);
        assert_eq!(evaluate(&mut t, &[], 1_000).len(), 0);
        assert!(t.is_empty());
    }

    #[test]
    fn releases_come_back_in_a_stable_order() {
        let mut t = tracker();
        let listing = [entry("B/1.mp3", 1), entry("A/1.mp3", 1), entry("a.mp3", 1)];
        let statuses = evaluate(&mut t, &listing, 0);
        let keys: Vec<_> = statuses.iter().map(|status| status.key.clone()).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
