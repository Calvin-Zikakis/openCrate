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
