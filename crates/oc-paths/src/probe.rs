//! Read-only questions about a volume and its roots. Nothing here creates, changes
//! or follows anything in a library folder; every call is a `read_dir` or an
//! `lstat`.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use crate::component::{bytes_to_os_string, os_str_to_bytes};
use crate::profile::NameProfile;

/// Identity of a file system object, compared by equality only.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId {
    device: u64,
    node: u64,
}

/// Identity of a file system object, compared by equality only: the volume serial
/// number and the file index, which hard links share.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId {
    volume: u64,
    index: u64,
}

impl FileId {
    /// Does not follow a final symbolic link on Unix.
    #[cfg(unix)]
    pub fn of(path: &Path) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path)?;
        Ok(Self {
            device: metadata.dev(),
            node: metadata.ino(),
        })
    }

    /// Opens the object to ask, so a final symbolic link is followed on Windows.
    #[cfg(windows)]
    pub fn of(path: &Path) -> io::Result<Self> {
        let handle = winapi_util::Handle::from_path_any(path)?;
        let information = winapi_util::file::information(&handle)?;
        Ok(Self {
            volume: information.volume_serial_number(),
            index: information.file_index(),
        })
    }

    /// The part that is stable across restarts, for storing beside a root. Device
    /// numbers can change between boots, so only the node is kept.
    #[cfg(unix)]
    pub fn persistent_token(&self) -> Option<Vec<u8>> {
        Some(self.node.to_le_bytes().to_vec())
    }

    /// The NTFS file index survives restarts; the volume serial is left out to match Unix.
    #[cfg(windows)]
    pub fn persistent_token(&self) -> Option<Vec<u8>> {
        Some(self.index.to_le_bytes().to_vec())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Insensitive,
    Sensitive,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub case: Axis,
    pub normalization: Axis,
}

impl Probe {
    /// The finest profile consistent with the evidence. Anything undecided or
    /// sensitive stays finer, because a miss is safe and a false match is not.
    #[must_use]
    pub fn profile(&self) -> NameProfile {
        match (self.case, self.normalization) {
            (Axis::Insensitive, Axis::Insensitive) => NameProfile::FoldCaseNormalized,
            (Axis::Insensitive, _) => NameProfile::FoldCase,
            _ => NameProfile::Exact,
        }
    }
}

/// Does `variant` reach the same object as `original`?
fn compare(original: &Path, variant: &Path) -> Axis {
    let Ok(original_id) = FileId::of(original) else {
        return Axis::Unknown;
    };
    match FileId::of(variant) {
        Ok(variant_id) if variant_id == original_id => Axis::Insensitive,
        Ok(_) => Axis::Sensitive,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Axis::Sensitive,
        Err(_) => Axis::Unknown,
    }
}

fn flip_ascii_case(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

fn case_variant(name: &OsStr) -> Option<OsString> {
    let bytes = os_str_to_bytes(name);
    let text = std::str::from_utf8(&bytes).ok()?;
    let flipped = flip_ascii_case(text);
    (flipped != text)
        .then(|| bytes_to_os_string(flipped.as_bytes()).ok())
        .flatten()
}

fn normalization_variant(name: &OsStr) -> Option<OsString> {
    let bytes = os_str_to_bytes(name);
    let text = std::str::from_utf8(&bytes).ok()?;
    let composed: String = text.nfc().collect();
    let decomposed: String = text.nfd().collect();
    if composed == decomposed {
        return None;
    }
    let other = if text == composed {
        decomposed
    } else {
        composed
    };
    bytes_to_os_string(other.as_bytes()).ok()
}

/// Look up case and normalization variants of entries that already exist under
/// `root`, examining at most `entry_limit` entries, never following links and
/// never creating anything. An axis with no testable entry stays `Unknown`.
pub fn probe_root(root: &Path, entry_limit: usize) -> io::Result<Probe> {
    let mut probe = Probe {
        case: Axis::Unknown,
        normalization: Axis::Unknown,
    };
    let mut pending = vec![root.to_path_buf()];
    let mut examined = 0usize;
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if directory == root => return Err(error),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            examined += 1;
            if examined > entry_limit {
                return Ok(probe);
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path.clone());
            }
            let name = entry.file_name();
            if probe.case == Axis::Unknown {
                if let Some(variant) = case_variant(&name) {
                    probe.case = compare(&path, &directory.join(variant));
                }
            }
            if probe.normalization == Axis::Unknown {
                if let Some(variant) = normalization_variant(&name) {
                    probe.normalization = compare(&path, &directory.join(variant));
                }
            }
            if probe.case != Axis::Unknown && probe.normalization != Axis::Unknown {
                return Ok(probe);
            }
        }
    }
    Ok(probe)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootRelation {
    Disjoint,
    Same,
    /// The first path is inside the second.
    Inside,
    /// The first path contains the second.
    Contains,
}

/// By path first, so an offline root can still be compared; then by object
/// identity, which also catches aliases such as symlinks, mapped drives and
/// case-only differences on an insensitive volume.
#[must_use]
pub fn relation(first: &Path, second: &Path) -> RootRelation {
    if first == second {
        return RootRelation::Same;
    }
    if first.starts_with(second) {
        return RootRelation::Inside;
    }
    if second.starts_with(first) {
        return RootRelation::Contains;
    }
    let (Ok(first_id), Ok(second_id)) = (FileId::of(first), FileId::of(second)) else {
        return RootRelation::Disjoint;
    };
    let has_ancestor = |path: &Path, id: &FileId| {
        path.ancestors()
            .skip(1)
            .any(|ancestor| FileId::of(ancestor).is_ok_and(|found| &found == id))
    };
    if first_id == second_id {
        RootRelation::Same
    } else if has_ancestor(first, &second_id) {
        RootRelation::Inside
    } else if has_ancestor(second, &first_id) {
        RootRelation::Contains
    } else {
        RootRelation::Disjoint
    }
}

#[derive(Debug, Error)]
pub enum RootError {
    #[error("That is not a folder.")]
    NotADirectory,
    #[error("That folder is, contains or lies inside a folder already in the library.")]
    Overlaps {
        index: usize,
        relation: RootRelation,
    },
    #[error("That folder is, contains or lies inside the folder where openCrate keeps its own data. Choose your music folder instead.")]
    Reserved {
        index: usize,
        relation: RootRelation,
    },
    #[error("The folder could not be read: {0}")]
    Io(#[from] io::Error),
}

/// A path with links and `..` resolved, in its plain form: on Windows `C:\Music`, not the
/// `\\?\C:\Music` the standard library returns, which rekordbox's paths, the xml links and
/// every comparison with a stored root would not match. Every stored path is made with this.
pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    dunce::canonicalize(path)
}

/// Validate a new library root without changing anything. Returns its canonical
/// path. `roots` and `reserved` (app data, staging, download folders) are
/// canonical paths; roots must not overlap or alias either.
pub fn check_new_root(
    candidate: &Path,
    roots: &[&Path],
    reserved: &[&Path],
) -> Result<PathBuf, RootError> {
    let canonical = canonical(candidate)?;
    if !fs::metadata(&canonical)?.is_dir() {
        return Err(RootError::NotADirectory);
    }
    for (index, other) in roots.iter().enumerate() {
        let relation = relation(&canonical, other);
        if relation != RootRelation::Disjoint {
            return Err(RootError::Overlaps { index, relation });
        }
    }
    for (index, other) in reserved.iter().enumerate() {
        let relation = relation(&canonical, other);
        if relation != RootRelation::Disjoint {
            return Err(RootError::Reserved { index, relation });
        }
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::tempdir;

    use super::*;

    fn touch(path: &Path) {
        File::create(path).unwrap();
    }

    #[test]
    fn compare_reads_identity_not_spelling() {
        let dir = tempdir().unwrap();
        let (a, b) = (dir.path().join("a.txt"), dir.path().join("b.txt"));
        touch(&a);
        touch(&b);
        assert_eq!(compare(&a, &b), Axis::Sensitive, "two different files");
        assert_eq!(compare(&a, &dir.path().join("missing")), Axis::Sensitive);
        assert_eq!(compare(&dir.path().join("gone"), &a), Axis::Unknown);
        let link = dir.path().join("alias.txt");
        fs::hard_link(&a, &link).unwrap();
        assert_eq!(compare(&a, &link), Axis::Insensitive, "same object");
    }

    #[test]
    fn variants_change_only_what_they_test() {
        assert_eq!(
            case_variant(OsStr::new("Track 01.aif")).unwrap(),
            OsString::from("tRACK 01.AIF")
        );
        assert_eq!(case_variant(OsStr::new("123.-_")), None);
        assert_eq!(
            case_variant(OsStr::new("\u{e9}a")).unwrap(),
            OsString::from("\u{e9}A"),
            "non-ASCII letters are left alone"
        );
        let composed = OsString::from("Caf\u{e9}");
        let decomposed = OsString::from("Cafe\u{301}");
        assert_eq!(normalization_variant(&composed).unwrap(), decomposed);
        assert_eq!(normalization_variant(&decomposed).unwrap(), composed);
        assert_eq!(normalization_variant(OsStr::new("plain.aif")), None);
    }

    #[cfg(unix)]
    #[test]
    fn names_that_are_not_text_are_never_probed() {
        use std::os::unix::ffi::OsStringExt;
        let name = OsString::from_vec(vec![b'A', 0xFF]);
        assert_eq!(case_variant(&name), None);
        assert_eq!(normalization_variant(&name), None);
    }

    #[test]
    fn an_empty_root_decides_nothing_and_stays_exact() {
        let dir = tempdir().unwrap();
        let probe = probe_root(dir.path(), 100).unwrap();
        assert_eq!(probe.case, Axis::Unknown);
        assert_eq!(probe.normalization, Axis::Unknown);
        assert_eq!(probe.profile(), NameProfile::Exact);
        assert!(probe_root(&dir.path().join("missing"), 100).is_err());
    }

    #[test]
    fn probing_writes_nothing() {
        let dir = tempdir().unwrap();
        for name in ["Track.aif", "Caf\u{e9}.aif"] {
            touch(&dir.path().join(name));
        }
        fs::create_dir(dir.path().join("Sub")).unwrap();
        let listing = || {
            let mut names: Vec<_> = fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let before = listing();
        probe_root(dir.path(), 100).unwrap();
        assert_eq!(listing(), before);
    }

    #[test]
    fn the_probed_profile_is_never_coarser_than_the_volume() {
        let dir = tempdir().unwrap();
        let canonical = canonical(dir.path()).unwrap();
        for name in ["Track.aif", "Caf\u{e9}.aif", "A-B.aif"] {
            touch(&canonical.join(name));
        }
        let profile = probe_root(&canonical, 100).unwrap().profile();
        let pairs = [
            ("Track.aif", "track.aif"),
            ("Caf\u{e9}.aif", "Cafe\u{301}.aif"),
            ("A-B.aif", "AB.aif"),
            ("A-B.aif", "a-b.aif"),
        ];
        for (stored, spelled) in pairs {
            if profile.components_equal(stored.as_bytes(), spelled.as_bytes()) {
                let a = FileId::of(&canonical.join(stored));
                let b = FileId::of(&canonical.join(spelled));
                assert!(
                    a.is_ok() && a.ok() == b.ok(),
                    "{profile:?} equates {stored:?} and {spelled:?} but the volume does not"
                );
            }
        }
    }

    #[test]
    fn root_relations() {
        let base = canonical(&tempdir().unwrap().keep()).unwrap();
        let (a, b, c) = (base.join("a"), base.join("a").join("b"), base.join("c"));
        for dir in [&a, &b, &c] {
            fs::create_dir_all(dir).unwrap();
        }
        assert_eq!(relation(&a, &a), RootRelation::Same);
        assert_eq!(relation(&b, &a), RootRelation::Inside);
        assert_eq!(relation(&a, &b), RootRelation::Contains);
        assert_eq!(relation(&a, &c), RootRelation::Disjoint);
        assert_eq!(
            relation(&a, &base.join("not-here")),
            RootRelation::Disjoint,
            "an offline root is compared by path only"
        );
        assert_eq!(
            relation(&b, &base.join("a").join("gone")),
            RootRelation::Disjoint
        );
        assert_eq!(
            relation(&base.join("a").join("gone").join("x"), &a),
            RootRelation::Inside
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn registration_refuses_overlaps_aliases_and_files() {
        use std::os::unix::fs::symlink;
        let base = canonical(&tempdir().unwrap().keep()).unwrap();
        let (music, nested, other) = (
            base.join("music"),
            base.join("music/sub"),
            base.join("other"),
        );
        for dir in [&music, &nested, &other] {
            fs::create_dir_all(dir).unwrap();
        }
        touch(&base.join("file.txt"));
        symlink(&music, base.join("link")).unwrap();

        let existing: [&Path; 1] = [&music];
        assert!(matches!(
            check_new_root(&nested, &existing, &[]),
            Err(RootError::Overlaps {
                index: 0,
                relation: RootRelation::Inside
            })
        ));
        assert!(matches!(
            check_new_root(&base, &existing, &[]),
            Err(RootError::Overlaps {
                index: 0,
                relation: RootRelation::Contains
            })
        ));
        assert!(matches!(
            check_new_root(&base.join("link"), &existing, &[]),
            Err(RootError::Overlaps {
                index: 0,
                relation: RootRelation::Same
            })
        ));
        assert!(matches!(
            check_new_root(&base.join("file.txt"), &existing, &[]),
            Err(RootError::NotADirectory)
        ));
        assert!(matches!(
            check_new_root(&base.join("missing"), &existing, &[]),
            Err(RootError::Io(_))
        ));
        assert_eq!(check_new_root(&other, &existing, &[]).unwrap(), other);
        let reserved: [&Path; 1] = [&other];
        assert!(matches!(
            check_new_root(&other, &[], &reserved),
            Err(RootError::Reserved {
                index: 0,
                relation: RootRelation::Same
            })
        ));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_case_only_alias_of_a_root_is_the_same_root_when_the_volume_says_so() {
        let base = canonical(&tempdir().unwrap().keep()).unwrap();
        let music = base.join("Music");
        fs::create_dir(&music).unwrap();
        let alias = base.join("music");
        let expected = if FileId::of(&alias).is_ok() {
            RootRelation::Same
        } else {
            RootRelation::Disjoint
        };
        assert_eq!(relation(&alias, &music), expected);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn the_persistent_token_ignores_the_device() {
        let dir = tempdir().unwrap();
        let id = FileId::of(dir.path()).unwrap();
        assert_eq!(id.persistent_token().unwrap().len(), 8);
    }

    #[test]
    fn hard_links_share_an_identity_and_a_replaced_folder_does_not() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("staged.aif");
        let second = dir.path().join("filed.aif");
        touch(&first);
        fs::hard_link(&first, &second).unwrap();
        assert_eq!(FileId::of(&first).unwrap(), FileId::of(&second).unwrap());

        let other = dir.path().join("other.aif");
        touch(&other);
        assert_ne!(FileId::of(&first).unwrap(), FileId::of(&other).unwrap());

        // The folder is kept open meanwhile, so the file system cannot reuse its number.
        let folder = dir.path().join("Music");
        fs::create_dir(&folder).unwrap();
        let before = FileId::of(&folder).unwrap().persistent_token();
        let kept = dir.path().join("Music.unplugged");
        fs::rename(&folder, &kept).unwrap();
        fs::create_dir(&folder).unwrap();
        assert_ne!(FileId::of(&folder).unwrap().persistent_token(), before);
    }
}
