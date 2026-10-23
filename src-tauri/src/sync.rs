//! rekordbox read-back for the UI: import an XML export, read it again, and explain
//! unmatched entries. The export file comes only from the native file dialog or from the path
//! already stored for it; the page never supplies a file path. It may send a Location prefix,
//! which is text to match against, not somewhere to read.

use std::path::Path;

use oc_core_db::library::PlaylistRow;
use oc_core_db::mirror::{AliasSuggestion, MirrorCounts, MirrorInfo, MirrorOutcome, TrackAnalysis};
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
    /// For display only; lossy where the name is not text.
    pub path: String,
    pub generation: i64,
    pub entries: i64,
    pub file_mtime_ms: Option<i64>,
    pub last_read_ms: Option<i64>,
    pub last_attempt_ms: Option<i64>,
    pub last_error: Option<String>,
    pub counts: CountsDto,
}

impl From<MirrorInfo> for MirrorDto {
    fn from(info: MirrorInfo) -> Self {
        Self {
            source_id: info.status.source_id,
            path: String::from_utf8_lossy(&info.status.path_raw).into_owned(),
            generation: info.status.generation,
            entries: info.status.entries,
            file_mtime_ms: info.status.file_mtime_ms,
            last_read_ms: info.status.last_read_ms,
            last_attempt_ms: info.status.last_attempt_ms,
            last_error: info.status.last_error,
            counts: CountsDto::from(&info.counts),
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
