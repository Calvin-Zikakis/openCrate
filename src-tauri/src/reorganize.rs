//! Reorganizing the library: an experimental Clean up tool that files existing music by the
//! folder layout in sittings. The moves themselves are the Folder layout card's (`refile`); this
//! module keeps the sitting, and puts the last one back: every file goes to where it was, in the
//! reverse order, never over another file, and rekordbox is told again.

use std::collections::HashMap;
use std::path::PathBuf;

use oc_core_db::now_ms;
use oc_core_db::service::{LibraryService, ScanOptions};
use oc_rekordbox::db::write;
use serde::Serialize;

use crate::library_fix::{move_tidied, refuse_ambiguous, undo, Move};
use crate::rekordbox_db::{self, Probe};
use crate::rekordbox_write;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SittingDto {
    pub id: i64,
    pub started_ms: i64,
    pub moves: i64,
    /// Already put back.
    pub undone: bool,
}

/// The newest sitting, if any.
pub fn last(service: &LibraryService) -> Result<Option<SittingDto>, String> {
    Ok(service
        .last_reorganize()
        .map_err(|error| error.to_string())?
        .map(|session| SittingDto {
            id: session.id,
            started_ms: session.started_ms,
            moves: session.moves,
            undone: session.undone_ms.is_some(),
        }))
}

pub fn begin(service: &LibraryService) -> Result<i64, String> {
    service
        .begin_reorganize()
        .map_err(|error| error.to_string())
}

/// Tracks whose tags lack something, for the tag step.
pub fn thin_tracks(service: &LibraryService, limit: usize) -> Result<Vec<i64>, String> {
    service
        .thin_tracks(limit)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UndoneDto {
    pub put_back: usize,
    pub relocated: usize,
    pub left: Vec<String>,
}

fn components(raw: &[u8]) -> Vec<Vec<u8>> {
    raw.split(|byte| *byte == b'/')
        .filter(|part| !part.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

/// Put the sitting's files back where they were. Refused when it was put back already.
pub fn undo_sitting(
    service: &LibraryService,
    probe: &Probe,
    session_id: i64,
) -> Result<UndoneDto, String> {
    let failed = |error: oc_core_db::Error| error.to_string();
    let Some(session) = service.last_reorganize().map_err(failed)? else {
        return Err("There is no sitting to put back.".into());
    };
    if session.id != session_id {
        return Err("Only the last sitting can be put back, and this is not it.".into());
    }
    if session.undone_ms.is_some() {
        return Err("That sitting was put back already.".into());
    }
    let reading = rekordbox_db::status(service, probe)?;
    if !reading.enabled || reading.needs_confirmation {
        return Err(
            "Putting files back moves files rekordbox may use. Turn on reading rekordbox's \
             library in Settings first, so openCrate knows which ones."
                .into(),
        );
    }
    rekordbox_db::read(service, probe)?;
    let recorded = service.reorganize_moves(session_id).map_err(failed)?;
    let roots: HashMap<i64, PathBuf> = service
        .roots()
        .map_err(failed)?
        .into_iter()
        .filter_map(|info| {
            let path = oc_paths::bytes_to_os_string(&info.root.path_raw).ok()?;
            Some((info.root.id, PathBuf::from(path)))
        })
        .collect();
    let mut by_root: HashMap<i64, Vec<oc_core_db::reorganize::ReorganizeMove>> = HashMap::new();
    for item in recorded {
        by_root.entry(item.root_id).or_default().push(item);
    }
    let mut done = UndoneDto::default();
    for (root_id, items) in by_root {
        let Some(root) = roots.get(&root_id) else {
            done.left
                .push("A music folder of the sitting is gone; its files stay.".into());
            continue;
        };
        refuse_ambiguous(service, root_id)?;
        let mut renamed: HashMap<i64, Vec<Vec<u8>>> = HashMap::new();
        let mut planned = Vec::new();
        for item in &items {
            let there = oc_paths::bytes_to_os_string(&item.to_raw)
                .map(|path| root.join(path))
                .ok();
            let now = match service.file_location(item.file_id).map_err(failed)? {
                Some((_, now)) if there.as_ref() == Some(&now) => now,
                _ => {
                    done.left.push(format!(
                        "{} is not where the sitting left it; it stays.",
                        String::from_utf8_lossy(&item.to_raw)
                    ));
                    continue;
                }
            };
            let keys = service.rekordbox_file_keys(item.file_id).map_err(failed)?;
            let back: PathBuf = oc_paths::bytes_to_os_string(&item.from_raw)
                .map(PathBuf::from)
                .map_err(|_| "A recorded path cannot be read here.".to_owned())?;
            renamed.insert(item.file_id, components(&item.from_raw));
            planned.push(Move {
                file_id: item.file_id,
                from: now,
                to: root.join(back),
                keys,
            });
        }
        let location = if planned.iter().any(|item| !item.keys.is_empty()) {
            Some(rekordbox_write::ready(service, probe).map_err(|error| {
                format!("rekordbox uses some of these files, so it has to be told where they go. {error}")
            })?)
        } else {
            None
        };
        let moved = move_tidied(service, root_id, planned, &renamed, &mut done.left)?;
        let relocations: Vec<(String, PathBuf)> = moved
            .iter()
            .flat_map(|item| item.keys.iter().map(|key| (key.clone(), item.to.clone())))
            .collect();
        if let Some(location) = location.filter(|_| !relocations.is_empty()) {
            if let Err(error) = write::relocate(
                location,
                &rekordbox_db::backups_dir(service),
                probe.running_now(),
                now_ms(),
                &relocations,
            ) {
                return Err(format!(
                    "rekordbox could not follow the files back ({error}); {}.",
                    undo(&moved)
                ));
            }
        }
        for item in &moved {
            if let Some(parent) = item.from.parent() {
                let mut at = parent.to_path_buf();
                while at.starts_with(root) && at != *root && std::fs::remove_dir(&at).is_ok() {
                    if !at.pop() {
                        break;
                    }
                }
            }
        }
        service
            .files_moved(
                root_id,
                moved
                    .iter()
                    .filter_map(|item| Some((item.file_id, renamed.remove(&item.file_id)?)))
                    .collect(),
            )
            .map_err(failed)?;
        service
            .scan_root(root_id, &ScanOptions::default())
            .map_err(failed)?;
        done.put_back += moved.len();
        done.relocated += relocations.len();
    }
    service.mark_reorganize_undone(session_id).map_err(failed)?;
    if done.relocated > 0 {
        if let Err(error) = rekordbox_db::read(service, probe) {
            done.left.push(format!(
                "rekordbox followed the files, but reading it again did not work: {error}"
            ));
        }
    }
    Ok(done)
}
