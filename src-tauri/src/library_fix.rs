//! Fixing what the library check finds. Tidying a folder ("Artist;" becomes "Artist"): its files
//! move to the tidy name, never over a file already there, and rekordbox is told where each file
//! it uses went, so those tracks keep their analysis, cues, My Tags and playlists. Resolving a
//! duplicate: the copies not kept move to an archive folder beside the music folder (never
//! deleted), and where rekordbox has them, their playlist places, My Tags and rating go to the
//! copy kept before their entries are removed. Both need reading rekordbox on, so openCrate knows
//! which files rekordbox uses, and writing on when it uses any. If rekordbox cannot follow, the
//! files are moved back.

use std::fs;
use std::path::{Path, PathBuf};

use oc_core_db::now_ms;
use oc_core_db::service::{LibraryService, ScanOptions};
use oc_rekordbox::db::write::{self, Duplicate};
use serde::Serialize;

use crate::rekordbox_db::{self, Probe};
use crate::rekordbox_write;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TidiedDto {
    pub folder: String,
    pub suggested: String,
    pub moved: usize,
    /// Moved files rekordbox was told about.
    pub relocated: usize,
    /// Files left where they are, and why.
    pub left: Vec<String>,
}

pub(crate) struct Move {
    pub(crate) file_id: i64,
    pub(crate) from: PathBuf,
    pub(crate) to: PathBuf,
    /// rekordbox's ids for the file: none when it does not use it, more than one when it holds
    /// the file twice.
    pub(crate) keys: Vec<String>,
}

/// Moving files in a folder where rekordbox has entries matching more than one file could leave
/// those entries pointing at nothing, so the move waits until they are sorted out.
pub(crate) fn refuse_ambiguous(service: &LibraryService, root_id: i64) -> Result<(), String> {
    let ambiguous = service
        .rekordbox_ambiguous_in_root(root_id)
        .map_err(|error| error.to_string())?;
    if ambiguous > 0 {
        return Err(format!(
            "rekordbox has {ambiguous} {} in this music folder that could be more than one file \
             here (the rekordbox screen counts them), so openCrate moves nothing in it: an \
             entry might be left pointing at nothing.",
            if ambiguous == 1 { "entry" } else { "entries" }
        ));
    }
    Ok(())
}

/// Move each planned file to its tidy place, unless that place is taken on disk or still
/// listed for a file gone missing there (noted in `left`). A failed move puts back what moved.
pub(crate) fn move_tidied(
    service: &LibraryService,
    root_id: i64,
    moves: Vec<Move>,
    renamed: &std::collections::HashMap<i64, Vec<Vec<u8>>>,
    left: &mut Vec<String>,
) -> Result<Vec<Move>, String> {
    let mut done: Vec<Move> = Vec::new();
    for planned in moves {
        if planned.to.exists() {
            left.push(format!(
                "{} stays: there is already a file at {}.",
                planned.from.display(),
                planned.to.display()
            ));
            continue;
        }
        // A file that has gone missing there is still listed, with its own track; moving onto
        // it would mix the two up.
        let listed = match renamed.get(&planned.file_id) {
            Some(relative) => service
                .file_at(root_id, relative)
                .map_err(|error| error.to_string())?,
            None => None,
        };
        if listed.is_some_and(|other| other != planned.file_id) {
            left.push(format!(
                "{} stays: openCrate still lists a missing file at {}, with its own track.",
                planned.from.display(),
                planned.to.display()
            ));
            continue;
        }
        let outcome = planned
            .to
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::rename(&planned.from, &planned.to));
        if let Err(error) = outcome {
            return Err(format!(
                "{} could not be moved ({error}); {}.",
                planned.from.display(),
                undo(&done)
            ));
        }
        done.push(planned);
    }
    Ok(done)
}

pub fn tidy_folder(
    service: &LibraryService,
    probe: &Probe,
    root_id: i64,
    folder: &str,
) -> Result<TidiedDto, String> {
    let reading = rekordbox_db::status(service, probe)?;
    if !reading.enabled || reading.needs_confirmation {
        return Err(
            "Tidying moves files rekordbox may use. Turn on reading rekordbox's library in \
             Settings first, so openCrate knows which ones."
                .into(),
        );
    }
    // What rekordbox uses now, not at some earlier read.
    rekordbox_db::read(service, probe)?;
    let failed = |error: oc_core_db::Error| error.to_string();
    let untidy = service
        .check_health()
        .map_err(failed)?
        .untidy
        .into_iter()
        .find(|found| found.root_id == root_id && found.folder == folder)
        .ok_or_else(|| "That folder is already tidy, or no longer there.".to_owned())?;
    // The folder's own name is the last of its components.
    let depth = folder.split('/').count() - 1;
    let mut left = Vec::new();
    let mut moves = Vec::new();
    // Each file's path inside the music folder once moved, for the library to follow.
    let mut renamed = std::collections::HashMap::new();
    let mut old_folder = None;
    for (file_id, components) in service.files_below(root_id, folder).map_err(failed)? {
        let Some((_, from)) = service.file_location(file_id).map_err(failed)? else {
            left.push(format!("File {file_id} cannot be reached."));
            continue;
        };
        let Some(music) = from
            .ancestors()
            .nth(components.len())
            .map(Path::to_path_buf)
        else {
            continue;
        };
        let mut to = music.clone();
        let mut old = music;
        let mut relative = components.clone();
        if let Some(name) = relative.get_mut(depth) {
            *name = oc_paths::os_str_to_bytes(std::ffi::OsStr::new(&untidy.suggested));
        }
        renamed.insert(file_id, relative);
        for (index, part) in components.iter().enumerate() {
            let part = oc_paths::bytes_to_os_string(part).map_err(|error| error.to_string())?;
            if index == depth {
                to.push(&untidy.suggested);
            } else {
                to.push(&part);
            }
            if index <= depth {
                old.push(&part);
            }
        }
        old_folder.get_or_insert(old);
        let keys = service.rekordbox_file_keys(file_id).map_err(failed)?;
        moves.push(Move {
            file_id,
            from,
            to,
            keys,
        });
    }

    refuse_ambiguous(service, root_id)?;
    let location = if moves.iter().any(|planned| !planned.keys.is_empty()) {
        Some(rekordbox_write::ready(service, probe).map_err(|error| {
            format!(
                "rekordbox uses files in this folder, so it has to be told where they go. {error}"
            )
        })?)
    } else {
        None
    };

    let done = move_tidied(service, root_id, moves, &renamed, &mut left)?;

    let relocations: Vec<(String, PathBuf)> = done
        .iter()
        .flat_map(|moved| moved.keys.iter().map(|key| (key.clone(), moved.to.clone())))
        .collect();
    if let Some(location) = location.filter(|_| !relocations.is_empty()) {
        if let Err(error) = write::relocate(
            location,
            &rekordbox_db::backups_dir(service),
            probe.running(),
            now_ms(),
            &relocations,
        ) {
            return Err(format!(
                "rekordbox could not follow the moved files ({error}); {}.",
                undo(&done)
            ));
        }
    }
    if let Some(old) = &old_folder {
        remove_empty(old);
    }
    service
        .files_moved(
            root_id,
            done.iter()
                .filter_map(|moved| Some((moved.file_id, renamed.remove(&moved.file_id)?)))
                .collect(),
        )
        .map_err(failed)?;
    service
        .scan_root(root_id, &ScanOptions::default())
        .map_err(failed)?;
    if !relocations.is_empty() {
        if let Err(error) = rekordbox_db::read(service, probe) {
            left.push(format!(
                "rekordbox followed the files, but reading it again did not work: {error}"
            ));
        }
    }
    Ok(TidiedDto {
        folder: untidy.folder,
        suggested: untidy.suggested,
        moved: done.len(),
        relocated: relocations.len(),
        left,
    })
}

/// One copy of a recording filed more than once, as the page shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyDto {
    pub file_id: i64,
    /// Inside the music folder.
    pub path: String,
    /// The music folder it is in, so copies with the same path in two folders can be told apart.
    pub root_label: String,
    /// "AIFF", or "MP3, 320 kbps".
    pub format: String,
    pub in_rekordbox: bool,
    pub analysed: bool,
    pub cues: i64,
    pub playlists: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiesDto {
    pub copies: Vec<CopyDto>,
    /// The copy to keep by the rule, when the rule decides.
    pub suggested: Option<i64>,
    pub note: Option<String>,
}

const LOSSLESS: [&str; 3] = ["AIFF", "WAV", "FLAC"];

fn reading_fresh(service: &LibraryService, probe: &Probe) -> Result<(), String> {
    let reading = rekordbox_db::status(service, probe)?;
    if !reading.enabled || reading.needs_confirmation {
        return Err(
            "This needs to know which files rekordbox uses. Turn on reading rekordbox's library \
             in Settings first."
                .into(),
        );
    }
    rekordbox_db::read(service, probe).map(|_| ())
}

fn copies(
    service: &LibraryService,
    file_ids: &[i64],
) -> Result<Vec<oc_core_db::health::Copy>, String> {
    let mut found = Vec::with_capacity(file_ids.len());
    for file_id in file_ids {
        found.push(
            service
                .duplicate_copy(*file_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!("File {file_id} is no longer in the library. Check again.")
                })?,
        );
    }
    Ok(found)
}

/// The copies of one recording, read against a fresh read of rekordbox. The rule: the copy
/// rekordbox has analysed, then the one with the most cues, then lossless over lossy, then the
/// higher bitrate; when all that is equal, the copy rekordbox has, so its entry stays as it is.
/// When more than one copy has cues, the owner chooses.
pub fn duplicate_copies(
    service: &LibraryService,
    probe: &Probe,
    file_ids: &[i64],
) -> Result<CopiesDto, String> {
    reading_fresh(service, probe)?;
    let found = copies(service, file_ids)?;
    let labels: std::collections::HashMap<i64, String> = service
        .roots()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|info| (info.root.id, info.root.label))
        .collect();
    let with_cues = found.iter().filter(|copy| copy.cues > 0).count();
    let rank = |copy: &oc_core_db::health::Copy| {
        (
            copy.analysed,
            copy.cues,
            LOSSLESS.contains(&copy.container.as_str()),
            copy.bitrate_kbps.unwrap_or(0),
            copy.rb_key.is_some(),
            std::cmp::Reverse(copy.file_id),
        )
    };
    let suggested = (with_cues <= 1)
        .then(|| {
            found
                .iter()
                .max_by_key(|copy| rank(copy))
                .map(|copy| copy.file_id)
        })
        .flatten();
    Ok(CopiesDto {
        note: (with_cues > 1).then(|| {
            "More than one copy has cues in rekordbox. Choose which to keep; the others' cues are \
             not moved."
                .to_owned()
        }),
        copies: found
            .iter()
            .map(|copy| CopyDto {
                file_id: copy.file_id,
                path: copy
                    .relative
                    .iter()
                    .map(|part| String::from_utf8_lossy(part).into_owned())
                    .collect::<Vec<_>>()
                    .join("/"),
                root_label: labels.get(&copy.root_id).cloned().unwrap_or_default(),
                format: match copy.bitrate_kbps {
                    Some(rate) if !LOSSLESS.contains(&copy.container.as_str()) => {
                        format!("{}, {rate} kbps", copy.container)
                    }
                    _ => copy.container.clone(),
                },
                in_rekordbox: copy.rb_key.is_some(),
                analysed: copy.analysed,
                cues: copy.cues,
                playlists: copy.playlists,
            })
            .collect(),
        suggested,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedDto {
    /// Copies moved to the archive folder.
    pub archived: usize,
    /// rekordbox entries merged into the kept copy's.
    pub merged: usize,
    pub archive: String,
}

/// Keep one copy; archive the others and merge their rekordbox entries into the kept one's.
pub fn resolve_duplicate(
    service: &LibraryService,
    probe: &Probe,
    keep: i64,
    drop: &[i64],
) -> Result<ResolvedDto, String> {
    if drop.is_empty() || drop.contains(&keep) {
        return Err("Choose one copy to keep and at least one other.".into());
    }
    reading_fresh(service, probe)?;
    let kept = copies(service, &[keep])?.remove(0);
    let dropped = copies(service, drop)?;
    let in_rekordbox: Vec<&String> = dropped
        .iter()
        .filter_map(|copy| copy.rb_key.as_ref())
        .collect();
    let location = if in_rekordbox.is_empty() {
        None
    } else {
        if kept.rb_key.is_none() {
            return Err(
                "rekordbox has a copy you chose not to keep, with its analysis. Keep that copy \
                 instead; replacing an analysed copy with a better file is planned separately."
                    .into(),
            );
        }
        Some(rekordbox_write::ready(service, probe).map_err(|error| {
            format!("rekordbox has a copy that would go, so it has to be told. {error}")
        })?)
    };

    for root_id in dropped.iter().map(|copy| copy.root_id) {
        refuse_ambiguous(service, root_id)?;
    }
    // Plan every move first: nothing moves unless every one can.
    let mut moves = Vec::with_capacity(dropped.len());
    let mut archive_root = None;
    for copy in &dropped {
        let (_, from) = service
            .file_location(copy.file_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("File {} cannot be reached.", copy.file_id))?;
        let music = from
            .ancestors()
            .nth(copy.relative.len())
            .ok_or_else(|| "The music folder could not be found.".to_owned())?;
        let archive = archive_for(music).ok_or_else(|| {
            format!(
                "{} is the top of a drive, so there is nowhere beside it on that drive to archive to. \
                 Nothing was moved.",
                music.display()
            )
        })?;
        let mut to = archive.clone();
        for part in &copy.relative {
            to.push(oc_paths::bytes_to_os_string(part).map_err(|error| error.to_string())?);
        }
        if to.exists() {
            return Err(format!(
                "{} is already in the archive, so nothing was moved.",
                to.display()
            ));
        }
        archive_root.get_or_insert(archive);
        moves.push(Move {
            file_id: copy.file_id,
            from,
            to,
            keys: service
                .rekordbox_file_keys(copy.file_id)
                .map_err(|error| error.to_string())?,
        });
    }
    let mut done: Vec<Move> = Vec::new();
    for planned in moves {
        let outcome = planned
            .to
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::rename(&planned.from, &planned.to));
        if let Err(error) = outcome {
            return Err(format!(
                "{} could not be archived ({error}); {}.",
                planned.from.display(),
                undo(&done)
            ));
        }
        done.push(planned);
    }

    let merges: Vec<Duplicate> = match &kept.rb_key {
        Some(kept_key) => done
            .iter()
            .flat_map(|moved| moved.keys.iter().cloned())
            .filter(|key| key != kept_key)
            .map(|dropped| Duplicate {
                kept: kept_key.clone(),
                dropped,
            })
            .collect(),
        None => Vec::new(),
    };
    if let Some(location) = location.filter(|_| !merges.is_empty()) {
        if let Err(error) = write::merge_duplicates(
            location,
            &rekordbox_db::backups_dir(service),
            probe.running(),
            now_ms(),
            &merges,
        ) {
            return Err(format!("{error} Archived files: {}.", undo(&done)));
        }
    }
    // The archived copies are out of the music folders now; the library forgets them rather
    // than a scan calling them missing.
    service
        .forget_archived(keep, dropped.iter().map(|copy| copy.file_id).collect())
        .map_err(|error| error.to_string())?;
    let mut roots = vec![kept.root_id];
    roots.extend(dropped.iter().map(|copy| copy.root_id));
    roots.dedup();
    for root_id in roots {
        service
            .scan_root(root_id, &ScanOptions::default())
            .map_err(|error| error.to_string())?;
    }
    if !merges.is_empty() {
        let _ = rekordbox_db::read(service, probe);
    }
    Ok(ResolvedDto {
        archived: done.len(),
        merged: merges.len(),
        archive: archive_root
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
    })
}

/// Where a music folder's archived copies go: a folder named after it inside "openCrate archive"
/// beside it, never inside it (a scan would bring them straight back). None for a drive's root.
pub(crate) fn archive_for(music: &Path) -> Option<PathBuf> {
    let parent = music.parent()?;
    let name = music.file_name()?;
    if is_mount_root(music, parent) {
        return None;
    }
    Some(parent.join("openCrate archive").join(name))
}

/// A music folder that is the top of a mounted drive (`/Volumes/Music`): beside it is another
/// volume (or the folder of mount points), where an archive cannot be made by a rename.
#[cfg(unix)]
fn is_mount_root(music: &Path, parent: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(music), fs::metadata(parent)) {
        (Ok(inside), Ok(beside)) => inside.dev() != beside.dev(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn is_mount_root(_music: &Path, _parent: &Path) -> bool {
    false
}

/// Move files back, newest first, and say how that went: "everything moved so far was put back",
/// or which files could not be and where they are now.
pub(crate) fn undo(done: &[Move]) -> String {
    let stuck: Vec<String> = done
        .iter()
        .rev()
        .filter(|moved| fs::rename(&moved.to, &moved.from).is_err())
        .map(|moved| format!("{} is at {}", moved.from.display(), moved.to.display()))
        .collect();
    if stuck.is_empty() {
        "everything moved so far was put back".to_owned()
    } else {
        format!(
            "{} could not be put back: {}",
            if stuck.len() == 1 {
                "one file"
            } else {
                "some files"
            },
            stuck.join("; ")
        )
    }
}

/// Remove `dir` and folders below it that are now empty. A folder with anything left in it stays.
fn remove_empty(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                remove_empty(&entry.path());
            }
        }
    }
    let _ = fs::remove_dir(dir);
}
