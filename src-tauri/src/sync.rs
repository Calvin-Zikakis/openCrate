//! rekordbox read-back for the UI: import an XML export, read it again, and explain
//! unmatched entries. The export file comes only from the native file dialog or from the path
//! already stored for it; the page never supplies a file path. It may send a Location prefix,
//! which is text to match against, not somewhere to read.

use std::path::Path;

use oc_core_db::library::PlaylistRow;
use oc_core_db::mirror::{
    AliasSuggestion, MirrorCounts, MirrorInfo, MirrorOutcome, SourceKind, TrackAnalysis,
};
use oc_core_db::service::LibraryService;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountsDto {
    pub matched: i64,
    pub unmapped: i64,
    pub not_indexed: i64,
    pub ambiguous: i64,
    pub unrecognized: i64,
    pub playlists: i64,
    pub unresolved_references: i64,
}

impl From<&MirrorCounts> for CountsDto {
    fn from(counts: &MirrorCounts) -> Self {
        Self {
            matched: counts.matched,
            unmapped: counts.unmapped,
            not_indexed: counts.not_indexed,
            ambiguous: counts.ambiguous,
            unrecognized: counts.unrecognized,
            playlists: counts.playlists,
            unresolved_references: counts.unresolved_references,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorDto {
    pub source_id: i64,
    /// `xml` for an export file, `master_db` for a read of rekordbox's own library.
    pub kind: &'static str,
    /// For display only; lossy where the name is not text.
    pub path: String,
    pub generation: i64,
    pub entries: i64,
    pub file_mtime_ms: Option<i64>,
    pub last_read_ms: Option<i64>,
    pub last_attempt_ms: Option<i64>,
    pub last_error: Option<String>,
    pub counts: CountsDto,
    /// The export file, or rekordbox's own library, changed after the last read.
    pub changed_since_read: bool,
}

fn modified_ms(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let since = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}

/// Whether the source changed after it was read: an export file whose modified time differs
/// from the one read, or rekordbox's library (or its log) written after the read. Only looked
/// at; a source that cannot be found counts as unchanged.
fn changed_since_read(info: &MirrorInfo) -> bool {
    let Ok(path) =
        oc_paths::bytes_to_os_string(&info.status.path_raw).map(std::path::PathBuf::from)
    else {
        return false;
    };
    match info.status.kind {
        SourceKind::Xml => match (modified_ms(&path), info.status.file_mtime_ms) {
            (Some(now), Some(read)) => now != read,
            _ => false,
        },
        SourceKind::MasterDb => {
            let mut log = path.clone().into_os_string();
            log.push("-wal");
            let newest = [modified_ms(&path), modified_ms(Path::new(&log))]
                .into_iter()
                .flatten()
                .max();
            matches!((newest, info.status.last_read_ms), (Some(now), Some(read)) if now > read)
        }
    }
}

impl From<MirrorInfo> for MirrorDto {
    fn from(info: MirrorInfo) -> Self {
        let changed = changed_since_read(&info);
        Self {
            source_id: info.status.source_id,
            kind: info.status.kind.as_str(),
            path: String::from_utf8_lossy(&info.status.path_raw).into_owned(),
            generation: info.status.generation,
            entries: info.status.entries,
            file_mtime_ms: info.status.file_mtime_ms,
            last_read_ms: info.status.last_read_ms,
            last_attempt_ms: info.status.last_attempt_ms,
            last_error: info.status.last_error,
            counts: CountsDto::from(&info.counts),
            changed_since_read: changed,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDto {
    pub source_id: i64,
    pub generation: i64,
    /// Nothing differed from the stored mirror, so nothing was rewritten.
    pub unchanged: bool,
    pub entries: usize,
    pub matched: usize,
    /// Tracks that now carry tempo and key from this export.
    pub derived_set: usize,
}

impl From<&MirrorOutcome> for ImportDto {
    fn from(outcome: &MirrorOutcome) -> Self {
        Self {
            source_id: outcome.source_id,
            generation: outcome.generation,
            unchanged: outcome.unchanged,
            entries: outcome.entries,
            matched: outcome.matched,
            derived_set: outcome.derived_set,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionDto {
    pub root_id: i64,
    pub root_label: String,
    pub prefix: String,
    pub entries: usize,
}

impl From<AliasSuggestion> for SuggestionDto {
    fn from(suggestion: AliasSuggestion) -> Self {
        Self {
            root_id: suggestion.root_id,
            root_label: suggestion.root_label,
            prefix: suggestion.prefix,
            entries: suggestion.entries,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDto {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub is_folder: bool,
    pub name: String,
    pub position: i64,
    pub entries: i64,
    pub in_library: i64,
}

impl From<PlaylistRow> for PlaylistDto {
    fn from(row: PlaylistRow) -> Self {
        Self {
            id: row.id,
            parent_id: row.parent_id,
            is_folder: row.is_folder,
            name: row.name,
            position: row.position,
            entries: row.entries,
            in_library: row.in_library,
        }
    }
}

pub fn playlists(service: &LibraryService) -> Result<Vec<PlaylistDto>, String> {
    service
        .playlists()
        .map(|rows| rows.into_iter().map(PlaylistDto::from).collect())
        .map_err(|error| error.to_string())
}

const SUGGESTIONS: usize = 5;

pub fn mirrors(service: &LibraryService) -> Result<Vec<MirrorDto>, String> {
    service
        .mirrors()
        .map(|all| all.into_iter().map(MirrorDto::from).collect())
        .map_err(|error| error.to_string())
}

/// Read the export the owner picked.
pub fn import(service: &LibraryService, path: &Path) -> Result<ImportDto, String> {
    service
        .import_export(path)
        .map(|outcome| ImportDto::from(&outcome))
        .map_err(|error| error.to_string())
}

pub fn reread(service: &LibraryService, source_id: i64) -> Result<ImportDto, String> {
    service
        .reread_mirror(source_id)
        .map(|outcome| ImportDto::from(&outcome))
        .map_err(|error| error.to_string())
}

pub fn suggestions(service: &LibraryService, source_id: i64) -> Result<Vec<SuggestionDto>, String> {
    service
        .alias_suggestions(source_id, SUGGESTIONS)
        .map(|all| all.into_iter().map(SuggestionDto::from).collect())
        .map_err(|error| error.to_string())
}

pub fn add_alias(service: &LibraryService, root_id: i64, prefix: &str) -> Result<bool, String> {
    service
        .add_alias(root_id, prefix)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueDto {
    pub kind: i64,
    pub number: Option<i64>,
    pub start_seconds: f64,
    pub end_seconds: Option<f64>,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempoDto {
    pub start_seconds: f64,
    pub bpm: f64,
    pub meter: Option<String>,
    pub beat: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDto {
    pub generation: i64,
    pub bpm: Option<f64>,
    pub musical_key: String,
    pub cues: Vec<CueDto>,
    pub tempos: Vec<TempoDto>,
}

impl From<TrackAnalysis> for AnalysisDto {
    fn from(analysis: TrackAnalysis) -> Self {
        Self {
            generation: analysis.generation,
            bpm: analysis.bpm,
            musical_key: analysis.musical_key,
            cues: analysis
                .cues
                .into_iter()
                .map(|cue| CueDto {
                    kind: cue.kind,
                    number: cue.number,
                    start_seconds: cue.start_seconds,
                    end_seconds: cue.end_seconds,
                    name: cue.name,
                })
                .collect(),
            tempos: analysis
                .tempos
                .into_iter()
                .map(|tempo| TempoDto {
                    start_seconds: tempo.start_seconds,
                    bpm: tempo.bpm,
                    meter: tempo.meter,
                    beat: tempo.beat,
                })
                .collect(),
        }
    }
}

/// The cues and beat grid rekordbox has for a library track; `None` if no export matched it.
pub fn track_analysis(
    service: &LibraryService,
    track_id: i64,
) -> Result<Option<AnalysisDto>, String> {
    service
        .track_analysis(track_id)
        .map(|found| found.map(AnalysisDto::from))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod changed_tests {
    use std::fs;
    use std::time::{Duration, SystemTime};

    use oc_core_db::service::LibraryService;

    use super::mirrors;

    const EXPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0"><COLLECTION Entries="0"></COLLECTION><PLAYLISTS><NODE Type="0" Name="ROOT" Count="0"/></PLAYLISTS></DJ_PLAYLISTS>"#;

    fn later(path: &std::path::Path) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(120))
            .unwrap();
    }

    #[test]
    fn an_export_changed_since_its_read_is_pointed_out() {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let folder = tempfile::tempdir().unwrap();
        let export = folder.path().join("collection.xml");
        fs::write(&export, EXPORT).unwrap();
        service.import_export(&export).unwrap();
        assert!(!mirrors(&service).unwrap()[0].changed_since_read);
        later(&export);
        assert!(mirrors(&service).unwrap()[0].changed_since_read);
    }

    #[test]
    fn a_rekordbox_library_written_since_the_read_is_pointed_out() {
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let folder = tempfile::tempdir().unwrap();
        let live = folder.path().join("master.db");
        fs::write(&live, b"x").unwrap();
        let empty = oc_rekordbox::xml::read(std::io::Cursor::new(EXPORT.as_bytes())).unwrap();
        service.import_database(&live, empty).unwrap();
        assert!(!mirrors(&service).unwrap()[0].changed_since_read);
        let wal = folder.path().join("master.db-wal");
        fs::write(&wal, b"y").unwrap();
        later(&wal);
        assert!(
            mirrors(&service).unwrap()[0].changed_since_read,
            "a change still in the log counts"
        );
    }
}
