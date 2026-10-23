//! A read-only walk of a library root. It lists directories and reads metadata; it never
//! opens a file, never follows a link and never writes. What it reports is what a scan
//! pass needs: the audio files seen, every directory found, and which directories were
//! listed completely. A directory with any listing error is not "listed", so nothing under
//! it can be concluded missing.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use crate::component::{check_component, os_str_to_bytes, Flavor, RelPath};

/// A directory as relative components; the root is the empty list.
pub type DirPath = Vec<Vec<u8>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkedFile {
    pub path: RelPath,
    pub size: i64,
    pub mtime_ms: i64,
    pub container: &'static str,
    pub file_token: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalkReport {
    pub files: Vec<WalkedFile>,
    pub existing_dirs: Vec<DirPath>,
    pub enumerated_dirs: Vec<DirPath>,
    /// Symbolic links, junctions and the like: not followed, not indexed.
    pub links_skipped: Vec<Vec<u8>>,
    /// Directories that could not be listed completely.
    pub unlisted_dirs: Vec<DirPath>,
    /// Names that cannot be stored as path components, and `AppleDouble` `._` files.
    pub names_skipped: usize,
    pub entries_seen: usize,
    pub cancelled: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct WalkOptions<'a> {
    /// Directories deeper than this are found but not listed (so nothing below is concluded).
    pub max_depth: usize,
    /// Checked between entries; a set flag ends the walk with `cancelled`.
    pub cancel: Option<&'a AtomicBool>,
}

impl Default for WalkOptions<'_> {
    fn default() -> Self {
        Self {
            max_depth: 64,
            cancel: None,
        }
    }
}

/// The container label for a file name, by extension only. Headers are read later.
#[must_use]
pub fn container_for(name: &[u8]) -> Option<&'static str> {
    let dot = name.iter().rposition(|byte| *byte == b'.')?;
    let extension = name.get(dot + 1..)?.to_ascii_lowercase();
    match extension.as_slice() {
        b"aif" | b"aiff" | b"aifc" => Some("AIFF"),
        b"mp3" => Some("MP3"),
        b"flac" => Some("FLAC"),
        b"wav" | b"wave" => Some("WAV"),
        b"m4a" | b"mp4" | b"aac" => Some("M4A"),
        b"ogg" | b"oga" | b"opus" => Some("OGG"),
        b"wma" => Some("WMA"),
        _ => None,
    }
}

fn millis(time: io::Result<std::time::SystemTime>) -> i64 {
    match time.map(|t| t.duration_since(UNIX_EPOCH)) {
        Ok(Ok(since)) => i64::try_from(since.as_millis()).unwrap_or(i64::MAX),
        _ => 0,
    }
}

/// The inode, which survives restarts on the same volume. Other platforms record none.
#[cfg(unix)]
fn inode_bytes(metadata: &fs::Metadata) -> Vec<u8> {
    use std::os::unix::fs::MetadataExt;
    metadata.ino().to_le_bytes().to_vec()
}

#[cfg(not(unix))]
fn inode_bytes(_metadata: &fs::Metadata) -> Vec<u8> {
    Vec::new()
}

/// Walk `root`. An error is returned only if the root itself cannot be listed, which a
/// caller treats as the root being unavailable.
pub fn walk_root(root: &Path, options: &WalkOptions<'_>) -> io::Result<WalkReport> {
    let mut report = WalkReport::default();
    report.existing_dirs.push(Vec::new());
    let mut pending: Vec<(PathBuf, DirPath)> = vec![(root.to_path_buf(), Vec::new())];
    let mut first = true;
    while let Some((directory, relative)) = pending.pop() {
        if options
            .cancel
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            report.cancelled = true;
            return Ok(report);
        }
        if relative.len() >= options.max_depth {
            report.unlisted_dirs.push(relative);
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if first => return Err(error),
            Err(_) => {
                report.unlisted_dirs.push(relative);
                continue;
            }
        };
        first = false;
        let mut complete = true;
        for entry in entries {
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            report.entries_seen += 1;
            let name = os_str_to_bytes(&entry.file_name());
            if name.starts_with(b"._") || check_component(&name, Flavor::HOST).is_err() {
                report.names_skipped += 1;
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                complete = false;
                continue;
            };
            let mut child = relative.clone();
            child.push(name.clone());
            if kind.is_symlink() {
                report.links_skipped.push(child.join(&0));
            } else if kind.is_dir() {
                report.existing_dirs.push(child.clone());
                pending.push((entry.path(), child));
            } else if kind.is_file() {
                let Some(container) = container_for(&name) else {
                    continue;
                };
                let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                    complete = false;
                    continue;
                };
                let Ok(path) = RelPath::from_components(&child, Flavor::HOST) else {
                    report.names_skipped += 1;
                    continue;
                };
                report.files.push(WalkedFile {
                    path,
                    size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
                    mtime_ms: millis(metadata.modified()),
                    container,
                    file_token: cfg!(unix).then(|| inode_bytes(&metadata)),
                });
            }
        }
        if complete {
            report.enumerated_dirs.push(relative);
        } else {
            report.unlisted_dirs.push(relative);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, write};

    use tempfile::tempdir;

    use super::*;

    fn dir(parts: &[&str]) -> DirPath {
        parts.iter().map(|part| part.as_bytes().to_vec()).collect()
    }

    fn names(report: &WalkReport) -> Vec<String> {
        let mut names: Vec<String> = report
            .files
            .iter()
            .map(|file| file.path.display_lossy())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn extensions_choose_the_container_and_ignore_case() {
        for (name, want) in [
            ("a.aif", Some("AIFF")),
            ("A.AIFF", Some("AIFF")),
            ("a.Mp3", Some("MP3")),
            ("a.flac", Some("FLAC")),
            ("a.wav", Some("WAV")),
            ("a.m4a", Some("M4A")),
            ("a.opus", Some("OGG")),
            ("a.txt", None),
            ("aif", None),
            ("a.", None),
            ("", None),
            (".mp3", Some("MP3")),
        ] {
            assert_eq!(container_for(name.as_bytes()), want, "{name}");
        }
    }

    #[test]
    fn a_walk_lists_audio_with_sizes_and_reports_every_directory() {
        let root = tempdir().unwrap();
        create_dir_all(root.path().join("Artist/Release")).unwrap();
        create_dir_all(root.path().join("Empty")).unwrap();
        write(
            root.path().join("Artist/Release/01 - A.AIF"),
            vec![0u8; 123],
        )
        .unwrap();
        write(root.path().join("Artist/Release/cover.jpg"), b"x").unwrap();
        write(root.path().join("Artist/song #1 + 100%.mp3"), b"yy").unwrap();
        write(root.path().join("top.flac"), b"").unwrap();
        let report = walk_root(root.path(), &WalkOptions::default()).unwrap();
        assert_eq!(
            names(&report),
            [
                "Artist/Release/01 - A.AIF",
                "Artist/song #1 + 100%.mp3",
                "top.flac"
            ]
        );
        let aiff = report
            .files
            .iter()
            .find(|file| file.container == "AIFF")
            .unwrap();
        assert_eq!(aiff.size, 123);
        assert!(
            aiff.mtime_ms > 1_000_000_000_000,
            "a plausible modified time"
        );
        let mut existing = report.existing_dirs.clone();
        existing.sort();
        assert_eq!(
            existing,
            [
                dir(&[]),
                dir(&["Artist"]),
                dir(&["Artist", "Release"]),
                dir(&["Empty"])
            ]
        );
        let mut listed = report.enumerated_dirs.clone();
        listed.sort();
        assert_eq!(listed, existing, "every directory was listed completely");
        assert!(report.links_skipped.is_empty() && !report.cancelled);
    }

    #[test]
    fn walking_changes_nothing_on_disk() {
        let root = tempdir().unwrap();
        create_dir_all(root.path().join("A")).unwrap();
        write(root.path().join("A/x.aif"), b"1").unwrap();
        let stamp = || {
            let dir = fs::metadata(root.path().join("A")).unwrap();
            let file = fs::metadata(root.path().join("A/x.aif")).unwrap();
            (
                dir.modified().unwrap(),
                file.modified().unwrap(),
                file.len(),
            )
        };
        let before = stamp();
        walk_root(root.path(), &WalkOptions::default()).unwrap();
        assert_eq!(stamp(), before);
    }

    #[test]
    fn a_root_that_cannot_be_listed_is_an_error() {
        let root = tempdir().unwrap();
        assert!(walk_root(&root.path().join("not-here"), &WalkOptions::default()).is_err());
        write(root.path().join("file"), b"").unwrap();
        assert!(walk_root(&root.path().join("file"), &WalkOptions::default()).is_err());
    }

    #[test]
    fn apple_double_files_and_unstorable_names_are_skipped_and_counted() {
        let root = tempdir().unwrap();
        write(root.path().join("._Track.mp3"), b"junk").unwrap();
        write(root.path().join("Track.mp3"), b"real").unwrap();
        let report = walk_root(root.path(), &WalkOptions::default()).unwrap();
        assert_eq!(names(&report), ["Track.mp3"]);
        assert_eq!(report.names_skipped, 1);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_reported_and_never_followed() {
        use std::os::unix::fs::symlink;
        let outside = tempdir().unwrap();
        write(outside.path().join("secret.mp3"), b"x").unwrap();
        let root = tempdir().unwrap();
        create_dir_all(root.path().join("Real")).unwrap();
        write(root.path().join("Real/in.mp3"), b"x").unwrap();
        symlink(outside.path(), root.path().join("Escape")).unwrap();
        symlink(
            outside.path().join("secret.mp3"),
            root.path().join("Real/alias.mp3"),
        )
        .unwrap();
        symlink(root.path().join("Real"), root.path().join("Loop")).unwrap();
        let report = walk_root(root.path(), &WalkOptions::default()).unwrap();
        assert_eq!(names(&report), ["Real/in.mp3"]);
        let mut links: Vec<String> = report
            .links_skipped
            .iter()
            .map(|link| String::from_utf8_lossy(link).replace('\0', "/"))
            .collect();
        links.sort();
        assert_eq!(links, ["Escape", "Loop", "Real/alias.mp3"]);
        assert!(!report.existing_dirs.contains(&dir(&["Escape"])));
    }

    #[cfg(unix)]
    #[test]
    fn names_that_are_not_text_are_kept_as_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let root = tempdir().unwrap();
        let odd = OsString::from_vec(vec![b'x', 0xFF, 0xFE, b'.', b'm', b'p', b'3']);
        if write(root.path().join(&odd), b"x").is_err() {
            return; // this file system refuses non-UTF-8 names (APFS does)
        }
        let report = walk_root(root.path(), &WalkOptions::default()).unwrap();
        assert_eq!(report.files.len(), 1);
        let component = report.files[0].path.components().next().unwrap();
        assert_eq!(component, [b'x', 0xFF, 0xFE, b'.', b'm', b'p', b'3']);
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_that_cannot_be_listed_is_found_but_not_listed() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempdir().unwrap();
        create_dir_all(root.path().join("Open")).unwrap();
        create_dir_all(root.path().join("Locked")).unwrap();
        write(root.path().join("Open/a.aif"), b"x").unwrap();
        write(root.path().join("Locked/b.aif"), b"x").unwrap();
        let locked = root.path().join("Locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let can_still_list = fs::read_dir(&locked).is_ok(); // running as root
        let report = walk_root(root.path(), &WalkOptions::default()).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if can_still_list {
            return;
        }
        assert_eq!(names(&report), ["Open/a.aif"]);
        assert!(report.existing_dirs.contains(&dir(&["Locked"])));
        assert!(!report.enumerated_dirs.contains(&dir(&["Locked"])));
        assert!(report.unlisted_dirs.contains(&dir(&["Locked"])));
        assert!(report.enumerated_dirs.contains(&dir(&["Open"])));
        assert!(report.enumerated_dirs.contains(&dir(&[])));
    }

    #[test]
    fn depth_and_cancellation_end_a_walk_without_claiming_more_than_was_seen() {
        let root = tempdir().unwrap();
        create_dir_all(root.path().join("a/b/c")).unwrap();
        write(root.path().join("a/b/c/deep.aif"), b"x").unwrap();
        write(root.path().join("a/top.aif"), b"x").unwrap();
        let shallow = walk_root(
            root.path(),
            &WalkOptions {
                max_depth: 2,
                cancel: None,
            },
        )
        .unwrap();
        assert_eq!(names(&shallow), ["a/top.aif"]);
        assert!(shallow.existing_dirs.contains(&dir(&["a", "b"])));
        assert!(
            !shallow.enumerated_dirs.contains(&dir(&["a", "b"])),
            "too deep to list"
        );

        let stop = AtomicBool::new(true);
        let cancelled = walk_root(
            root.path(),
            &WalkOptions {
                max_depth: 64,
                cancel: Some(&stop),
            },
        )
        .unwrap();
        assert!(cancelled.cancelled);
        assert!(
            cancelled.enumerated_dirs.is_empty(),
            "nothing listed, nothing concluded"
        );
    }

    #[test]
    fn unicode_names_are_reported_as_the_disk_spells_them() {
        let root = tempdir().unwrap();
        let name = "Cafe\u{301} \u{30c8}\u{30e9}\u{30c3}\u{30af}.flac";
        write(root.path().join(name), b"x").unwrap();
        let report = walk_root(root.path(), &WalkOptions::default()).unwrap();
        assert_eq!(report.files.len(), 1);
        let reported = report.files[0].path.components().next().unwrap().to_vec();
        // APFS preserves the spelling it was given; some volumes store another form. Either
        // way the bytes are exactly what the directory listing returned.
        let listed: Vec<Vec<u8>> = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| os_str_to_bytes(&entry.unwrap().file_name()))
            .collect();
        assert_eq!(listed, [reported]);
    }
}
