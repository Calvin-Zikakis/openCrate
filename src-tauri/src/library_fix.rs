//! Tidying a folder the library check names ("Artist;" becomes "Artist"). Its files move to the
//! tidy name, never over a file already there, and rekordbox is told where each file it uses
//! went, so those tracks keep their analysis, cues, My Tags and playlists. It needs reading
//! rekordbox on, so openCrate knows which files rekordbox uses, and writing on when it uses any.
//! If rekordbox cannot follow, the files are moved back.

use std::fs;
use std::path::{Path, PathBuf};

use oc_core_db::now_ms;
use oc_core_db::service::{LibraryService, ScanOptions};
use oc_rekordbox::db::write;
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

struct Move {
    from: PathBuf,
    to: PathBuf,
    /// rekordbox's id for the file, when it uses it.
    key: Option<String>,
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
        let key = service.rekordbox_file_key(file_id).map_err(failed)?;
        moves.push(Move { from, to, key });
    }

    let location = if moves.iter().any(|planned| planned.key.is_some()) {
        Some(rekordbox_write::ready(service, probe).map_err(|error| {
            format!(
                "rekordbox uses files in this folder, so it has to be told where they go. {error}"
            )
        })?)
    } else {
        None
    };

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
        let outcome = planned
            .to
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::rename(&planned.from, &planned.to));
        if let Err(error) = outcome {
            undo(&done);
            return Err(format!(
                "{} could not be moved ({error}); everything moved so far was put back.",
                planned.from.display()
            ));
        }
        done.push(planned);
    }

    let relocations: Vec<(String, PathBuf)> = done
        .iter()
        .filter_map(|moved| moved.key.clone().map(|key| (key, moved.to.clone())))
        .collect();
    if let Some(location) = location.filter(|_| !relocations.is_empty()) {
        if let Err(error) = write::relocate(
            location,
            &rekordbox_db::backups_dir(service),
            probe.running(),
            now_ms(),
            &relocations,
        ) {
            undo(&done);
            return Err(format!(
                "rekordbox could not follow the moved files ({error}); the files were put back."
            ));
        }
    }
    if let Some(old) = &old_folder {
        remove_empty(old);
    }
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

/// Move files back, newest first.
fn undo(done: &[Move]) {
    for moved in done.iter().rev() {
        let _ = fs::rename(&moved.to, &moved.from);
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
