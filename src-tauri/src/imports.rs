//! What was imported, for the Recently imported screen, and where an import's tracks go
//! afterwards (a crate, a rekordbox playlist): a library setting the page reads and writes.

use oc_core_db::imports::ImportBatchRow;
use oc_core_db::service::LibraryService;
use serde::{Deserialize, Serialize};

/// The setting that holds the import target, as JSON.
pub const TARGET_KEY: &str = "import_target";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchDto {
    pub id: i64,
    pub started_ms: i64,
    pub last_ms: i64,
    /// Files filed in this batch.
    pub files: i64,
    /// Of those, the ones the library knows as tracks.
    pub tracks: i64,
}

impl From<ImportBatchRow> for ImportBatchDto {
    fn from(row: ImportBatchRow) -> Self {
        Self {
            id: row.id,
            started_ms: row.started_ms,
            last_ms: row.last_ms,
            files: row.files,
            tracks: row.tracks,
        }
    }
}

/// Where imported tracks go once they are in the library. Either may be unset.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportTargetDto {
    /// One of the owner's crates.
    pub crate_id: Option<i64>,
    /// A rekordbox playlist (with writing to rekordbox on).
    pub playlist_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatDto {
    pub format: String,
    pub tracks: i64,
}

fn text(error: &oc_core_db::Error) -> String {
    error.to_string()
}

pub fn batches(service: &LibraryService) -> Result<Vec<ImportBatchDto>, String> {
    service
        .import_batches()
        .map(|rows| rows.into_iter().map(ImportBatchDto::from).collect())
        .map_err(|e| text(&e))
}

/// The track ids behind import rows, once the folder has been scanned.
pub fn tracks(service: &LibraryService, file_ids: &[i64]) -> Result<Vec<i64>, String> {
    if file_ids.len() > crate::crates::MAX_IDS {
        return Err(format!(
            "At most {} files at a time.",
            crate::crates::MAX_IDS
        ));
    }
    service.imported_tracks(file_ids).map_err(|e| text(&e))
}

pub fn formats(service: &LibraryService) -> Result<Vec<FormatDto>, String> {
    service
        .formats()
        .map(|rows| {
            rows.into_iter()
                .map(|(format, tracks)| FormatDto { format, tracks })
                .collect()
        })
        .map_err(|e| text(&e))
}

/// The target as set, minus a crate that no longer exists.
pub fn target(service: &LibraryService) -> Result<ImportTargetDto, String> {
    let stored = service.setting(TARGET_KEY).map_err(|e| text(&e))?;
    let mut target: ImportTargetDto = stored
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    if let Some(crate_id) = target.crate_id {
        let crates = service.crates().map_err(|e| text(&e))?;
        if !crates.iter().any(|row| row.id == crate_id) {
            target.crate_id = None;
        }
    }
    Ok(target)
}

pub fn set_target(
    service: &LibraryService,
    target: &ImportTargetDto,
) -> Result<ImportTargetDto, String> {
    if let Some(crate_id) = target.crate_id {
        let crates = service.crates().map_err(|e| text(&e))?;
        if !crates.iter().any(|row| row.id == crate_id) {
            return Err("That crate is no longer in the list.".to_owned());
        }
    }
    let value = if *target == ImportTargetDto::default() {
        None
    } else {
        Some(serde_json::to_string(target).map_err(|error| error.to_string())?)
    };
    service
        .set_setting(TARGET_KEY, value)
        .map_err(|e| text(&e))?;
    self::target(service)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_kept_and_a_vanished_crate_drops_out_of_it() {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        assert_eq!(target(&service).unwrap(), ImportTargetDto::default());
        let crate_id = service.create_crate("New this week").unwrap();
        let set = set_target(
            &service,
            &ImportTargetDto {
                crate_id: Some(crate_id),
                playlist_id: Some(4),
            },
        )
        .unwrap();
        assert_eq!((set.crate_id, set.playlist_id), (Some(crate_id), Some(4)));
        assert_eq!(target(&service).unwrap(), set);
        assert!(set_target(
            &service,
            &ImportTargetDto {
                crate_id: Some(999),
                playlist_id: None,
            },
        )
        .is_err());
        service.delete_crate(crate_id).unwrap();
        let after = target(&service).unwrap();
        assert_eq!((after.crate_id, after.playlist_id), (None, Some(4)));
        let cleared = set_target(&service, &ImportTargetDto::default()).unwrap();
        assert_eq!(cleared, ImportTargetDto::default());
        assert_eq!(
            service.setting(TARGET_KEY).unwrap(),
            None,
            "unset is removed"
        );
    }
}
