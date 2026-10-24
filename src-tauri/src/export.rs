//! Exporting a list of tracks (the whole library, a crate, a playlist, a release or an artist) to
//! an M3U8 playlist or a CSV spreadsheet, at a place the owner chooses with the system dialog.
//! Only files are read; the one written is the chosen file.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use oc_core_db::library::{LibraryQuery, MAX_WINDOW};
use oc_core_db::service::LibraryService;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    M3u8,
    Csv,
}

impl ExportFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::M3u8 => "m3u8",
            Self::Csv => "csv",
        }
    }
}

/// One track as exported.
#[derive(Clone, Debug)]
pub struct Row {
    pub title: String,
    pub artist: String,
    pub release: String,
    pub bpm: Option<f64>,
    pub key: String,
    pub duration_ms: i64,
    pub format: Option<String>,
    /// Its file; none when no file can be reached.
    pub path: Option<PathBuf>,
}

fn path_text(path: &Path) -> String {
    // Players read a playlist's paths with forward slashes everywhere, Windows included.
    path.to_string_lossy().replace('\\', "/")
}

/// An extended M3U playlist: a length and name line, then the file, for each track with a file.
pub fn render_m3u8(rows: &[Row]) -> String {
    let mut text = String::from("#EXTM3U\n");
    for row in rows {
        let Some(path) = &row.path else {
            continue;
        };
        let _ = write!(
            text,
            "#EXTINF:{},{} - {}\n{}\n",
            row.duration_ms / 1000,
            row.artist,
            row.title,
            path_text(path)
        );
    }
    text
}

fn csv_field(value: &str) -> String {
    if value.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// A CSV with a byte order mark, so a spreadsheet reads it as UTF-8.
pub fn render_csv(rows: &[Row]) -> String {
    let mut text = String::from("\u{feff}Title,Artist,Release,BPM,Key,Length,Format,Path\n");
    for row in rows {
        let seconds = row.duration_ms / 1000;
        let fields = [
            csv_field(&row.title),
            csv_field(&row.artist),
            csv_field(&row.release),
            row.bpm.map(|bpm| bpm.to_string()).unwrap_or_default(),
            csv_field(&row.key),
            format!("{}:{:02}", seconds / 60, seconds % 60),
            row.format.clone().unwrap_or_default(),
            row.path
                .as_deref()
                .map(path_text)
                .map(|path| csv_field(&path))
                .unwrap_or_default(),
        ];
        text.push_str(&fields.join(","));
        text.push('\n');
    }
    text
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedDto {
    pub path: String,
    pub tracks: usize,
    /// Tracks with no file that can be reached; left out of an M3U8, listed without a path in a CSV.
    pub without_file: usize,
}

/// Every track the query lists, in its order, with its file.
fn rows_for(service: &LibraryService, query: &LibraryQuery) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let mut start = 0;
    loop {
        let page = service
            .read_page(query, start, MAX_WINDOW)
            .map_err(|error| error.to_string())?;
        if page.items.is_empty() {
            break;
        }
        for item in &page.items {
            rows.push(Row {
                title: item.title.clone(),
                artist: item.artist.clone(),
                release: item.release.clone(),
                bpm: item.bpm,
                key: item.musical_key.clone(),
                duration_ms: item.duration_ms,
                format: item.format.clone(),
                path: service
                    .track_file(item.id)
                    .map_err(|error| error.to_string())?,
            });
        }
        start += page.items.len();
        if start as u64 >= page.total {
            break;
        }
    }
    Ok(rows)
}

/// Write the query's tracks to `target` in `format`, making the folder if needed.
pub fn export_query(
    service: &LibraryService,
    query: &LibraryQuery,
    format: ExportFormat,
    target: &Path,
) -> Result<ExportedDto, String> {
    let rows = rows_for(service, query)?;
    let text = match format {
        ExportFormat::M3u8 => render_m3u8(&rows),
        ExportFormat::Csv => render_csv(&rows),
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(target, text).map_err(|error| format!("{}: {error}", target.display()))?;
    Ok(ExportedDto {
        path: target.display().to_string(),
        tracks: rows.len(),
        without_file: rows.iter().filter(|row| row.path.is_none()).count(),
    })
}
