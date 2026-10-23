//! Has a file stopped changing?
//!
//! A download that is still being written grows or has a moving modified time. The tracker
//! never looks at the clock or the disk: the caller reports what it saw and when, in
//! milliseconds, and the tracker says whether the file has been unchanged long enough. The
//! time is the caller's own observation time, not the file's modified time, because modified
//! times on a network share can be wrong by minutes.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub size: u64,
    pub modified_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StabilityConfig {
    /// How long a file must look the same before it counts as finished.
    pub quiet_ms: i64,
    /// How many sightings with the same size and modified time are needed, so one lucky read
    /// between two bursts of writing is not enough.
    pub min_sightings: u32,
    /// A file whose own modified time is at least this old, and which has not changed since it
    /// was first seen, counts as finished on its first sighting. A writer updates the modified
    /// time, so an old time means nobody has written to it for that long. `None` turns it off,
    /// for volumes whose clocks cannot be trusted.
    pub trust_old_mtime_ms: Option<i64>,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            quiet_ms: 10_000,
            min_sightings: 2,
            trust_old_mtime_ms: Some(30_000),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    /// Seen, but not yet unchanged for long enough. `quiet_for_ms` is how long it has been
    /// unchanged so far (zero on the first sighting or just after a change).
    Settling {
        quiet_for_ms: i64,
    },
    Stable,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    last: Observation,
    changed_at_ms: i64,
    sightings: u32,
    /// The size or modified time differed between two sightings, so the modified time no
    /// longer says how long the file has been idle.
    ever_changed: bool,
}

#[derive(Clone, Debug)]
pub struct StabilityTracker<K> {
    config: StabilityConfig,
    entries: HashMap<K, Entry>,
}

impl<K: Hash + Eq> StabilityTracker<K> {
    #[must_use]
    pub fn new(config: StabilityConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
        }
    }

    /// Report one sighting of `key` at `now_ms`. An empty file is never stable: it is a file
    /// that was just created, and an empty audio file is not importable anyway.
    pub fn observe(&mut self, key: K, seen: Observation, now_ms: i64) -> Stability {
        let entry = self.entries.entry(key).or_insert(Entry {
            last: seen,
            changed_at_ms: now_ms,
            sightings: 0,
            ever_changed: false,
        });
        if entry.last == seen {
            entry.sightings = entry.sightings.saturating_add(1);
        } else {
            *entry = Entry {
                last: seen,
                changed_at_ms: now_ms,
                sightings: 1,
                ever_changed: true,
            };
        }
        // A clock that moved backwards restarts the wait instead of producing a negative one.
        if now_ms < entry.changed_at_ms {
            entry.changed_at_ms = now_ms;
        }
        let quiet_for_ms = now_ms - entry.changed_at_ms;
        let idle_for_long = !entry.ever_changed
            && self.config.trust_old_mtime_ms.is_some_and(|age| {
                seen.modified_ms > 0 && now_ms.saturating_sub(seen.modified_ms) >= age
            });
        if seen.size > 0 && idle_for_long {
            return Stability::Stable;
        }
        if seen.size > 0
            && entry.sightings >= self.config.min_sightings
            && quiet_for_ms >= self.config.quiet_ms
        {
            Stability::Stable
        } else {
            Stability::Settling { quiet_for_ms }
        }
    }

    /// Forget everything not in `present`, so a file that was removed and later reappears under
    /// the same name starts its wait again, and the map does not grow without bound.
    pub fn retain_present(&mut self, present: &HashSet<K>) {
        self.entries.retain(|key, _| present.contains(key));
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<K: Hash + Eq> Default for StabilityTracker<K> {
    fn default() -> Self {
        Self::new(StabilityConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(size: u64, modified_ms: i64) -> Observation {
        Observation { size, modified_ms }
    }

    fn tracker() -> StabilityTracker<&'static str> {
        StabilityTracker::new(StabilityConfig {
            quiet_ms: 10_000,
            min_sightings: 2,
            trust_old_mtime_ms: None,
        })
    }

    #[test]
    fn a_file_is_stable_only_after_it_is_unchanged_for_the_quiet_period() {
        let mut t = tracker();
        let file = seen(5_000, 100);
        assert_eq!(
            t.observe("a", file, 0),
            Stability::Settling { quiet_for_ms: 0 }
        );
        assert_eq!(
            t.observe("a", file, 9_999),
            Stability::Settling {
                quiet_for_ms: 9_999
            }
        );
        assert_eq!(t.observe("a", file, 10_000), Stability::Stable);
        assert_eq!(t.observe("a", file, 60_000), Stability::Stable);
    }

    #[test]
    fn growth_or_a_new_modified_time_restarts_the_wait() {
        let mut t = tracker();
        t.observe("a", seen(1_000, 1), 0);
        t.observe("a", seen(1_000, 1), 9_000);
        assert_eq!(
            t.observe("a", seen(2_000, 1), 9_500),
            Stability::Settling { quiet_for_ms: 0 }
        );
        assert_eq!(
            t.observe("a", seen(2_000, 1), 15_000),
            Stability::Settling {
                quiet_for_ms: 5_500
            }
        );
        assert_eq!(t.observe("a", seen(2_000, 1), 19_500), Stability::Stable);
        // The same size with a new modified time is a change as well.
        assert_eq!(
            t.observe("a", seen(2_000, 2), 20_000),
            Stability::Settling { quiet_for_ms: 0 }
        );
    }

    #[test]
    fn one_sighting_is_never_enough_even_with_a_zero_quiet_period() {
        let mut t = StabilityTracker::new(StabilityConfig {
            quiet_ms: 0,
            min_sightings: 2,
            trust_old_mtime_ms: None,
        });
        assert!(matches!(
            t.observe("a", seen(10, 1), 5),
            Stability::Settling { .. }
        ));
        assert_eq!(t.observe("a", seen(10, 1), 5), Stability::Stable);
    }

    #[test]
    fn an_empty_file_is_never_stable() {
        let mut t = tracker();
        for now in [0, 20_000, 40_000] {
            assert!(matches!(
                t.observe("empty", seen(0, 1), now),
                Stability::Settling { .. }
            ));
        }
    }

    #[test]
    fn files_are_tracked_independently() {
        let mut t = tracker();
        t.observe("a", seen(10, 1), 0);
        t.observe("b", seen(10, 1), 8_000);
        t.observe("a", seen(10, 1), 10_000);
        assert_eq!(t.observe("a", seen(10, 1), 10_001), Stability::Stable);
        assert!(matches!(
            t.observe("b", seen(10, 1), 10_001),
            Stability::Settling { .. }
        ));
    }

    #[test]
    fn a_clock_that_goes_backwards_restarts_the_wait() {
        let mut t = tracker();
        t.observe("a", seen(10, 1), 50_000);
        assert_eq!(
            t.observe("a", seen(10, 1), 1_000),
            Stability::Settling { quiet_for_ms: 0 }
        );
        assert_eq!(t.observe("a", seen(10, 1), 11_000), Stability::Stable);
    }

    #[test]
    fn a_removed_file_that_comes_back_waits_again() {
        let mut t = tracker();
        t.observe("a", seen(10, 1), 0);
        assert_eq!(t.observe("a", seen(10, 1), 20_000), Stability::Stable);
        t.retain_present(&HashSet::new());
        assert!(t.is_empty());
        assert_eq!(
            t.observe("a", seen(10, 1), 30_000),
            Stability::Settling { quiet_for_ms: 0 }
        );
        assert_eq!(t.len(), 1);
    }

    fn trusting() -> StabilityTracker<&'static str> {
        StabilityTracker::new(StabilityConfig {
            quiet_ms: 10_000,
            min_sightings: 2,
            trust_old_mtime_ms: Some(30_000),
        })
    }

    #[test]
    fn a_file_untouched_for_a_long_time_is_stable_on_first_sight() {
        let mut t = trusting();
        let now = 1_000_000;
        assert_eq!(
            t.observe("old", seen(5, now - 30_000), now),
            Stability::Stable
        );
        assert!(matches!(
            t.observe("fresh", seen(5, now - 29_999), now),
            Stability::Settling { .. }
        ));
        assert!(matches!(
            t.observe("empty", seen(0, now - 90_000), now),
            Stability::Settling { .. }
        ));
        assert!(matches!(
            t.observe("no-time", seen(5, 0), now),
            Stability::Settling { .. }
        ));
        assert!(matches!(
            t.observe("future", seen(5, now + 60_000), now),
            Stability::Settling { .. }
        ));
    }

    #[test]
    fn the_old_time_is_not_trusted_once_the_file_has_been_seen_to_change() {
        let mut t = trusting();
        let now = 1_000_000;
        // A client that preserves the sender's modified time while it keeps writing.
        assert_eq!(t.observe("a", seen(5, 1_000), now), Stability::Stable);
        assert!(matches!(
            t.observe("a", seen(9, 1_000), now + 1_000),
            Stability::Settling { quiet_for_ms: 0 }
        ));
        assert!(matches!(
            t.observe("a", seen(9, 1_000), now + 2_000),
            Stability::Settling { .. }
        ));
        assert_eq!(
            t.observe("a", seen(9, 1_000), now + 11_000),
            Stability::Stable
        );
    }
}
