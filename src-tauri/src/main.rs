#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art_fix;
#[cfg(test)]
mod art_fix_tests;
mod artwork;
mod crates;
mod downloads;
mod editor;
#[cfg(test)]
mod editor_tests;
mod export;
#[cfg(test)]
mod export_tests;
mod folders;
mod import;
mod ipc;
mod layout;
mod library_fix;
#[cfg(test)]
mod library_fix_tests;
mod lookup;
#[cfg(target_os = "macos")]
mod menu;
mod plan;
mod player;
mod rekordbox_db;
mod rekordbox_merge;
mod rekordbox_send;
mod rekordbox_write;
#[cfg(test)]
mod rekordbox_write_tests;
mod sync;
mod tagview;
mod waves;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use oc_core_db::service::LibraryService;
use oc_core_db::synthetic::MAX_TRACKS;
use tauri::{Manager, RunEvent, State};
use tauri_plugin_dialog::DialogExt;

const DEMO_PREFIX: &str = "opencrate-demo-";
const STALE_AFTER: Duration = Duration::from_hours(24);

/// A killed app, or a normal exit on a platform that cannot delete open files, leaves its
/// demo database behind. Remove old ones, and only folders this app would have made.
fn remove_stale_demos(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let is_ours = entry.file_name().to_string_lossy().starts_with(DEMO_PREFIX);
        let is_real_dir = entry
            .file_type()
            .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink());
        let is_old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > STALE_AFTER);
        if is_ours && is_real_dir && is_old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The open library, if any. With neither `OPENCRATE_DATA` nor `OPENCRATE_DEMO` there is
/// none and the UI keeps its in-memory demo catalog, so nothing is read or written on disk.
struct AppState {
    service: Option<Arc<LibraryService>>,
    scan: Arc<folders::ScanControl>,
    downloads: Arc<downloads::DownloadWatch>,
    art: Arc<artwork::ArtCache>,
    lookups: Arc<lookup::Lookups>,
    /// Started with the app; the sound device opens when the first track loads.
    player: Arc<oc_player::Player>,
    /// Close-up waveforms of the last few tracks played.
    waves: Arc<waves::DetailCache>,
    /// The database lives in a folder the owner named and survives restarts.
    persistent: bool,
    /// Removed when the process ends. Holds the demo database.
    demo_directory: Option<tempfile::TempDir>,
    /// Set when an `OPENCRATE_*` setting was given but could not be used, so the window can
    /// say so instead of quietly showing the demo.
    startup_error: Option<String>,
}

impl AppState {
    fn none() -> Self {
        Self {
            service: None,
            scan: Arc::default(),
            downloads: Arc::default(),
            art: Arc::default(),
            lookups: Arc::default(),
            player: Arc::new(oc_player::Player::start()),
            waves: Arc::default(),
            persistent: false,
            demo_directory: None,
            startup_error: None,
        }
    }

    /// `OPENCRATE_DATA=<folder>` opens a persistent library there; `OPENCRATE_DEMO=<tracks>`
    /// opens a synthetic one in a temporary folder; otherwise the library lives in `default`, the
    /// app's local data folder (A86). The first that is set wins.
    fn from_environment(default: Option<&Path>) -> Self {
        let data = std::env::var("OPENCRATE_DATA").ok();
        let demo = std::env::var("OPENCRATE_DEMO").ok();
        Self::from_settings(
            data.as_deref(),
            demo.as_deref(),
            &std::env::temp_dir(),
            default,
        )
    }

    fn from_settings(
        data: Option<&str>,
        demo: Option<&str>,
        temp: &Path,
        default: Option<&Path>,
    ) -> Self {
        if let Some(setting) = data {
            match Self::open_persistent(Path::new(setting.trim())) {
                Ok(state) => return state,
                Err(message) => {
                    return Self::failed(format!(
                        "The library folder could not be opened: {message}"
                    ));
                }
            }
        }
        let Some(setting) = demo else {
            let Some(folder) = default else {
                return Self::none();
            };
            return Self::open_persistent(folder).unwrap_or_else(|message| {
                Self::failed(format!("The library folder could not be opened: {message}"))
            });
        };
        match Self::open_demo(setting.trim(), temp) {
            Ok(state) => state,
            Err(message) => {
                Self::failed(format!("The demo library could not be opened: {message}"))
            }
        }
    }

    /// No library, and a reason the window will show.
    fn failed(message: String) -> Self {
        eprintln!("{message}");
        Self {
            startup_error: Some(message),
            ..Self::none()
        }
    }

    fn open_persistent(directory: &Path) -> Result<Self, String> {
        if !directory.is_absolute() {
            return Err("expected an absolute folder path".to_owned());
        }
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let service = LibraryService::open(directory, 4).map_err(|error| error.to_string())?;
        // An import a crash cut short is finished or set aside before anything else runs.
        if let Err(error) = service.recover_publications() {
            eprintln!("Unfinished imports could not be checked: {error}");
        }
        Ok(Self {
            service: Some(Arc::new(service)),
            persistent: true,
            ..Self::none()
        })
    }

    fn open_demo(setting: &str, parent: &Path) -> Result<Self, String> {
        let tracks: u32 = setting
            .parse()
            .ok()
            .filter(|count| (1..=MAX_TRACKS).contains(count))
            .ok_or_else(|| format!("expected a track count from 1 to {MAX_TRACKS}"))?;
        remove_stale_demos(parent);
        let directory = tempfile::Builder::new()
            .prefix(DEMO_PREFIX)
            .tempdir_in(parent)
            .map_err(|error| error.to_string())?;
        let service =
            LibraryService::open(directory.path(), 4).map_err(|error| error.to_string())?;
        service
            .seed_demo(tracks)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            service: Some(Arc::new(service)),
            demo_directory: Some(directory),
            ..Self::none()
        })
    }

    fn library(&self) -> Result<Arc<LibraryService>, String> {
        self.service
            .clone()
            .ok_or_else(|| "No database library is open.".to_owned())
    }
}

/// Whether the UI should query the Rust library, and what it may do with it.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its State by value"
)]
fn library_mode(state: State<'_, AppState>) -> folders::ModeDto {
    folders::ModeDto {
        mode: if state.service.is_some() {
            "database"
        } else {
            "synthetic"
        },
        can_manage_folders: state.service.is_some(),
        persistent: state.persistent,
        error: state.startup_error.clone(),
    }
}

#[tauri::command]
async fn library_read(
    state: State<'_, AppState>,
    query: ipc::QueryDto,
    start: usize,
    count: usize,
) -> Result<ipc::PageDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || ipc::read(&service, &query, start, count))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_roots(state: State<'_, AppState>) -> Result<Vec<folders::RootDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::roots(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Ask the owner for a folder with the system dialog and register it. The page never supplies
/// a path. `None` means the dialog was cancelled.
#[tauri::command]
async fn library_add_root(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<folders::RootDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .set_title("Choose a music folder")
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|error| error.to_string())?;
        folders::add_root(&service, &path).map(Some)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_watch_folders(
    state: State<'_, AppState>,
) -> Result<Vec<folders::WatchFolderDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::watch_folders(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Look at every download folder (read-only) so an unplugged drive is noticed.
#[tauri::command]
async fn library_check_watch_folders(
    state: State<'_, AppState>,
) -> Result<Vec<folders::WatchFolderDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::check_watch_folders(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Ask the owner for a download folder with the system dialog and add it. The page never
/// supplies a path. `None` means the dialog was cancelled.
#[tauri::command]
async fn library_add_watch_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<folders::WatchFolderDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .set_title("Choose a download folder")
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|error| error.to_string())?;
        folders::add_watch_folder(&service, &path).map(Some)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// What is in the download folders now, and which releases look finished. Read only.
#[tauri::command]
async fn library_download_releases(
    state: State<'_, AppState>,
) -> Result<downloads::DownloadsDto, String> {
    let service = state.library()?;
    let watch = Arc::clone(&state.downloads);
    tauri::async_runtime::spawn_blocking(move || downloads::poll_now(&service, &watch))
        .await
        .map_err(|error| error.to_string())?
}

/// A dry run of importing one release: where each file would go. Reads only.
#[tauri::command]
async fn library_plan_release(
    state: State<'_, AppState>,
    folder_id: i64,
    name: String,
    root_id: i64,
    options: Option<plan::ImportOptions>,
) -> Result<plan::PlanDto, String> {
    let options = options.unwrap_or_default();
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        plan::plan_release(&service, folder_id, &name, root_id, options)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// A track's waveform overview: 800 slices of low, mid and high, 0 to 255; computed once per
/// file version.
#[tauri::command]
async fn library_waveform(
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<Option<Vec<u8>>, String> {
    let service = state.library()?;
    let cache = Arc::clone(&state.waves);
    tauri::async_runtime::spawn_blocking(move || waves::overview(&service, &cache, track_id))
        .await
        .map_err(|error| error.to_string())?
}

/// The close-up of a track's waveform for the scrolling view, as raw bytes (see `waves::detail`).
#[tauri::command]
async fn library_waveform_detail(
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<tauri::ipc::Response, String> {
    let service = state.library()?;
    let cache = Arc::clone(&state.waves);
    tauri::async_runtime::spawn_blocking(move || waves::detail(&service, &cache, track_id))
        .await
        .map_err(|error| error.to_string())?
        .map(tauri::ipc::Response::new)
}

/// Load a track and play it (or keep it paused) from `start_ms`.
#[tauri::command]
async fn player_load(
    state: State<'_, AppState>,
    track_id: i64,
    start_ms: Option<u64>,
    play: Option<bool>,
) -> Result<(), String> {
    let service = state.library()?;
    let player = Arc::clone(&state.player);
    tauri::async_runtime::spawn_blocking(move || {
        player::load(
            &service,
            &player,
            track_id,
            start_ms.unwrap_or(0),
            play.unwrap_or(true),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Play, pause, seek or stop the loaded track, then say where it is.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its State and arguments by value"
)]
fn player_control(
    state: State<'_, AppState>,
    action: String,
    position_ms: Option<u64>,
    volume: Option<f32>,
) -> Result<player::PlayerDto, String> {
    let player = &state.player;
    match action.as_str() {
        "play" => player.play(),
        "pause" => player.pause(),
        "stop" => player.stop(),
        "seek" => player.seek(position_ms.ok_or("Seeking needs a position.")?),
        "volume" => player.set_volume(volume.ok_or("A volume is needed.")?),
        "status" => {}
        other => return Err(format!("Unknown player action: {other}")),
    }
    Ok(player::dto(player.status()))
}

/// Folders named from untidy tags, and recordings filed twice. Reads only.
#[tauri::command]
async fn library_check(state: State<'_, AppState>) -> Result<folders::HealthDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::check_health(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Move a folder the library check names to its tidy name; rekordbox follows the files it uses.
#[tauri::command]
async fn library_tidy_folder(
    state: State<'_, AppState>,
    root_id: i64,
    folder: String,
) -> Result<library_fix::TidiedDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        library_fix::tidy_folder(
            &service,
            &rekordbox_db::Probe::installed(),
            root_id,
            &folder,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The copies of a recording filed more than once, and which to keep by the rule.
#[tauri::command]
async fn library_duplicate_copies(
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
) -> Result<library_fix::CopiesDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        library_fix::duplicate_copies(&service, &rekordbox_db::Probe::installed(), &file_ids)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Keep one copy of a duplicate: the others are archived and merged in rekordbox.
#[tauri::command]
async fn library_resolve_duplicate(
    state: State<'_, AppState>,
    keep: i64,
    drop: Vec<i64>,
) -> Result<library_fix::ResolvedDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        library_fix::resolve_duplicate(&service, &rekordbox_db::Probe::installed(), keep, &drop)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// What the chosen tracks' files hold, for the editor.
#[tauri::command]
async fn library_track_tags(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
) -> Result<Vec<editor::TrackTagsDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::track_tags(&service, &track_ids))
        .await
        .map_err(|error| error.to_string())?
}

/// Write the editor's changes into the chosen tracks' files.
#[tauri::command]
async fn library_edit_tracks(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
    changes: Vec<editor::FieldChange>,
) -> Result<editor::EditedDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        editor::edit_tracks(&service, &track_ids, &changes)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Each track's own changes, written in one pass.
#[tauri::command]
async fn library_edit_each(
    state: State<'_, AppState>,
    edits: Vec<editor::TrackEdit>,
) -> Result<editor::EditedDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::edit_each(&service, &edits))
        .await
        .map_err(|error| error.to_string())?
}

/// Every track's title and artist, for checking them against the cleanup rules.
#[tauri::command]
async fn library_track_names(
    state: State<'_, AppState>,
) -> Result<Vec<editor::TrackNameDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::track_names(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Genres with their track counts, and the ones spelled alike.
#[tauri::command]
async fn library_genres(state: State<'_, AppState>) -> Result<editor::GenresDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::genres(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Give every track with one of the genres `from` the genre `to`.
#[tauri::command]
async fn library_rename_genres(
    state: State<'_, AppState>,
    from: Vec<String>,
    to: String,
) -> Result<editor::EditedDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::rename_genres(&service, &from, &to))
        .await
        .map_err(|error| error.to_string())?
}

/// Releases whose files carry no picture.
#[tauri::command]
async fn library_releases_without_art(
    state: State<'_, AppState>,
) -> Result<Vec<art_fix::MissingArtDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || art_fix::releases_without_art(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Give a release a cover: "folder" (the image beside its files), "online" (the Cover Art
/// Archive), or "choose" (the system picker; the page never supplies a path). `None` means
/// the picker was cancelled.
#[tauri::command]
async fn library_set_release_art(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    release_id: i64,
    source: String,
) -> Result<Option<art_fix::ArtChangedDto>, String> {
    let service = state.library()?;
    let (art, lookups) = (Arc::clone(&state.art), Arc::clone(&state.lookups));
    tauri::async_runtime::spawn_blocking(move || {
        let source = match source.as_str() {
            "folder" => art_fix::ArtSource::Folder,
            "online" => {
                let (artist, title) = service
                    .releases_all()
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .find(|(id, _, _)| *id == release_id)
                    .map(|(_, artist, title)| (artist, title))
                    .ok_or_else(|| "That release is no longer in the library.".to_owned())?;
                let files = service
                    .release_files(release_id, usize::MAX)
                    .map_or(0, |found| found.len());
                art_fix::ArtSource::Bytes(art_fix::find_online(&lookups, &artist, &title, files)?)
            }
            "choose" => {
                let Some(picked) = app
                    .dialog()
                    .file()
                    .set_title("Choose a cover image")
                    .add_filter("Images", &["jpg", "jpeg", "png", "webp", "gif"])
                    .blocking_pick_file()
                else {
                    return Ok(None);
                };
                let path = picked.into_path().map_err(|error| error.to_string())?;
                art_fix::ArtSource::File(path)
            }
            other => return Err(format!("{other} is not a place a cover can come from.")),
        };
        art_fix::set_release_art(&service, &art, release_id, &source).map(Some)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Strip every picture from a release's files.
#[tauri::command]
async fn library_remove_release_art(
    state: State<'_, AppState>,
    release_id: i64,
) -> Result<art_fix::ArtChangedDto, String> {
    let service = state.library()?;
    let art = Arc::clone(&state.art);
    tauri::async_runtime::spawn_blocking(move || {
        art_fix::remove_release_art(&service, &art, release_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Export the tracks a query lists to an M3U8 or CSV file the owner picks with the system dialog.
/// `name` is the suggested file name. `None` means the dialog was cancelled.
#[tauri::command]
async fn library_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    query: ipc::QueryDto,
    format: export::ExportFormat,
    name: String,
) -> Result<Option<export::ExportedDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_,.()".contains(c) {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        let Some(picked) = app
            .dialog()
            .file()
            .set_title("Export to")
            .set_file_name(format!("{}.{}", safe.trim(), format.extension()))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let target = picked.into_path().map_err(|error| error.to_string())?;
        export::export_query(&service, &query.to_query(), format, &target).map(Some)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Every save of the track editor, newest first.
#[tauri::command]
async fn library_edit_history(
    state: State<'_, AppState>,
) -> Result<Vec<editor::EditBatchDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::edit_history(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Put back what the files held before the newest save of the track editor.
#[tauri::command]
async fn library_undo_last_edit(state: State<'_, AppState>) -> Result<editor::EditedDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::undo_last(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Add every track of the crates `from` to `into`, each once, removing `from` when asked.
#[tauri::command]
async fn library_merge_crates(
    state: State<'_, AppState>,
    into: i64,
    from: Vec<i64>,
    remove: bool,
) -> Result<usize, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        service
            .merge_crates(into, from, remove)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Put a crate in one of the library's orders, for good.
#[tauri::command]
async fn library_sort_crate(
    state: State<'_, AppState>,
    crate_id: i64,
    sort: ipc::SortDto,
) -> Result<Vec<i64>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        service
            .sort_crate(crate_id, sort.to_sort())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// What two crates share and what only one holds, each track named.
#[tauri::command]
async fn library_compare_crates(
    state: State<'_, AppState>,
    first: i64,
    second: i64,
) -> Result<crates::ComparisonDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || crates::compare(&service, first, second))
        .await
        .map_err(|error| error.to_string())?
}

/// The owner's custom fields on tracks, and every name in use.
#[tauri::command]
async fn library_custom_fields(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
) -> Result<editor::CustomFieldsDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || editor::custom_fields(&service, &track_ids))
        .await
        .map_err(|error| error.to_string())?
}

/// Set one custom field on tracks; an empty value removes it.
#[tauri::command]
async fn library_set_custom_field(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
    name: String,
    value: String,
) -> Result<(), String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        service
            .set_custom_field(&track_ids, &name, &value)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Which lookup keys are saved, by their last four characters only.
#[tauri::command]
async fn lookup_keys(state: State<'_, AppState>) -> Result<lookup::KeysDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || lookup::keys_status(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Save or clear the owner's key for a lookup service ("discogs" or "acoustid").
#[tauri::command]
async fn lookup_set_key(
    state: State<'_, AppState>,
    service_name: String,
    value: Option<String>,
) -> Result<lookup::KeysDto, String> {
    let service = state.library()?;
    let lookups = Arc::clone(&state.lookups);
    tauri::async_runtime::spawn_blocking(move || {
        lookup::set_key(&service, &lookups, &service_name, value)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// How imported music is arranged, the presets, and whether converting is possible.
#[tauri::command]
async fn library_folder_layout(state: State<'_, AppState>) -> Result<layout::LayoutDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || layout::get(&service))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_set_folder_layout(
    state: State<'_, AppState>,
    layout: String,
) -> Result<layout::LayoutDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || layout::set(&service, &layout))
        .await
        .map_err(|error| error.to_string())?
}

/// Look a release up (MusicBrainz, and Discogs with a token) and suggest tag values for its files. Only on request;
/// nothing is written.
#[tauri::command]
async fn library_lookup_release(
    state: State<'_, AppState>,
    folder_id: i64,
    name: String,
    root_id: i64,
    options: Option<plan::ImportOptions>,
) -> Result<lookup::LookupDto, String> {
    let service = state.library()?;
    let client = Arc::clone(&state.lookups);
    let options = options.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        lookup::lookup_release(&service, &client, folder_id, &name, root_id, options)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Copy the ready files of one release into a music folder. The originals stay where they are.
#[tauri::command]
async fn library_import_release(
    state: State<'_, AppState>,
    folder_id: i64,
    name: String,
    root_id: i64,
    options: Option<plan::ImportOptions>,
) -> Result<import::ImportDto, String> {
    let options = options.unwrap_or_default();
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        import::import_release(&service, folder_id, &name, root_id, options)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_set_watch_folder_enabled(
    state: State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        folders::set_watch_folder_enabled(&service, id, enabled)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Count everything in a download folder now as already there. Reads names only.
#[tauri::command]
async fn library_retake_watch_baseline(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::retake_watch_baseline(&service, id))
        .await
        .map_err(|error| error.to_string())?
}

/// Remove a download folder from the list. Nothing on disk is touched.
#[tauri::command]
async fn library_remove_watch_folder(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::remove_watch_folder(&service, id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_mirrors(state: State<'_, AppState>) -> Result<Vec<sync::MirrorDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || sync::mirrors(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// Look at every folder (read-only) so an unplugged drive is noticed without a scan.
#[tauri::command]
async fn library_check_roots(state: State<'_, AppState>) -> Result<Vec<folders::RootDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::check_roots(&service))
        .await
        .map_err(|error| error.to_string())?
}

/// The owner confirmed that the folder now at a registered path is the library.
#[tauri::command]
async fn library_confirm_root(state: State<'_, AppState>, root_id: i64) -> Result<(), String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || folders::confirm_root(&service, root_id))
        .await
        .map_err(|error| error.to_string())?
}

/// Ask for a rekordbox XML export with the system dialog and mirror it. `None` means the
/// dialog was cancelled. The page never supplies the file path.
/// Up to four of an artist's releases that have a cover, for the artist's collage.
#[tauri::command]
async fn library_artist_covers(
    state: State<'_, AppState>,
    artist_id: i64,
) -> Result<Vec<i64>, String> {
    let service = state.library()?;
    let art = Arc::clone(&state.art);
    tauri::async_runtime::spawn_blocking(move || artwork::artist_covers(&service, &art, artist_id))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn library_crates(state: State<'_, AppState>) -> Result<Vec<crates::CrateDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || crates::list(&service))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_create_crate(
    state: State<'_, AppState>,
    name: String,
) -> Result<crates::CrateDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || crates::create(&service, &name))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_rename_crate(
    state: State<'_, AppState>,
    crate_id: i64,
    name: String,
) -> Result<(), String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        service
            .rename_crate(crate_id, &name)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Delete a crate. Its tracks stay in the library.
#[tauri::command]
async fn library_delete_crate(state: State<'_, AppState>, crate_id: i64) -> Result<(), String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        service
            .delete_crate(crate_id)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_add_to_crate(
    state: State<'_, AppState>,
    crate_id: i64,
    track_ids: Vec<i64>,
) -> Result<usize, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || crates::add(&service, crate_id, track_ids))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_remove_from_crate(
    state: State<'_, AppState>,
    crate_id: i64,
    track_ids: Vec<i64>,
) -> Result<usize, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || crates::remove(&service, crate_id, track_ids))
        .await
        .map_err(|error| error.to_string())?
}

/// Arrange the whole track list by hand: move tracks to just before `before_id` (or the end),
/// starting from the order `from_sort` shows.
#[tauri::command]
async fn library_move_tracks(
    state: State<'_, AppState>,
    track_ids: Vec<i64>,
    before_id: Option<i64>,
    from_sort: ipc::SortDto,
) -> Result<(), String> {
    let service = state.library()?;
    if track_ids.len() > crates::MAX_IDS {
        return Err(format!("At most {} tracks at a time.", crates::MAX_IDS));
    }
    tauri::async_runtime::spawn_blocking(move || {
        service
            .move_in_library(track_ids, before_id, from_sort.to_sort())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Move tracks within a crate to just before `before_id`, or to the end.
#[tauri::command]
async fn library_move_in_crate(
    state: State<'_, AppState>,
    crate_id: i64,
    track_ids: Vec<i64>,
    before_id: Option<i64>,
) -> Result<Vec<i64>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        crates::move_tracks(&service, crate_id, track_ids, before_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The cues and beat grid rekordbox has for one track. Read only.
#[tauri::command]
async fn library_track_analysis(
    state: State<'_, AppState>,
    track_id: i64,
) -> Result<Option<sync::AnalysisDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || sync::track_analysis(&service, track_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_playlists(state: State<'_, AppState>) -> Result<Vec<sync::PlaylistDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || sync::playlists(&service))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_import_xml(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<sync::ImportDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .set_title("Choose your rekordbox export")
            .add_filter("rekordbox export (XML)", &["xml"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|error| error.to_string())?;
        sync::import(&service, &path).map(Some)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_db_status(
    state: State<'_, AppState>,
) -> Result<rekordbox_db::RekordboxDbDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_db::status(&service, &rekordbox_db::Probe::installed())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_db_enable(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<rekordbox_db::RekordboxDbDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_db::set_enabled(&service, &rekordbox_db::Probe::installed(), enabled)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_db_enable_write(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<rekordbox_db::RekordboxDbDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_write::set_enabled(&service, &rekordbox_db::Probe::installed(), enabled)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_add_to_playlist(
    state: State<'_, AppState>,
    playlist_id: i64,
    track_ids: Vec<i64>,
) -> Result<rekordbox_write::WriteDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_write::add_to_playlist(
            &service,
            &rekordbox_db::Probe::installed(),
            playlist_id,
            &track_ids,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_crate_to_playlist(
    state: State<'_, AppState>,
    crate_id: i64,
    folder_id: Option<i64>,
) -> Result<rekordbox_write::WriteDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_write::crate_to_playlist(
            &service,
            &rekordbox_db::Probe::installed(),
            crate_id,
            folder_id,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_create_folder(
    state: State<'_, AppState>,
    name: String,
    folder_id: Option<i64>,
) -> Result<rekordbox_write::FolderDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_write::create_folder(
            &service,
            &rekordbox_db::Probe::installed(),
            &name,
            folder_id,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_send_new(
    state: State<'_, AppState>,
    file_ids: Option<Vec<i64>>,
) -> Result<rekordbox_write::WriteDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_write::send_new(
            &service,
            &rekordbox_db::Probe::installed(),
            file_ids.as_deref(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_merge_review(
    state: State<'_, AppState>,
) -> Result<rekordbox_merge::ReviewDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_merge::review(&service, &rekordbox_db::Probe::installed())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_merge_apply(
    state: State<'_, AppState>,
    choices: Vec<rekordbox_merge::ChoiceDto>,
) -> Result<rekordbox_merge::AppliedDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_merge::apply(&service, &rekordbox_db::Probe::installed(), &choices)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_db_read(state: State<'_, AppState>) -> Result<sync::ImportDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_db::read(&service, &rekordbox_db::Probe::installed())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_backups(
    state: State<'_, AppState>,
) -> Result<Vec<rekordbox_db::BackupDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || Ok(rekordbox_db::backups(&service)))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_backup_pin(
    state: State<'_, AppState>,
    name: String,
    pinned: bool,
) -> Result<Vec<rekordbox_db::BackupDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || rekordbox_db::pin(&service, &name, pinned))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_restore(
    state: State<'_, AppState>,
    name: String,
) -> Result<rekordbox_db::BackupDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_db::restore(&service, &rekordbox_db::Probe::installed(), &name)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_new_tracks(
    state: State<'_, AppState>,
) -> Result<rekordbox_send::NewTracksDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || rekordbox_send::new_tracks(&service))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_send(
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
) -> Result<rekordbox_send::SentDto, String> {
    let service = state.library()?;
    let now = oc_core_db::now_ms();
    tauri::async_runtime::spawn_blocking(move || rekordbox_send::send(&service, &file_ids, now))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_crates(state: State<'_, AppState>) -> Result<rekordbox_send::CratesDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || rekordbox_send::crates(&service))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_set_crate(
    state: State<'_, AppState>,
    crate_id: i64,
    in_file: bool,
) -> Result<rekordbox_send::CratesDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_send::set_crate(&service, crate_id, in_file)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Write openCrate's rekordbox file again, so crates changed since show as they are now.
#[tauri::command]
async fn rekordbox_update_file(
    state: State<'_, AppState>,
) -> Result<rekordbox_send::CratesDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || rekordbox_send::update_file(&service))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn rekordbox_hide(
    state: State<'_, AppState>,
    file_ids: Vec<i64>,
    hidden: bool,
) -> Result<rekordbox_send::NewTracksDto, String> {
    let service = state.library()?;
    let now = oc_core_db::now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        rekordbox_send::set_hidden(&service, &file_ids, hidden, now)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_reread_mirror(
    state: State<'_, AppState>,
    source_id: i64,
) -> Result<sync::ImportDto, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || sync::reread(&service, source_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_alias_suggestions(
    state: State<'_, AppState>,
    source_id: i64,
) -> Result<Vec<sync::SuggestionDto>, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || sync::suggestions(&service, source_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_add_alias(
    state: State<'_, AppState>,
    root_id: i64,
    prefix: String,
) -> Result<bool, String> {
    let service = state.library()?;
    tauri::async_runtime::spawn_blocking(move || sync::add_alias(&service, root_id, &prefix))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn library_scan(
    state: State<'_, AppState>,
    root_id: i64,
    confirm_missing: Option<bool>,
) -> Result<(), String> {
    let service = state.library()?;
    let scan = Arc::clone(&state.scan);
    let confirm = confirm_missing.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || scan.start(&service, root_id, confirm).map(drop))
        .await
        .map_err(|error| error.to_string())?
}

/// Remove a folder from the library. Index entries only; no music file is touched.
#[tauri::command]
async fn library_remove_root(state: State<'_, AppState>, root_id: i64) -> Result<usize, String> {
    let service = state.library()?;
    let scan = Arc::clone(&state.scan);
    tauri::async_runtime::spawn_blocking(move || folders::remove_root(&service, &scan, root_id))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its State by value"
)]
fn library_scan_status(state: State<'_, AppState>) -> folders::ScanStatusDto {
    state.scan.status()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its State by value"
)]
fn library_scan_cancel(state: State<'_, AppState>) {
    state.scan.cancel();
}

fn main() -> Result<(), tauri::Error> {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // A menu on Windows would add a menu strip to the window; there Ctrl+, opens Settings.
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::build).on_menu_event(menu::on_event);
    builder
        .setup(|app| {
            // The local (not roaming) folder: the library must never sit in a synced place.
            let default = app.path().app_local_data_dir().ok();
            app.manage(AppState::from_environment(default.as_deref()));
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("artwork", |context, request, responder| {
            let state = context.app_handle().state::<AppState>();
            let (service, art) = (state.service.clone(), Arc::clone(&state.art));
            let path = request.uri().path().to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(artwork::respond(service.as_deref(), &art, &path));
            });
        })
        .invoke_handler(tauri::generate_handler![
            library_mode,
            library_read,
            library_roots,
            library_check_roots,
            library_confirm_root,
            library_add_root,
            library_watch_folders,
            library_check_watch_folders,
            library_add_watch_folder,
            library_download_releases,
            library_plan_release,
            library_import_release,
            library_lookup_release,
            library_folder_layout,
            library_check,
            player_load,
            library_waveform,
            library_waveform_detail,
            player_control,
            library_set_folder_layout,
            library_set_watch_folder_enabled,
            library_remove_watch_folder,
            library_retake_watch_baseline,
            library_scan,
            library_remove_root,
            library_scan_status,
            library_scan_cancel,
            library_mirrors,
            library_playlists,
            library_track_analysis,
            library_artist_covers,
            library_crates,
            library_create_crate,
            library_rename_crate,
            library_delete_crate,
            library_add_to_crate,
            library_remove_from_crate,
            library_move_in_crate,
            library_move_tracks,
            library_import_xml,
            library_reread_mirror,
            library_alias_suggestions,
            library_add_alias,
            rekordbox_db_status,
            rekordbox_db_enable,
            rekordbox_db_read,
            rekordbox_db_enable_write,
            rekordbox_add_to_playlist,
            rekordbox_crate_to_playlist,
            rekordbox_create_folder,
            rekordbox_send_new,
            rekordbox_merge_review,
            rekordbox_merge_apply,
            library_tidy_folder,
            lookup_keys,
            library_track_tags,
            library_edit_tracks,
            library_edit_each,
            library_custom_fields,
            library_set_custom_field,
            library_merge_crates,
            library_sort_crate,
            library_compare_crates,
            library_edit_history,
            library_undo_last_edit,
            library_export,
            library_releases_without_art,
            library_set_release_art,
            library_remove_release_art,
            library_track_names,
            library_genres,
            library_rename_genres,
            library_duplicate_copies,
            library_resolve_duplicate,
            lookup_set_key,
            rekordbox_backups,
            rekordbox_backup_pin,
            rekordbox_restore,
            rekordbox_new_tracks,
            rekordbox_send,
            rekordbox_crates,
            rekordbox_set_crate,
            rekordbox_update_file,
            rekordbox_hide,
        ])
        .build(tauri::generate_context!())?
        .run(|app, event| {
            // Tauri exits the process without dropping managed state, so remove the demo
            // database here. Where open files cannot be deleted, the next start sweeps it.
            if let RunEvent::Exit = event {
                if let Some(directory) = &app.state::<AppState>().demo_directory {
                    let _ = std::fs::remove_dir_all(directory.path());
                }
            }
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
    use tauri::webview::InvokeRequest;
    use tauri::{WebviewUrl, WebviewWindow, WebviewWindowBuilder};

    use super::*;

    /// A demo database inside a folder the test removes, so no run leaves files behind.
    fn demo(tracks: &str) -> (tempfile::TempDir, AppState) {
        let parent = tempfile::tempdir().unwrap();
        let state = AppState::open_demo(tracks, parent.path()).unwrap();
        (parent, state)
    }

    fn app(state: AppState) -> WebviewWindow<tauri::test::MockRuntime> {
        let app = mock_builder()
            .manage(state)
            .invoke_handler(tauri::generate_handler![
                library_mode,
                library_read,
                library_roots,
                library_check_roots,
                library_confirm_root,
                library_watch_folders,
                library_check_watch_folders,
                library_download_releases,
                library_plan_release,
                library_import_release,
                library_lookup_release,
                library_folder_layout,
                library_check,
                player_load,
                library_waveform,
                library_waveform_detail,
                player_control,
                library_set_folder_layout,
                library_set_watch_folder_enabled,
                library_remove_watch_folder,
                library_retake_watch_baseline,
                library_scan,
                library_remove_root,
                library_scan_status,
                library_scan_cancel,
                library_mirrors,
                library_playlists,
                library_track_analysis,
                library_crates,
                library_create_crate,
                library_rename_crate,
                library_delete_crate,
                library_add_to_crate,
                library_remove_from_crate,
                library_move_in_crate,
                library_move_tracks,
                library_reread_mirror,
                library_alias_suggestions,
                library_add_alias,
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
            .build()
            .unwrap()
    }

    fn call(
        window: &WebviewWindow<tauri::test::MockRuntime>,
        command: &str,
        body: Value,
    ) -> Result<Value, Value> {
        call_from(window, command, body, local_origin())
    }

    fn local_origin() -> &'static str {
        if cfg!(windows) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
    }

    fn call_from(
        window: &WebviewWindow<tauri::test::MockRuntime>,
        command: &str,
        body: Value,
        origin: &str,
    ) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: origin.parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(window, request).map(|response| match response {
            InvokeResponseBody::Json(text) => serde_json::from_str(&text).unwrap(),
            InvokeResponseBody::Raw(bytes) => json!(bytes.len()),
        })
    }

    fn query() -> Value {
        json!({"view": "tracks", "search": "", "sort": "title", "format": null,
               "artistId": null, "releaseId": null, "playlistId": null})
    }

    #[test]
    fn the_demo_setting_is_validated() {
        let parent = tempfile::tempdir().unwrap();
        for bad in ["", "0", "-5", "abc", "100001", "1.5"] {
            assert!(AppState::open_demo(bad, parent.path()).is_err(), "{bad:?}");
        }
        let (_parent, state) = demo("300");
        assert!(state.service.is_some());
    }

    #[test]
    fn the_commands_work_through_tauri_with_the_names_the_ui_uses() {
        let window = app(demo("300").1);
        assert_eq!(
            call(&window, "library_mode", json!({})).unwrap(),
            json!({"mode": "database", "canManageFolders": true, "persistent": false, "error": null})
        );

        let page = call(
            &window,
            "library_read",
            json!({"query": query(), "start": 10, "count": 5}),
        )
        .unwrap();
        assert_eq!(
            (page["total"].as_u64(), page["start"].as_u64()),
            (Some(300), Some(10))
        );
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), 5);
        assert_eq!(items[0]["kind"], "tracks");
        assert!(items[0]["musicalKey"].is_string() && items[0]["durationMs"].is_i64());

        let mut filtered = query();
        filtered["format"] = json!("AIFF");
        filtered["search"] = json!("night");
        filtered["artistId"] = json!(null);
        let found = call(
            &window,
            "library_read",
            json!({"query": filtered, "start": 0, "count": 200}),
        )
        .unwrap();
        assert!(found["total"].as_u64().unwrap() > 0);
        assert!(found["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["format"] == "AIFF"));
    }

    #[test]
    fn bad_requests_and_a_missing_database_come_back_as_errors() {
        let window = app(demo("50").1);
        let read = |body: Value| call(&window, "library_read", body);
        assert!(read(json!({"query": query(), "start": 0, "count": 0})).is_err());
        assert!(read(json!({"query": query(), "start": 0, "count": 201})).is_err());
        assert!(read(json!({"query": query(), "start": -1, "count": 10})).is_err());
        let mut extra = query();
        extra["path"] = json!("/etc");
        assert!(read(json!({"query": extra, "start": 0, "count": 10})).is_err());
        assert!(read(json!({"start": 0, "count": 10})).is_err());

        let none = app(AppState::none());
        assert_eq!(
            call(&none, "library_mode", json!({})).unwrap(),
            json!({"mode": "synthetic", "canManageFolders": false, "persistent": false, "error": null})
        );
        let error = call(
            &none,
            "library_read",
            json!({"query": query(), "start": 0, "count": 10}),
        )
        .unwrap_err();
        assert!(error.to_string().contains("No database library"), "{error}");
    }

    #[test]
    fn pages_from_other_origins_cannot_reach_the_commands() {
        let window = app(demo("50").1);
        let body = json!({"query": query(), "start": 0, "count": 5});
        assert!(call(&window, "library_read", body.clone()).is_ok());
        for origin in ["https://example.com", "http://localhost:9999"] {
            assert!(
                call_from(&window, "library_read", body.clone(), origin).is_err(),
                "{origin}"
            );
            assert!(
                call_from(&window, "library_mode", json!({}), origin).is_err(),
                "{origin}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn only_old_demo_folders_of_ours_are_swept() {
        let parent = tempfile::tempdir().unwrap();
        let make = |name: &str, age_hours: u64| {
            let path = parent.path().join(name);
            std::fs::create_dir(&path).unwrap();
            let old = SystemTime::now() - Duration::from_secs(age_hours * 3600);
            std::fs::File::open(&path)
                .unwrap()
                .set_modified(old)
                .unwrap();
            path
        };
        let stale = make("opencrate-demo-old", 48);
        let fresh = make("opencrate-demo-new", 1);
        let other = make("someone-elses-old-folder", 48);
        let file = parent.path().join("opencrate-demo-file");
        std::fs::write(&file, b"x").unwrap();
        remove_stale_demos(parent.path());
        assert!(!stale.exists());
        assert!(fresh.exists() && other.exists() && file.exists());
        remove_stale_demos(&parent.path().join("missing")); // no folder, no error
    }

    fn audio_folder() -> tempfile::TempDir {
        use oc_tag::fixtures::{aiff, wav, write_tags};
        use oc_tag::TagInfo;
        let folder = tempfile::tempdir().unwrap();
        for (relative, bytes, title) in [
            (
                "Studio/Between Spaces/01 - Night Drive.aif",
                aiff(2),
                "Night Drive",
            ),
            (
                "Studio/Between Spaces/02 - Soft Focus.wav",
                wav(1),
                "Soft Focus",
            ),
        ] {
            let path = folder.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            write_tags(
                &path,
                &TagInfo {
                    title: Some(title.into()),
                    artist: Some("Studio".into()),
                    album: Some("Between Spaces".into()),
                    ..TagInfo::default()
                },
            );
        }
        folder
    }

    fn wait_for_scan(window: &WebviewWindow<tauri::test::MockRuntime>) -> Value {
        for _ in 0..500 {
            let status = call(window, "library_scan_status", json!({})).unwrap();
            if status["state"] != "running" {
                return status;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the scan did not finish");
    }

    #[test]
    fn folders_are_listed_scanned_and_read_through_the_commands() {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::open_persistent(data.path()).unwrap();
        let folder = audio_folder();
        // The native dialog is the only way a page can add a folder; here we stand in for it.
        let added = folders::add_root(&state.library().unwrap(), folder.path()).unwrap();
        let window = app(state);

        let mode = call(&window, "library_mode", json!({})).unwrap();
        assert_eq!(
            (mode["mode"].as_str(), mode["persistent"].as_bool()),
            (Some("database"), Some(true))
        );
        let listed = call(&window, "library_roots", json!({})).unwrap();
        assert_eq!(listed[0]["id"], json!(added.id));
        assert_eq!(
            (
                listed[0]["files"].as_i64(),
                listed[0]["availability"].as_str()
            ),
            (Some(0), Some("available"))
        );
        assert_eq!(
            call(&window, "library_scan_status", json!({})).unwrap()["state"],
            "idle"
        );

        call(&window, "library_scan", json!({"rootId": added.id})).unwrap();
        let status = wait_for_scan(&window);
        assert_eq!(status["state"], "finished", "{status}");
        assert_eq!(status["summary"]["tracksCreated"], 2);
        assert_eq!(status["summary"]["tagErrors"], 0);
        assert_eq!(status["rootId"], json!(added.id));

        let listed = call(&window, "library_roots", json!({})).unwrap();
        assert_eq!(listed[0]["files"], 2);
        let page = call(
            &window,
            "library_read",
            json!({"query": query(), "start": 0, "count": 10}),
        )
        .unwrap();
        assert_eq!(page["total"], 2);
        assert_eq!(page["items"][0]["title"], "Night Drive");
        assert_eq!(page["items"][0]["format"], "AIFF");

        assert!(call(&window, "library_scan", json!({"rootId": 999})).is_err());
        assert!(call(&window, "library_scan", json!({"rootId": "1"})).is_err());
        assert_eq!(
            call(&window, "library_scan_cancel", json!({})).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn a_persistent_library_is_still_there_after_a_restart() {
        let data = tempfile::tempdir().unwrap();
        let folder = audio_folder();
        {
            let state = AppState::open_persistent(data.path()).unwrap();
            let service = state.library().unwrap();
            let id = folders::add_root(&service, folder.path()).unwrap().id;
            state
                .scan
                .start(&service, id, false)
                .unwrap()
                .join()
                .unwrap();
        }
        let again = AppState::open_persistent(data.path()).unwrap();
        let service = again.library().unwrap();
        let roots = folders::roots(&service).unwrap();
        assert_eq!((roots.len(), roots[0].files), (1, 2));
        let page = service
            .read_page(
                &oc_core_db::library::LibraryQuery::new(oc_core_db::library::View::Tracks),
                0,
                10,
            )
            .unwrap();
        assert_eq!(page.total, 2);
        assert!(AppState::open_persistent(Path::new("relative/dir")).is_err());
        assert!(AppState::open_persistent(Path::new("")).is_err());
    }

    #[test]
    fn the_folder_commands_are_refused_to_other_origins() {
        let data = tempfile::tempdir().unwrap();
        let window = app(AppState::open_persistent(data.path()).unwrap());
        for command in [
            "library_roots",
            "library_scan_status",
            "library_scan_cancel",
        ] {
            assert!(call(&window, command, json!({})).is_ok(), "{command}");
            assert!(
                call_from(&window, command, json!({}), "https://example.com").is_err(),
                "{command}"
            );
        }
        assert!(call_from(
            &window,
            "library_scan",
            json!({"rootId": 1}),
            "https://example.com"
        )
        .is_err());
    }

    /// The cues and grid in the sync fixture's export, read through the command.
    fn check_track_analysis(window: &WebviewWindow<tauri::test::MockRuntime>, night_id: i64) {
        let analysis = call(
            window,
            "library_track_analysis",
            json!({"trackId": night_id}),
        )
        .unwrap();
        assert_eq!(analysis["generation"], 2);
        assert_eq!(analysis["bpm"], 126.0);
        assert_eq!(analysis["musicalKey"], "6A");
        assert_eq!(analysis["cues"][0]["name"], "Drop");
        assert_eq!(analysis["cues"][0]["startSeconds"], 12.5);
        assert_eq!(analysis["tempos"][0]["meter"], "4/4");
        assert_eq!(
            call(window, "library_track_analysis", json!({"trackId": 99_999})).unwrap(),
            json!(null)
        );
        assert!(call(window, "library_track_analysis", json!({"trackId": "1"})).is_err());
        assert!(call_from(
            window,
            "library_track_analysis",
            json!({"trackId": night_id}),
            "https://example.com"
        )
        .is_err());
    }

    #[test]
    fn rekordbox_sync_works_through_the_commands_and_refuses_bad_input() {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::open_persistent(data.path()).unwrap();
        let service = state.library().unwrap();
        let folder = audio_folder();
        let root = folders::add_root(&service, folder.path()).unwrap().id;
        state
            .scan
            .start(&service, root, false)
            .unwrap()
            .join()
            .unwrap();
        let exports = tempfile::tempdir().unwrap();
        let export = exports.path().join("collection.xml");
        std::fs::write(
            &export,
            "<DJ_PLAYLISTS><COLLECTION Entries=\"2\">\
             <TRACK TrackID=\"1\" AverageBpm=\"126.00\" Tonality=\"6A\" \
              Location=\"file://localhost/G:/Library/Studio/Between%20Spaces/01%20-%20Night%20Drive.aif\">\
              <TEMPO Inizio=\"0.025\" Bpm=\"126.00\" Metro=\"4/4\" Battito=\"1\"/>\
              <POSITION_MARK Name=\"Drop\" Type=\"0\" Start=\"12.5\" Num=\"0\"/></TRACK>\
             <TRACK TrackID=\"2\" Location=\"file://localhost/G:/Elsewhere/x.aif\"/>\
             </COLLECTION></DJ_PLAYLISTS>",
        )
        .unwrap();
        // The native dialog is the only way a page can pick the file; here we stand in for it.
        let imported = sync::import(&service, &export).unwrap();
        assert_eq!((imported.entries, imported.matched), (2, 0));
        let window = app(state);

        let listed = call(&window, "library_mirrors", json!({})).unwrap();
        assert_eq!(listed[0]["sourceId"], json!(imported.source_id));
        assert_eq!(
            (
                listed[0]["generation"].as_i64(),
                listed[0]["counts"]["unmapped"].as_i64()
            ),
            (Some(1), Some(2))
        );
        assert!(listed[0]["lastError"].is_null());

        let suggestions = call(
            &window,
            "library_aliasSuggestions",
            json!({"sourceId": imported.source_id}),
        );
        assert!(suggestions.is_err(), "commands keep their snake_case names");
        let suggestions = call(
            &window,
            "library_alias_suggestions",
            json!({"sourceId": imported.source_id}),
        )
        .unwrap();
        assert_eq!(suggestions[0]["prefix"], "/G:/Library");
        assert_eq!(
            (
                suggestions[0]["rootId"].as_i64(),
                suggestions[0]["entries"].as_i64()
            ),
            (Some(root), Some(1))
        );

        let added = call(
            &window,
            "library_add_alias",
            json!({"rootId": root, "prefix": "/G:/Library"}),
        )
        .unwrap();
        assert_eq!(added, json!(true));
        let again = call(
            &window,
            "library_add_alias",
            json!({"rootId": root, "prefix": "/G:/Library"}),
        )
        .unwrap();
        assert_eq!(again, json!(false));
        let reread = call(
            &window,
            "library_reread_mirror",
            json!({"sourceId": imported.source_id}),
        )
        .unwrap();
        assert_eq!(
            (
                reread["matched"].as_i64(),
                reread["generation"].as_i64(),
                reread["derivedSet"].as_i64()
            ),
            (Some(1), Some(2), Some(1))
        );
        let page = call(
            &window,
            "library_read",
            json!({"query": query(), "start": 0, "count": 10}),
        )
        .unwrap();
        let night = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["title"] == "Night Drive")
            .unwrap();
        assert_eq!(
            (night["bpm"].as_f64(), night["musicalKey"].as_str()),
            (Some(126.0), Some("6A"))
        );

        let night_id = night["id"].as_i64().unwrap();
        check_track_analysis(&window, night_id);

        for bad in ["", "relative", "/G:/a/../b"] {
            assert!(
                call(
                    &window,
                    "library_add_alias",
                    json!({"rootId": root, "prefix": bad})
                )
                .is_err(),
                "{bad:?}"
            );
        }
        assert!(call(
            &window,
            "library_add_alias",
            json!({"rootId": 999, "prefix": "/G:/x"})
        )
        .is_err());
        assert!(call(&window, "library_reread_mirror", json!({"sourceId": 999})).is_err());
        assert!(call(&window, "library_reread_mirror", json!({"sourceId": "1"})).is_err());
        for command in [
            "library_mirrors",
            "library_alias_suggestions",
            "library_reread_mirror",
            "library_add_alias",
        ] {
            let body = json!({"sourceId": 1, "rootId": 1, "prefix": "/G:/x"});
            assert!(
                call_from(&window, command, body, "https://example.com").is_err(),
                "{command}"
            );
        }
    }

    #[test]
    fn crates_are_made_filled_reordered_and_read_through_the_commands() {
        let (_parent, state) = demo("12");
        let window = app(state);
        let made = call(&window, "library_create_crate", json!({"name": "Warm-up"})).unwrap();
        let id = made["id"].as_i64().unwrap();
        assert_eq!(
            (made["name"].as_str(), made["tracks"].as_i64()),
            (Some("Warm-up"), Some(0))
        );
        let page = call(
            &window,
            "library_read",
            json!({"query": query(), "start": 0, "count": 12}),
        )
        .unwrap();
        let ids: Vec<i64> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_i64().unwrap())
            .collect();
        assert_eq!(
            call(
                &window,
                "library_add_to_crate",
                json!({"crateId": id, "trackIds": &ids[..4]})
            )
            .unwrap(),
            json!(4)
        );
        let order = call(
            &window,
            "library_move_in_crate",
            json!({"crateId": id, "trackIds": [ids[3]], "beforeId": ids[0]}),
        )
        .unwrap();
        assert_eq!(order, json!([ids[3], ids[0], ids[1], ids[2]]));
        let mut in_crate = query();
        in_crate["crateId"] = json!(id);
        in_crate["sort"] = json!("crate");
        let read = call(
            &window,
            "library_read",
            json!({"query": in_crate, "start": 0, "count": 10}),
        )
        .unwrap();
        let shown: Vec<i64> = read["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_i64().unwrap())
            .collect();
        assert_eq!(shown, [ids[3], ids[0], ids[1], ids[2]]);
        assert_eq!(
            call(
                &window,
                "library_remove_from_crate",
                json!({"crateId": id, "trackIds": [ids[0]]})
            )
            .unwrap(),
            json!(1)
        );
        call(
            &window,
            "library_rename_crate",
            json!({"crateId": id, "name": "Opening"}),
        )
        .unwrap();
        let listed = call(&window, "library_crates", json!({})).unwrap();
        assert_eq!(
            (listed[0]["name"].as_str(), listed[0]["tracks"].as_i64()),
            (Some("Opening"), Some(3))
        );
        assert!(call(&window, "library_create_crate", json!({"name": "  "})).is_err());
        assert!(call(
            &window,
            "library_add_to_crate",
            json!({"crateId": 999, "trackIds": [1]})
        )
        .is_err());
        assert!(call(
            &window,
            "library_add_to_crate",
            json!({"crateId": id, "trackIds": ["1"]})
        )
        .is_err());
        let too_many: Vec<i64> = (0..=10_000).collect();
        assert!(call(
            &window,
            "library_add_to_crate",
            json!({"crateId": id, "trackIds": too_many})
        )
        .is_err());
        assert!(call_from(
            &window,
            "library_delete_crate",
            json!({"crateId": id}),
            "https://example.com"
        )
        .is_err());
        call(&window, "library_delete_crate", json!({"crateId": id})).unwrap();
        assert_eq!(
            call(&window, "library_crates", json!({})).unwrap(),
            json!([])
        );
    }

    #[test]
    fn download_folders_go_through_commands_and_are_refused_when_they_overlap() {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::open_persistent(data.path()).unwrap();
        let service = state.library().unwrap();
        let area = tempfile::tempdir().unwrap();
        let music = area.path().join("Music");
        let downloads = area.path().join("Downloads");
        std::fs::create_dir_all(&music).unwrap();
        std::fs::create_dir_all(&downloads).unwrap();
        folders::add_root(&service, &music).unwrap();
        assert!(folders::add_watch_folder(&service, &music).is_err());
        let id = folders::add_watch_folder(&service, &downloads).unwrap().id;
        let window = app(state);

        let listed = call(&window, "library_watch_folders", json!({})).unwrap();
        assert_eq!(listed[0]["id"], id);
        assert_eq!(listed[0]["enabled"], true);
        assert_eq!(listed[0]["online"], true);
        std::fs::create_dir_all(downloads.join("Fresh Release")).unwrap();
        std::fs::write(downloads.join("Fresh Release/01.flac.part"), b"x").unwrap();
        let seen = call(&window, "library_download_releases", json!({})).unwrap();
        assert_eq!(seen["folders"][0]["state"], "watching");
        assert_eq!(seen["releases"][0]["name"], "Fresh Release");
        assert_eq!(seen["releases"][0]["state"], "downloading");
        assert!(call_from(
            &window,
            "library_download_releases",
            json!({}),
            "https://example.com"
        )
        .is_err());
        call(
            &window,
            "library_set_watch_folder_enabled",
            json!({"id": id, "enabled": false}),
        )
        .unwrap();
        assert!(call(
            &window,
            "library_set_watch_folder_enabled",
            json!({"id": id, "enabled": "no"}),
        )
        .is_err());
        let checked = call(&window, "library_check_watch_folders", json!({})).unwrap();
        assert_eq!(checked[0]["enabled"], false);
        assert!(call(&window, "library_remove_watch_folder", json!({"id": 999})).is_err());
        assert!(call_from(
            &window,
            "library_remove_watch_folder",
            json!({"id": id}),
            "https://example.com"
        )
        .is_err());
        call(&window, "library_remove_watch_folder", json!({"id": id})).unwrap();
        assert_eq!(
            call(&window, "library_watch_folders", json!({})).unwrap(),
            json!([])
        );
        assert!(downloads.exists());
    }

    #[test]
    fn a_folder_is_removed_through_the_command_and_a_running_scan_blocks_it() {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::open_persistent(data.path()).unwrap();
        let service = state.library().unwrap();
        let folder = audio_folder();
        let id = folders::add_root(&service, folder.path()).unwrap().id;
        state
            .scan
            .start(&service, id, false)
            .unwrap()
            .join()
            .unwrap();
        let window = app(state);

        assert_eq!(
            call(
                &window,
                "library_scan",
                json!({"rootId": id, "confirmMissing": true})
            )
            .map(|_| ()),
            Ok(())
        );
        let status = wait_for_scan(&window);
        assert_eq!(status["state"], "finished", "{status}");

        assert!(call(&window, "library_remove_root", json!({"rootId": 999})).is_err());
        assert!(call(&window, "library_remove_root", json!({"rootId": "1"})).is_err());
        assert_eq!(
            call(&window, "library_remove_root", json!({"rootId": id})).unwrap(),
            json!(2)
        );
        assert_eq!(
            call(&window, "library_roots", json!({})).unwrap(),
            json!([])
        );
        let page = call(
            &window,
            "library_read",
            json!({"query": query(), "start": 0, "count": 10}),
        )
        .unwrap();
        assert_eq!(page["total"], 0);
        assert!(folder
            .path()
            .join("Studio/Between Spaces/02 - Soft Focus.wav")
            .exists());
        assert!(call_from(
            &window,
            "library_remove_root",
            json!({"rootId": id}),
            "https://example.com"
        )
        .is_err());
    }

    #[test]
    fn removing_is_refused_while_a_scan_is_running() {
        let (_database, service) = {
            let database = tempfile::tempdir().unwrap();
            let service = Arc::new(LibraryService::open(database.path(), 1).unwrap());
            (database, service)
        };
        let control = folders::ScanControl::default();
        let folder = audio_folder();
        let id = folders::add_root(&service, folder.path()).unwrap().id;
        control.force_running_for_test(id);
        let error = folders::remove_root(&service, &control, id).unwrap_err();
        assert!(error.contains("scan is running"), "{error}");
    }

    #[test]
    fn a_setting_that_cannot_be_used_is_reported_to_the_window_not_just_stderr() {
        let temp = tempfile::tempdir().unwrap();
        let none = AppState::from_settings(None, None, temp.path(), None);
        assert!(none.service.is_none() && none.startup_error.is_none());

        let relative = AppState::from_settings(Some("relative/dir"), None, temp.path(), None);
        assert!(relative.service.is_none());
        assert!(
            relative
                .startup_error
                .as_deref()
                .is_some_and(|m| m.contains("absolute")),
            "{:?}",
            relative.startup_error
        );
        let window = app(relative);
        let mode = call(&window, "library_mode", json!({})).unwrap();
        assert_eq!(mode["mode"], "synthetic");
        assert!(mode["error"]
            .as_str()
            .is_some_and(|m| m.contains("library folder could not be opened")));

        // A library another window already holds cannot be opened twice.
        let data = tempfile::tempdir().unwrap();
        let first = AppState::open_persistent(data.path()).unwrap();
        let second = AppState::from_settings(data.path().to_str(), None, temp.path(), None);
        assert!(second.service.is_none());
        assert!(
            second
                .startup_error
                .as_deref()
                .is_some_and(|m| m.contains("already owned")),
            "{:?}",
            second.startup_error
        );
        drop(first);

        let bad_demo = AppState::from_settings(None, Some("0"), temp.path(), None);
        assert!(bad_demo
            .startup_error
            .as_deref()
            .is_some_and(|m| m.contains("demo library")));
        let data_wins =
            AppState::from_settings(data.path().to_str(), Some("500"), temp.path(), None);
        assert!(data_wins.persistent && data_wins.startup_error.is_none());
    }

    #[test]
    fn without_settings_the_library_lives_in_the_app_data_folder() {
        let temp = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let folder = home.path().join("com.opencrate.desktop");
        let state = AppState::from_settings(None, None, temp.path(), Some(&folder));
        assert!(state.persistent && state.startup_error.is_none());
        assert!(folder.is_dir(), "made on first start");
        drop(state);
        let again = AppState::from_settings(None, None, temp.path(), Some(&folder));
        assert!(again.persistent, "the same library opens next time");
        let demo = AppState::from_settings(None, Some("50"), temp.path(), Some(&folder));
        assert!(
            !demo.persistent && demo.service.is_some(),
            "a demo asked for still wins"
        );
    }

    #[test]
    fn folder_checks_work_through_the_commands_and_unknown_folders_are_refused() {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::open_persistent(data.path()).unwrap();
        let folder = audio_folder();
        let id = folders::add_root(&state.library().unwrap(), folder.path())
            .unwrap()
            .id;
        let window = app(state);

        let checked = call(&window, "library_check_roots", json!({})).unwrap();
        assert_eq!(
            (
                checked[0]["id"].as_i64(),
                checked[0]["availability"].as_str()
            ),
            (Some(id), Some("available"))
        );
        std::fs::remove_dir_all(folder.path()).unwrap();
        let gone = call(&window, "library_check_roots", json!({})).unwrap();
        assert_eq!(gone[0]["availability"], "unavailable");
        assert_eq!(
            (gone[0]["missing"].as_i64(), gone[0]["reason"].is_string()),
            (Some(0), true)
        );

        assert!(
            call(&window, "library_confirm_root", json!({"rootId": id})).is_err(),
            "a folder that is not there cannot be confirmed"
        );
        assert!(call(&window, "library_confirm_root", json!({"rootId": 999})).is_err());
        assert!(call(&window, "library_confirm_root", json!({"rootId": "1"})).is_err());
        std::fs::create_dir_all(folder.path()).unwrap();
        assert_eq!(
            call(&window, "library_check_roots", json!({})).unwrap()[0]["availability"],
            "unverified"
        );
        assert_eq!(
            call(&window, "library_confirm_root", json!({"rootId": id})).unwrap(),
            Value::Null
        );
        assert_eq!(
            call(&window, "library_check_roots", json!({})).unwrap()[0]["availability"],
            "available"
        );
        assert!(call_from(
            &window,
            "library_check_roots",
            json!({}),
            "https://example.com"
        )
        .is_err());
        assert!(call_from(
            &window,
            "library_confirm_root",
            json!({"rootId": id}),
            "https://example.com"
        )
        .is_err());
    }
}
