//! Files moved or renamed outside openCrate: where each went, and relinking its track to it, so
//! the track keeps its crate places, fields, date added and history. When rekordbox uses the file
//! (reading on), rekordbox is told the new place first, as tidying a folder tells it, so its cues,
//! grid and playlists stay; that needs writing on, and rekordbox closed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use oc_core_db::moved::Described;
use oc_core_db::now_ms;
use oc_core_db::service::{LibraryService, ScanOptions};
use oc_rekordbox::db::write;
use serde::Serialize;

use crate::rekordbox_db::{self, Probe};
use crate::rekordbox_write;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDto {
    pub file_id: i64,
    pub title: String,
    pub artist: String,
    /// "Music: Artist/Release/01 Title.aif".
    pub path: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MovedDto {
    pub missing: FileDto,
    pub found: FileDto,
    /// The same file on disk, not only one of the same name and size.
    pub same_file: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MovedFilesDto {
    pub moved: Vec<MovedDto>,
    /// Missing files with no likely new place.
    pub unmatched: Vec<FileDto>,
}

fn file_dto(described: Described) -> FileDto {
    FileDto {
        file_id: described.file_id,
        title: described.title,
        artist: described.artist,
        path: format!("{}: {}", described.folder, described.path),
    }
}

fn described(service: &LibraryService, ids: &[i64]) -> Result<HashMap<i64, FileDto>, String> {
    Ok(service
        .describe_files(ids)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|found| (found.file_id, file_dto(found)))
        .collect())
}

/// Every missing file, with where it most likely is now when that can be told.
pub fn moved_files(service: &LibraryService) -> Result<MovedFilesDto, String> {
    let failed = |error: oc_core_db::Error| error.to_string();
    let moved = service.moved_files().map_err(failed)?;
    let missing: Vec<i64> = service
        .missing_files()
        .map_err(failed)?
        .into_iter()
        .map(|(file, _)| file)
        .collect();
    let mut ids = missing.clone();
    ids.extend(moved.iter().map(|item| item.found_file_id));
    let mut files = described(service, &ids)?;
    let mut pairs = Vec::with_capacity(moved.len());
    for item in &moved {
        let (Some(from), Some(to)) = (
            files.remove(&item.missing_file_id),
            files.remove(&item.found_file_id),
        ) else {
            continue;
        };
        pairs.push(MovedDto {
            missing: from,
            found: to,
            same_file: item.same_file,
        });
    }
    let unmatched = missing
        .into_iter()
        .filter_map(|file| files.remove(&file))
        .collect();
    Ok(MovedFilesDto {
        moved: pairs,
        unmatched,
    })
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RelinkedDto {
    pub relinked: usize,
    /// Relinked files rekordbox was told about.
    pub relocated: usize,
    /// Tracks left as they were, and why.
    pub left: Vec<String>,
}

struct Plan {
    missing: i64,
    found: i64,
    title: String,
    root: i64,
    to: PathBuf,
    keys: Vec<String>,
}

/// Relink each `(missing file, found file)`: rekordbox first, where it uses the file, then the
/// library, then the music folders are read again.
pub fn relink(
    service: &LibraryService,
    probe: &Probe,
    pairs: &[(i64, i64)],
) -> Result<RelinkedDto, String> {
    let failed = |error: oc_core_db::Error| error.to_string();
    let reading = rekordbox_db::status(service, probe)?;
    let reading_on = reading.enabled && !reading.needs_confirmation;
    // Which files rekordbox uses now, not at some earlier read.
    if reading_on {
        rekordbox_db::read(service, probe)?;
    }
    let ids: Vec<i64> = pairs.iter().map(|(missing, _)| *missing).collect();
    let names = described(service, &ids)?;
    let mut done = RelinkedDto::default();
    let mut plans = Vec::with_capacity(pairs.len());
    for (missing, found) in pairs {
        let title = names
            .get(missing)
            .map_or_else(|| format!("File {missing}"), |file| file.title.clone());
        let Some((root, to)) = service.file_location(*found).map_err(failed)? else {
            done.left.push(format!(
                "{title}: the file it was to take is no longer there. Find moved files again."
            ));
            continue;
        };
        let keys = if reading_on {
            service.rekordbox_file_keys(*missing).map_err(failed)?
        } else {
            Vec::new()
        };
        plans.push(Plan {
            missing: *missing,
            found: *found,
            title,
            root,
            to,
            keys,
        });
    }

    if plans.iter().any(|plan| !plan.keys.is_empty()) {
        match rekordbox_write::ready(service, probe) {
            Ok(location) => {
                let relocations: Vec<(String, PathBuf)> = plans
                    .iter()
                    .flat_map(|plan| plan.keys.iter().map(|key| (key.clone(), plan.to.clone())))
                    .collect();
                write::relocate(
                    location,
                    &rekordbox_db::backups_dir(service),
                    probe.running_now(),
                    now_ms(),
                    &relocations,
                )
                .map_err(|error| {
                    format!("rekordbox could not be told where the files went ({error}); nothing was relinked.")
                })?;
                done.relocated = plans.iter().filter(|plan| !plan.keys.is_empty()).count();
            }
            Err(why) => {
                // Relinked here alone, rekordbox would keep looking in the old place with no
                // way left to tell it; those wait until it can be told.
                plans.retain(|plan| {
                    if plan.keys.is_empty() {
                        return true;
                    }
                    done.left.push(format!(
                        "{}: rekordbox uses this file, so it has to be told where it went. {why}",
                        plan.title
                    ));
                    false
                });
            }
        }
    }

    let mut roots = Vec::new();
    for plan in &plans {
        match service.relink(plan.missing, plan.found) {
            Ok(()) => {
                done.relinked += 1;
                if !roots.contains(&plan.root) {
                    roots.push(plan.root);
                }
            }
            Err(error) => done.left.push(format!("{}: {error}", plan.title)),
        }
    }
    for root in roots {
        service
            .scan_root(root, &ScanOptions::default())
            .map_err(failed)?;
    }
    if done.relocated > 0 {
        if let Err(error) = rekordbox_db::read(service, probe) {
            done.left.push(format!(
                "rekordbox was told the new places, but reading it again did not work: {error}"
            ));
        }
    }
    Ok(done)
}

/// The library file at `path`, which must be inside a music folder; that folder is read first
/// when the file is not listed yet.
pub fn file_at_path(service: &LibraryService, path: &Path) -> Result<i64, String> {
    let failed = |error: oc_core_db::Error| error.to_string();
    let path = oc_paths::canonical(path).map_err(|error| error.to_string())?;
    for info in service.roots().map_err(failed)? {
        let Ok(root) = oc_paths::bytes_to_os_string(&info.root.path_raw).map(PathBuf::from) else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(&root) else {
            continue;
        };
        let components: Vec<Vec<u8>> = relative
            .components()
            .map(|part| oc_paths::os_str_to_bytes(part.as_os_str()))
            .collect();
        if let Some(file) = service.file_at(info.root.id, &components).map_err(failed)? {
            return Ok(file);
        }
        service
            .scan_root(info.root.id, &ScanOptions::default())
            .map_err(failed)?;
        return service
            .file_at(info.root.id, &components)
            .map_err(failed)?
            .ok_or_else(|| "openCrate could not read that file as music.".to_owned());
    }
    Err(
        "That file is not in one of your music folders. Add its folder in Settings first, or move \
         the file into a music folder."
            .into(),
    )
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FoundInFolderDto {
    pub missing: FileDto,
    /// Where a copy was found, as a full path.
    pub path: String,
    /// The same size as the missing file, not only the same name.
    pub same_size: bool,
    /// Other files of that name in the folder, left out; the first is offered.
    pub others: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderSearchDto {
    pub folder: String,
    /// Audio files looked at.
    pub searched: usize,
    pub found: Vec<FoundInFolderDto>,
    /// Missing files with nothing of their name in the folder.
    pub not_found: usize,
}

/// The most files looked at in one folder, so a whole drive does not hang the screen.
const SEARCH_CAP: usize = 200_000;

/// Every audio file below `folder`, by lower-cased name. Links are not followed; folders that
/// cannot be read are skipped.
fn audio_below(folder: &Path) -> HashMap<String, Vec<(PathBuf, u64)>> {
    let mut by_name: HashMap<String, Vec<(PathBuf, u64)>> = HashMap::new();
    let mut pending = vec![folder.to_path_buf()];
    let mut seen = 0;
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let name = entry.file_name();
            let bytes = oc_paths::os_str_to_bytes(&name);
            if !matches!(oc_ingest::classify(&bytes), oc_ingest::EntryKind::Audio(_)) {
                continue;
            }
            seen += 1;
            if seen > SEARCH_CAP {
                return by_name;
            }
            let size = entry.metadata().map_or(0, |metadata| metadata.len());
            by_name
                .entry(name.to_string_lossy().to_lowercase())
                .or_default()
                .push((entry.path(), size));
        }
    }
    by_name
}

/// Look for every missing file below `folder`, by name, preferring the same size. Reads only.
pub fn find_in_folder(service: &LibraryService, folder: &Path) -> Result<FolderSearchDto, String> {
    let failed = |error: oc_core_db::Error| error.to_string();
    let missing = service.missing_file_details().map_err(failed)?;
    let ids: Vec<i64> = missing.iter().map(|file| file.file_id).collect();
    let mut names = described(service, &ids)?;
    let by_name = audio_below(folder);
    let searched = by_name.values().map(Vec::len).sum();
    let mut found = Vec::new();
    let mut not_found = 0;
    for file in missing {
        let Some(candidates) = by_name.get(&file.name.to_lowercase()) else {
            not_found += 1;
            continue;
        };
        let Some(dto) = names.remove(&file.file_id) else {
            continue;
        };
        let size = u64::try_from(file.size).unwrap_or(0);
        let same: Vec<&(PathBuf, u64)> = candidates
            .iter()
            .filter(|(_, found)| *found == size)
            .collect();
        let (path, same_size, others) = match same.first() {
            Some((path, _)) => (path.clone(), true, same.len() - 1),
            None => (candidates[0].0.clone(), false, candidates.len() - 1),
        };
        found.push(FoundInFolderDto {
            missing: dto,
            path: path.to_string_lossy().into_owned(),
            same_size,
            others,
        });
    }
    Ok(FolderSearchDto {
        folder: folder.to_string_lossy().into_owned(),
        searched,
        found,
        not_found,
    })
}

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoredDto {
    /// Files copied back to where the library expects them.
    pub restored: usize,
    /// Files left as they were, and why.
    pub left: Vec<String>,
}

/// Copy each found file back to the place its missing file had, so the track, its crates, its
/// fields and rekordbox's links all hold without a relink; then the music folders are read
/// again. Nothing is ever written over, and the found file is left where it was.
pub fn restore_from_folder(
    service: &LibraryService,
    pairs: &[(i64, PathBuf)],
) -> Result<RestoredDto, String> {
    let failed = |error: oc_core_db::Error| error.to_string();
    let ids: Vec<i64> = pairs.iter().map(|(missing, _)| *missing).collect();
    let names = described(service, &ids)?;
    let roots: HashMap<i64, (String, PathBuf)> = service
        .roots()
        .map_err(failed)?
        .into_iter()
        .filter_map(|info| {
            let path = oc_paths::bytes_to_os_string(&info.root.path_raw).ok()?;
            Some((info.root.id, (info.root.label, PathBuf::from(path))))
        })
        .collect();
    let mut done = RestoredDto::default();
    let mut touched = Vec::new();
    for (missing, source) in pairs {
        let title = names
            .get(missing)
            .map_or_else(|| format!("File {missing}"), |file| file.title.clone());
        let Some((root_id, destination)) = service.file_place(*missing).map_err(failed)? else {
            done.left
                .push(format!("{title}: it is no longer in the library."));
            continue;
        };
        if destination.exists() {
            done.left
                .push(format!("{title}: a file is back at its place already."));
            continue;
        }
        let Some((label, root_path)) = roots.get(&root_id) else {
            done.left
                .push(format!("{title}: its music folder is gone."));
            continue;
        };
        let outcome = (|| -> Result<(), String> {
            let staging = crate::import::staging_for(root_path, label)?;
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let staged = crate::import::staged_name(&staging, &destination);
            oc_publish::stage_copy(source, &staged, &mut oc_publish::NoFaults)
                .map_err(|error| error.to_string())?;
            match service.publish_file(staged.clone(), destination.clone()) {
                Ok(oc_publish::Outcome::Published) => Ok(()),
                Ok(oc_publish::Outcome::Rejected(_)) => {
                    let _ = std::fs::remove_file(&staged);
                    Err("something changed at its place while copying".to_owned())
                }
                Err(error) => {
                    if !destination.exists() {
                        let _ = std::fs::remove_file(&staged);
                    }
                    Err(error.to_string())
                }
            }
        })();
        match outcome {
            Ok(()) => {
                done.restored += 1;
                if !touched.contains(&root_id) {
                    touched.push(root_id);
                }
            }
            Err(why) => done.left.push(format!("{title}: {why}.")),
        }
    }
    for root in touched {
        service
            .scan_root(root, &ScanOptions::default())
            .map_err(failed)?;
    }
    Ok(done)
}
