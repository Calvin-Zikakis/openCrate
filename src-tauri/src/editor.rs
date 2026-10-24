//! Editing track details in openCrate: what the files' tags hold, and changes written into
//! every file of each chosen track, through a copy that replaces the file in one rename (never
//! in place). The folders are read again afterwards, so the library shows the change, and the
//! next sync with rekordbox offers it to rekordbox.

use oc_core_db::service::{LibraryService, ScanOptions};
use oc_tag::TextTag;
use serde::{Deserialize, Serialize};

/// What one track's file holds, for the editor.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackTagsDto {
    pub track_id: i64,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub year: Option<i32>,
    pub track: Option<u32>,
}

/// The tags of each track's first file, in the order asked. A track with no file that can be
/// reached is left out.
pub fn track_tags(
    service: &LibraryService,
    track_ids: &[i64],
) -> Result<Vec<TrackTagsDto>, String> {
    let mut found = Vec::with_capacity(track_ids.len());
    for &track_id in track_ids {
        let files = service
            .track_files(track_id)
            .map_err(|error| error.to_string())?;
        let Some((_, path)) = files.first() else {
            continue;
        };
        let tags = oc_tag::read_file(path)
            .map_err(|error| error.to_string())?
            .tags;
        found.push(TrackTagsDto {
            track_id,
            title: tags.title,
            artist: tags.artist,
            album: tags.album,
            album_artist: tags.album_artist,
            genre: tags.genre,
            label: tags.label,
            year: tags.year,
            track: tags.track_number,
        });
    }
    Ok(found)
}

/// One field set to one value; empty clears it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditedDto {
    /// Files rewritten.
    pub files: usize,
    /// Files left as they were, and why.
    pub left: Vec<String>,
}

fn tag_of(field: &str) -> Option<TextTag> {
    Some(match field {
        "title" => TextTag::Title,
        "artist" => TextTag::Artist,
        "album" => TextTag::Album,
        "albumArtist" => TextTag::AlbumArtist,
        "genre" => TextTag::Genre,
        "label" => TextTag::Label,
        "year" => TextTag::Year,
        "track" => TextTag::TrackNumber,
        _ => return None,
    })
}

/// Write `changes` into every file of every track in `track_ids`. Every field and number is
/// checked before any file is touched; a file that cannot be rewritten is reported and the
/// others still change.
pub fn edit_tracks(
    service: &LibraryService,
    track_ids: &[i64],
    changes: &[FieldChange],
) -> Result<EditedDto, String> {
    let mut tags = Vec::with_capacity(changes.len());
    for change in changes {
        let tag = tag_of(&change.field)
            .ok_or_else(|| format!("{} is not a field the editor can change.", change.field))?;
        let value = change.value.trim().to_owned();
        let number_ok = |max: u32| value.is_empty() || value.parse::<u32>().is_ok_and(|n| n <= max);
        if tag == TextTag::Year && !number_ok(9999) {
            return Err(format!("\"{value}\" is not a year."));
        }
        if tag == TextTag::TrackNumber && !number_ok(9999) {
            return Err(format!("\"{value}\" is not a track number."));
        }
        tags.push((tag, value));
    }
    if tags.is_empty() {
        return Ok(EditedDto {
            files: 0,
            left: Vec::new(),
        });
    }
    let mut files = 0;
    let mut left = Vec::new();
    let mut roots = Vec::new();
    for &track_id in track_ids {
        for (root_id, path) in service
            .track_files(track_id)
            .map_err(|error| error.to_string())?
        {
            match oc_tag::rewrite_tags(&path, &tags) {
                Ok(()) => {
                    files += 1;
                    if !roots.contains(&root_id) {
                        roots.push(root_id);
                    }
                }
                Err(error) => left.push(format!("{}: {error}", path.display())),
            }
        }
    }
    for root_id in roots {
        service
            .scan_root(root_id, &ScanOptions::default())
            .map_err(|error| error.to_string())?;
    }
    Ok(EditedDto { files, left })
}
