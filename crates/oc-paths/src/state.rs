//! Names for the states in the filesystem contract, and the rules that move rows
//! between them. Offline is not a file state; it is derived from the root.

use crate::component::RelPath;
use crate::location::Resolution;

macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The text stored in the database.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            #[must_use]
            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

string_enum! {
    /// Evaluated on the root and stored with a timestamp and reason; never on file rows.
    Availability {
        Available => "available",
        Unavailable => "unavailable",
        Unverified => "unverified",
    }
}

string_enum! {
    /// Set by openCrate operations. The scanner never changes it.
    Lifecycle {
        Staged => "staged",
        Active => "active",
        Archived => "archived",
    }
}

string_enum! {
    /// What the last trustworthy scan saw.
    Presence {
        Present => "present",
        Missing => "missing",
    }
}

string_enum! {
    /// A database-only result for one rekordbox entry. Never says "missing".
    MatchStatus {
        Matched => "matched",
        Unmapped => "unmapped",
        NotIndexed => "not_indexed",
        Ambiguous => "ambiguous",
        Unrecognized => "unrecognized",
    }
}

string_enum! {
    MatchKind {
        ExactBytes => "exact_bytes",
        KeyEqual => "key_equal",
        VolumeResolved => "volume_resolved",
    }
}

string_enum! {
    /// An on-demand, read-only walk of a mapped path.
    DiskCheck {
        Unchecked => "unchecked",
        Exists => "exists",
        Absent => "absent",
        RootUnavailable => "root_unavailable",
    }
}

/// A pass that would mark at least this share of a root's present files missing
/// (and at least [`HOLD_MIN_FILES`]) is held for the owner to confirm. Unmeasured.
pub const HOLD_PERCENT: usize = 25;
pub const HOLD_MIN_FILES: usize = 20;

/// A row may become missing only under a directory that was enumerated
/// successfully while its root was available.
#[must_use]
pub fn may_mark_missing(root: Availability, parent_enumerated_ok: bool) -> bool {
    root == Availability::Available && parent_enumerated_ok
}

#[must_use]
pub fn hold_mass_disappearance(present_in_root: usize, would_mark_missing: usize) -> bool {
    would_mark_missing >= HOLD_MIN_FILES
        && would_mark_missing.saturating_mul(100) >= present_in_root.saturating_mul(HOLD_PERCENT)
}

/// A row seen again is present, even if it was missing (the same row revives). A
/// row not seen changes only when the rules allow it; otherwise it is untouched.
#[must_use]
pub fn next_presence(current: Presence, seen: bool, may_mark_missing: bool) -> Presence {
    if seen {
        Presence::Present
    } else if may_mark_missing {
        Presence::Missing
    } else {
        current
    }
}

/// Only a negative check on an available root may be worded as "not on disk".
#[must_use]
pub fn may_claim_absent(check: DiskCheck, root: Availability) -> bool {
    check == DiskCheck::Absent && root == Availability::Available
}

/// `indexed` says whether a row with the resolved key exists in that root.
#[must_use]
pub fn match_status(resolution: &Resolution, indexed: bool) -> MatchStatus {
    match resolution {
        Resolution::Mapped { .. } if indexed => MatchStatus::Matched,
        Resolution::Mapped { .. } => MatchStatus::NotIndexed,
        Resolution::NoFile { .. } => MatchStatus::Unrecognized,
        Resolution::Unmapped => MatchStatus::Unmapped,
        Resolution::Ambiguous { .. } => MatchStatus::Ambiguous,
    }
}

/// Call only when the keys already agree.
#[must_use]
pub fn classify_match(stored: &RelPath, remainder: &RelPath) -> MatchKind {
    if stored == remainder {
        MatchKind::ExactBytes
    } else {
        MatchKind::KeyEqual
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Flavor;

    #[test]
    fn stored_names_round_trip_and_are_unique() {
        fn check<T: Copy + std::fmt::Debug + PartialEq>(
            all: &[T],
            name: fn(T) -> &'static str,
            parse: fn(&str) -> Option<T>,
        ) {
            let mut seen = std::collections::HashSet::new();
            for &value in all {
                assert!(seen.insert(name(value)), "duplicate {value:?}");
                assert_eq!(parse(name(value)), Some(value));
            }
            assert_eq!(parse("nonsense"), None);
        }
        check(Availability::ALL, Availability::as_str, Availability::parse);
        check(Lifecycle::ALL, Lifecycle::as_str, Lifecycle::parse);
        check(Presence::ALL, Presence::as_str, Presence::parse);
        check(MatchStatus::ALL, MatchStatus::as_str, MatchStatus::parse);
        check(MatchKind::ALL, MatchKind::as_str, MatchKind::parse);
        check(DiskCheck::ALL, DiskCheck::as_str, DiskCheck::parse);
    }

    #[test]
    fn nothing_goes_missing_unless_the_root_and_directory_were_readable() {
        for root in Availability::ALL {
            for ok in [true, false] {
                let allowed = *root == Availability::Available && ok;
                assert_eq!(may_mark_missing(*root, ok), allowed, "{root:?} {ok}");
            }
        }
        let unseen = |allowed| next_presence(Presence::Present, false, allowed);
        assert_eq!(unseen(false), Presence::Present, "unreadable: untouched");
        assert_eq!(unseen(true), Presence::Missing);
        assert_eq!(
            next_presence(Presence::Missing, false, false),
            Presence::Missing,
            "stays missing"
        );
    }

    #[test]
    fn a_file_that_reappears_is_present_again() {
        for allowed in [true, false] {
            assert_eq!(
                next_presence(Presence::Missing, true, allowed),
                Presence::Present
            );
        }
    }

    #[test]
    fn mass_disappearance_is_held_but_small_changes_are_not() {
        assert!(!hold_mass_disappearance(10_000, 19), "under the minimum");
        assert!(!hold_mass_disappearance(10_000, 100), "1% of the root");
        assert!(hold_mass_disappearance(10_000, 2_500), "exactly 25%");
        assert!(
            hold_mass_disappearance(10_000, 10_000),
            "an empty mount point"
        );
        assert!(hold_mass_disappearance(20, 20), "small root, all gone");
        assert!(!hold_mass_disappearance(0, 0));
        assert!(!hold_mass_disappearance(usize::MAX, 20), "no overflow");
    }

    #[test]
    fn only_a_negative_check_on_an_available_root_may_say_not_on_disk() {
        for check in DiskCheck::ALL {
            for root in Availability::ALL {
                let expected = *check == DiskCheck::Absent && *root == Availability::Available;
                assert_eq!(may_claim_absent(*check, *root), expected);
            }
        }
    }

    #[test]
    fn match_status_never_says_missing() {
        let remainder = RelPath::from_components([b"x"], Flavor::Posix).unwrap();
        let mapped = Resolution::Mapped {
            root_id: 1,
            alias_len: 1,
            remainder: remainder.clone(),
        };
        assert_eq!(match_status(&mapped, true), MatchStatus::Matched);
        assert_eq!(match_status(&mapped, false), MatchStatus::NotIndexed);
        assert_eq!(
            match_status(&Resolution::Unmapped, false),
            MatchStatus::Unmapped
        );
        assert_eq!(
            match_status(
                &Resolution::Ambiguous {
                    root_ids: vec![1, 2]
                },
                true
            ),
            MatchStatus::Ambiguous
        );
        assert_eq!(
            match_status(&Resolution::NoFile { root_id: 1 }, true),
            MatchStatus::Unrecognized
        );
    }

    #[test]
    fn match_kind_records_whether_the_bytes_agree() {
        let path = |text: &str| RelPath::from_components([text.as_bytes()], Flavor::Posix).unwrap();
        assert_eq!(
            classify_match(&path("a.aif"), &path("a.aif")),
            MatchKind::ExactBytes
        );
        assert_eq!(
            classify_match(&path("A.aif"), &path("a.aif")),
            MatchKind::KeyEqual
        );
    }
}
