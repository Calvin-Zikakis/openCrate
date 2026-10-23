//! Name profiles: the rule that turns a stored path into a lookup key.
//!
//! A profile must never be coarser than the volume it describes. Being finer only
//! costs a missed match; being coarser would join two names the volume keeps apart.

use std::collections::HashMap;

use unicode_normalization::UnicodeNormalization;

use crate::component::RelPath;

/// Bump when any profile's key function changes, so stored keys can be recomputed.
pub const KEY_SCHEME_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameProfile {
    /// The raw bytes. Case-sensitive volumes, unknown volumes, and the fallback.
    Exact,
    /// ASCII letters folded to lowercase. Case-insensitive, non-normalizing volumes.
    FoldCase,
    /// NFC, then ASCII lowercase. Case- and normalization-insensitive volumes.
    FoldCaseNormalized,
}

impl NameProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::FoldCase => "fold_case",
            Self::FoldCaseNormalized => "fold_case_normalized",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "exact" => Some(Self::Exact),
            "fold_case" => Some(Self::FoldCase),
            "fold_case_normalized" => Some(Self::FoldCaseNormalized),
            _ => None,
        }
    }

    /// Key of one component. Only components that are valid UTF-8 are folded; any
    /// other bytes stay raw, which is finer and therefore safe.
    #[must_use]
    pub fn key_component(self, component: &[u8]) -> Vec<u8> {
        let Ok(text) = std::str::from_utf8(component) else {
            return component.to_vec();
        };
        match self {
            Self::Exact => component.to_vec(),
            Self::FoldCase => text.to_ascii_lowercase().into_bytes(),
            Self::FoldCaseNormalized => text
                .nfc()
                .collect::<String>()
                .to_ascii_lowercase()
                .into_bytes(),
        }
    }

    #[must_use]
    pub fn components_equal(self, left: &[u8], right: &[u8]) -> bool {
        self.key_component(left) == self.key_component(right)
    }

    #[must_use]
    pub fn key(self, path: &RelPath) -> NameKey {
        let mut key = Vec::with_capacity(path.as_stored().len());
        for (index, component) in path.components().enumerate() {
            if index > 0 {
                key.push(0);
            }
            key.extend_from_slice(&self.key_component(component));
        }
        NameKey(key)
    }
}

/// A derived lookup key, unique per `(root, key)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NameKey(Vec<u8>);

impl NameKey {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Indexes of the first two different paths that share a key. Such a pair proves
/// the profile was coarser than the directory, so the root is demoted to `Exact`.
#[must_use]
pub fn first_collision<'a, I>(profile: NameProfile, paths: I) -> Option<(usize, usize)>
where
    I: IntoIterator<Item = &'a RelPath>,
{
    let mut seen: HashMap<NameKey, (usize, &RelPath)> = HashMap::new();
    for (index, path) in paths.into_iter().enumerate() {
        match seen.get(&profile.key(path)) {
            Some(&(first, earlier)) if earlier != path => return Some((first, index)),
            Some(_) => {}
            None => {
                seen.insert(profile.key(path), (index, path));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Flavor;

    const ALL: [NameProfile; 3] = [
        NameProfile::Exact,
        NameProfile::FoldCase,
        NameProfile::FoldCaseNormalized,
    ];

    fn rel(parts: &[&str]) -> RelPath {
        RelPath::from_components(parts.iter().map(|p| p.as_bytes()), Flavor::Posix).unwrap()
    }

    fn same(profile: NameProfile, left: &str, right: &str) -> bool {
        profile.key(&rel(&[left])) == profile.key(&rel(&[right]))
    }

    #[test]
    fn punctuation_is_never_folded() {
        for profile in ALL {
            assert!(!same(profile, "A-B.aif", "AB.aif"), "{profile:?}");
            assert!(!same(profile, "A_B.aif", "A B.aif"), "{profile:?}");
        }
    }

    #[test]
    fn case_is_folded_only_by_the_case_profiles() {
        assert!(!same(NameProfile::Exact, "Track.AIF", "track.aif"));
        assert!(same(NameProfile::FoldCase, "Track.AIF", "track.aif"));
        assert!(same(
            NameProfile::FoldCaseNormalized,
            "Track.AIF",
            "track.aif"
        ));
    }

    #[test]
    fn normalization_is_folded_only_by_the_normalizing_profile() {
        let (nfc, nfd) = ("Caf\u{e9}.aif", "Cafe\u{301}.aif");
        assert!(!same(NameProfile::Exact, nfc, nfd));
        assert!(!same(NameProfile::FoldCase, nfc, nfd));
        assert!(same(NameProfile::FoldCaseNormalized, nfc, nfd));
    }

    #[test]
    fn non_ascii_case_and_full_case_folds_stay_distinct() {
        for profile in ALL {
            assert!(!same(profile, "\u{e9}.aif", "\u{c9}.aif"), "{profile:?}");
            assert!(
                !same(profile, "Stra\u{df}e.aif", "STRASSE.AIF"),
                "{profile:?}"
            );
            assert!(!same(profile, "\u{fb01}.aif", "fi.aif"), "{profile:?}");
        }
    }

    #[test]
    fn canonical_singletons_join_only_under_normalization() {
        // U+212A KELVIN SIGN decomposes canonically to K, so a volume that ignores
        // canonical equivalence keeps it apart from k.
        assert!(!same(NameProfile::Exact, "\u{212a}.aif", "k.aif"));
        assert!(!same(NameProfile::FoldCase, "\u{212a}.aif", "k.aif"));
        assert!(same(
            NameProfile::FoldCaseNormalized,
            "\u{212a}.aif",
            "k.aif"
        ));
    }

    #[test]
    fn components_that_are_not_text_stay_raw() {
        let raw = |bytes: &[u8]| RelPath::from_components([bytes], Flavor::Posix).unwrap();
        let upper: &[u8] = &[b'A', 0xFF];
        let lower: &[u8] = &[b'a', 0xFF];
        for profile in ALL {
            assert_ne!(profile.key(&raw(upper)), profile.key(&raw(lower)));
        }
    }

    #[test]
    fn keys_keep_component_boundaries() {
        let nested = rel(&["a", "b"]);
        let flat = rel(&["ab"]);
        for profile in ALL {
            assert_ne!(profile.key(&nested), profile.key(&flat));
        }
    }

    #[test]
    fn collisions_are_found_only_between_different_paths() {
        let paths = [
            rel(&["A", "x.aif"]),
            rel(&["B", "y.aif"]),
            rel(&["a", "x.aif"]),
        ];
        assert_eq!(first_collision(NameProfile::Exact, &paths), None);
        assert_eq!(first_collision(NameProfile::FoldCase, &paths), Some((0, 2)));
        let repeated = [rel(&["A"]), rel(&["A"])];
        assert_eq!(first_collision(NameProfile::FoldCase, &repeated), None);
    }

    #[test]
    fn profile_names_round_trip() {
        for profile in ALL {
            assert_eq!(NameProfile::parse(profile.as_str()), Some(profile));
        }
        assert_eq!(NameProfile::parse("nfc"), None);
    }
}
