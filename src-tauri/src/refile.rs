//! Re-filing music already in the music folders by the folder layout, which otherwise applies to
//! imports only. Each file's place is worked out from its own tags, the way an import works it
//! out; nothing moves until the owner ticks it, never over a file already there, and rekordbox is
//! told where each file it uses went, as tidying a folder tells it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use oc_core_db::now_ms;
use oc_core_db::service::{LibraryService, ScanOptions};
use oc_organize::{
    destination, folder_artist, full_title, parse_title, Fields, PathBudget, Template,
    DEFAULT_TEMPLATE,
};
use oc_rekordbox::db::write;
use serde::Serialize;

use crate::library_fix::{move_tidied, refuse_ambiguous, undo, Move};
use crate::plan::LAYOUT_KEY;
use crate::rekordbox_db::{self, Probe};
use crate::rekordbox_write;

/// Threads reading tags at once.
const READERS: usize = 4;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefileMoveDto {
    pub file_id: i64,
    pub root_label: String,
    /// Inside the music folder, with "/" between the parts.
    pub from: String,
    pub to: String,
    pub in_rekordbox: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefileSkipDto {
    pub path: String,
    pub why: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefilePlanDto {
    pub layout: String,
    pub moves: Vec<RefileMoveDto>,
    /// Files whose place cannot be worked out, or is taken.
    pub skipped: Vec<RefileSkipDto>,
    /// Files already where the layout puts them.
    pub in_place: usize,
}

struct Planned {
    root_id: i64,
    root: PathBuf,
    item: RefileMoveDto,
    /// The new place inside the music folder, as components.
    relative: Vec<Vec<u8>>,
}

fn shown(components: &[Vec<u8>]) -> String {
    components
        .iter()
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Where a file belongs by `template`, from its own tags; the reason when that cannot be said.
fn place_of(template: &Template, root_text: &str, path: &Path) -> Result<Vec<String>, String> {
    let info =
        oc_tag::read_file(path).map_err(|error| format!("its tags could not be read ({error})"))?;
    let tags = info.tags;
    let (Some(artist), Some(title)) = (tags.artist.clone(), tags.title.clone()) else {
        return Err("it has no artist or title in its tags".into());
    };
    let parsed = parse_title(&title);
    let fields = Fields {
        folder_artist: Some(folder_artist(
            &parsed,
            tags.album_artist.as_deref(),
            &artist,
        )),
        release: tags.album.clone(),
        artist: Some(artist),
        album_artist: tags.album_artist.clone(),
        title: Some(full_title(&parsed)),
        track: tags.track_number,
        disc: tags.disc_number,
        year: tags.year.and_then(|year| u32::try_from(year).ok()),
        ..Fields::default()
    };
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned())
        .unwrap_or_default();
    destination(
        template,
        &fields,
        &extension,
        root_text,
        &PathBudget::classic(),
    )
    .map(|made| made.components)
    .map_err(|error| match error {
        oc_organize::DestinationError::Template(oc_organize::TemplateError::Missing(field)) => {
            format!("it has no {} in its tags", field.replace('_', " "))
        }
        other => other.to_string(),
    })
}

fn planned(
    service: &LibraryService,
) -> Result<(String, Vec<Planned>, Vec<RefileSkipDto>, usize), String> {
    let failed = |error: oc_core_db::Error| error.to_string();
    let layout = service
        .setting(LAYOUT_KEY)
        .map_err(failed)?
        .unwrap_or_else(|| DEFAULT_TEMPLATE.to_owned());
    let template = Template::parse(&layout).map_err(|error| error.to_string())?;
    let mut moves = Vec::new();
    let mut skipped = Vec::new();
    let mut in_place = 0;
    for info in service.roots().map_err(failed)? {
        if info.root.availability != oc_paths::Availability::Available {
            continue;
        }
        let Ok(root) = oc_paths::bytes_to_os_string(&info.root.path_raw).map(PathBuf::from) else {
            continue;
        };
        let root_text = root.to_string_lossy().into_owned();
        let files = service.files_below(info.root.id, "").map_err(failed)?;
        let chunk = files.len().div_ceil(READERS).max(1);
        let places: Vec<Result<Vec<String>, String>> = std::thread::scope(|scope| {
            let workers: Vec<_> = files
                .chunks(chunk)
                .map(|part| {
                    let (template, root, root_text) = (&template, &root, &root_text);
                    scope.spawn(move || {
                        part.iter()
                            .map(|(_, components)| {
                                let path =
                                    components.iter().try_fold(root.clone(), |path, part| {
                                        oc_paths::bytes_to_os_string(part)
                                            .map(|part| path.join(part))
                                    });
                                match path {
                                    Ok(path) => place_of(template, root_text, &path),
                                    Err(_) => Err("its name cannot be read".into()),
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap_or_default())
                .collect()
        });
        for ((file_id, components), place) in files.iter().zip(places) {
            let from = shown(components);
            match place {
                Err(why) => skipped.push(RefileSkipDto {
                    path: format!("{}: {from}", info.root.label),
                    why,
                }),
                Ok(place) if place.join("/") == from => in_place += 1,
                Ok(place) => {
                    let relative: Vec<Vec<u8>> = place
                        .iter()
                        .map(|part| oc_paths::os_str_to_bytes(std::ffi::OsStr::new(part)))
                        .collect();
                    moves.push(Planned {
                        root_id: info.root.id,
                        root: root.clone(),
                        item: RefileMoveDto {
                            file_id: *file_id,
                            root_label: info.root.label.clone(),
                            from,
                            to: place.join("/"),
                            in_rekordbox: !service
                                .rekordbox_file_keys(*file_id)
                                .map_err(failed)?
                                .is_empty(),
                        },
                        relative,
                    });
                }
            }
        }
    }
    // Two files bound for one place, or a place a file already holds: neither is guessed at.
    let mut bound: HashMap<(i64, String), usize> = HashMap::new();
    for plan in &moves {
        *bound
            .entry((plan.root_id, plan.item.to.to_lowercase()))
            .or_default() += 1;
    }
    moves.retain(|plan| {
        let why = if bound[&(plan.root_id, plan.item.to.to_lowercase())] > 1 {
            Some("another file would go to the same place")
        } else if plan.root.join(&plan.item.to).exists() {
            Some("a file is already where it would go")
        } else {
            None
        };
        if let Some(why) = why {
            skipped.push(RefileSkipDto {
                path: format!("{}: {}", plan.item.root_label, plan.item.from),
                why: why.to_owned(),
            });
        }
        why.is_none()
    });
    Ok((layout, moves, skipped, in_place))
}

/// Every file whose place by the folder layout differs from where it is. Reads each file's tags.
pub fn refile_plan(service: &LibraryService) -> Result<RefilePlanDto, String> {
    let (layout, moves, skipped, in_place) = planned(service)?;
    Ok(RefilePlanDto {
        layout,
        moves: moves.into_iter().map(|plan| plan.item).collect(),
        skipped,
        in_place,
    })
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RefiledDto {
    pub moved: usize,
    /// Moved files rekordbox was told about.
    pub relocated: usize,
    /// Files left where they are, and why.
    pub left: Vec<String>,
}

/// Remove `dir` and the folders above it, up to `root`, while they are empty.
fn remove_empty_up(dir: &Path, root: &Path) {
    let mut at = dir.to_path_buf();
    while at.starts_with(root) && at != root && std::fs::remove_dir(&at).is_ok() {
        if !at.pop() {
            break;
        }
    }
}

/// Move the chosen files to their places by the folder layout, worked out afresh. Needs reading
/// rekordbox on, so openCrate knows which files rekordbox uses, and writing on when it uses any.
pub fn refile(
    service: &LibraryService,
    probe: &Probe,
    file_ids: &[i64],
) -> Result<RefiledDto, String> {
    let reading = rekordbox_db::status(service, probe)?;
    if !reading.enabled || reading.needs_confirmation {
        return Err(
            "Re-filing moves files rekordbox may use. Turn on reading rekordbox's library in \
             Settings first, so openCrate knows which ones."
                .into(),
        );
    }
    // What rekordbox uses now, not at some earlier read.
    rekordbox_db::read(service, probe)?;
    let failed = |error: oc_core_db::Error| error.to_string();
    let chosen: HashSet<i64> = file_ids.iter().copied().collect();
    let (_, plans, _, _) = planned(service)?;
    let mut done = RefiledDto::default();
    let mut by_root: HashMap<i64, Vec<Planned>> = HashMap::new();
    for plan in plans
        .into_iter()
        .filter(|plan| chosen.contains(&plan.item.file_id))
    {
        by_root.entry(plan.root_id).or_default().push(plan);
    }
    if by_root.is_empty() {
        return Err("Those files are already in place, or no longer there. Check again.".into());
    }
    for (root_id, plans) in by_root {
        refuse_ambiguous(service, root_id)?;
        let root = plans[0].root.clone();
        let mut renamed = HashMap::new();
        let mut planned_moves = Vec::new();
        for plan in plans {
            let Some((_, from)) = service.file_location(plan.item.file_id).map_err(failed)? else {
                done.left
                    .push(format!("{} cannot be reached.", plan.item.from));
                continue;
            };
            let keys = service
                .rekordbox_file_keys(plan.item.file_id)
                .map_err(failed)?;
            renamed.insert(plan.item.file_id, plan.relative);
            planned_moves.push(Move {
                file_id: plan.item.file_id,
                from,
                to: root.join(&plan.item.to),
                keys,
            });
        }
        let location = if planned_moves.iter().any(|planned| !planned.keys.is_empty()) {
            Some(rekordbox_write::ready(service, probe).map_err(|error| {
                format!("rekordbox uses some of these files, so it has to be told where they go. {error}")
            })?)
        } else {
            None
        };
        let moved = move_tidied(service, root_id, planned_moves, &renamed, &mut done.left)?;
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
                    "rekordbox could not follow the moved files ({error}); {}.",
                    undo(&moved)
                ));
            }
        }
        for item in &moved {
            if let Some(parent) = item.from.parent() {
                remove_empty_up(parent, &root);
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
        done.moved += moved.len();
        done.relocated += relocations.len();
    }
    if done.relocated > 0 {
        if let Err(error) = rekordbox_db::read(service, probe) {
            done.left.push(format!(
                "rekordbox followed the files, but reading it again did not work: {error}"
            ));
        }
    }
    Ok(done)
}
