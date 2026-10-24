//! The owner's crates for the UI. Crates live in openCrate only and never reach rekordbox.

use oc_core_db::crates::CrateRow;
use oc_core_db::service::LibraryService;
use serde::Serialize;

/// The most track ids one request may carry.
pub const MAX_IDS: usize = 10_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrateDto {
    pub id: i64,
    pub name: String,
    pub tracks: i64,
}

impl From<CrateRow> for CrateDto {
    fn from(row: CrateRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            tracks: row.tracks,
        }
    }
}

fn text(error: &oc_core_db::Error) -> String {
    error.to_string()
}

pub fn list(service: &LibraryService) -> Result<Vec<CrateDto>, String> {
    service
        .crates()
        .map(|rows| rows.into_iter().map(CrateDto::from).collect())
        .map_err(|e| text(&e))
}

pub fn create(service: &LibraryService, name: &str) -> Result<CrateDto, String> {
    let id = service.create_crate(name).map_err(|e| text(&e))?;
    list(service)?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| "The crate was made but could not be listed.".to_owned())
}

fn checked(ids: Vec<i64>) -> Result<Vec<i64>, String> {
    if ids.len() > MAX_IDS {
        return Err(format!("At most {MAX_IDS} tracks at a time."));
    }
    Ok(ids)
}

pub fn add(service: &LibraryService, crate_id: i64, ids: Vec<i64>) -> Result<usize, String> {
    service
        .add_to_crate(crate_id, checked(ids)?)
        .map_err(|e| text(&e))
}

pub fn remove(service: &LibraryService, crate_id: i64, ids: Vec<i64>) -> Result<usize, String> {
    service
        .remove_from_crate(crate_id, checked(ids)?)
        .map_err(|e| text(&e))
}

pub fn move_tracks(
    service: &LibraryService,
    crate_id: i64,
    ids: Vec<i64>,
    before: Option<i64>,
) -> Result<Vec<i64>, String> {
    service
        .move_in_crate(crate_id, checked(ids)?, before)
        .map_err(|e| text(&e))
}

/// A track as the crate comparison names it.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamedTrackDto {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonDto {
    pub both: Vec<NamedTrackDto>,
    pub only_first: Vec<NamedTrackDto>,
    pub only_second: Vec<NamedTrackDto>,
}

/// What two crates share and what only one holds, each track named.
pub fn compare(service: &LibraryService, first: i64, second: i64) -> Result<ComparisonDto, String> {
    let found = service
        .compare_crates(first, second)
        .map_err(|error| error.to_string())?;
    let names: std::collections::HashMap<i64, (String, String)> = service
        .track_names()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|(id, title, artist)| (id, (title, artist)))
        .collect();
    let with_names = |ids: Vec<i64>| -> Vec<NamedTrackDto> {
        ids.into_iter()
            .map(|track_id| {
                let (title, artist) = names.get(&track_id).cloned().unwrap_or_default();
                NamedTrackDto {
                    track_id,
                    title,
                    artist,
                }
            })
            .collect()
    };
    Ok(ComparisonDto {
        both: with_names(found.both),
        only_first: with_names(found.only_first),
        only_second: with_names(found.only_second),
    })
}
