//! Editing track details in openCrate: what the files' tags hold, and changes written into
//! every file of each chosen track, through a copy that replaces the file in one rename (never
//! in place). The folders are read again afterwards, so the library shows the change, and the
//! next sync with rekordbox offers it to rekordbox.

use oc_core_db::edits::EditItem;
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

/// Every field and number checked, before any file is touched.
fn checked(changes: &[FieldChange]) -> Result<Vec<(TextTag, String)>, String> {
    let mut tags = Vec::with_capacity(changes.len());
    for change in changes {
        let tag = tag_of(&change.field)
            .ok_or_else(|| format!("{} is not a field the editor can change.", change.field))?;
        let value = change.value.trim().to_owned();
        let number_ok = value.is_empty() || value.parse::<u32>().is_ok_and(|n| n <= 9999);
        if tag == TextTag::Year && !number_ok {
            return Err(format!("\"{value}\" is not a year."));
        }
        if tag == TextTag::TrackNumber && !number_ok {
            return Err(format!("\"{value}\" is not a track number."));
        }
        tags.push((tag, value));
    }
    Ok(tags)
}

/// One track and the changes for it alone.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackEdit {
    pub track_id: i64,
    pub changes: Vec<FieldChange>,
}

/// The name the editor uses for a tag, the way the page sends it.
const fn field_name(tag: TextTag) -> &'static str {
    match tag {
        TextTag::Title => "title",
        TextTag::Artist => "artist",
        TextTag::Album => "album",
        TextTag::AlbumArtist => "albumArtist",
        TextTag::Genre => "genre",
        TextTag::Label => "label",
        TextTag::Year => "year",
        TextTag::TrackNumber => "track",
    }
}

/// What a file holds for a tag, as text; empty when unset.
fn value_of(tags: &oc_tag::TagInfo, tag: TextTag) -> String {
    match tag {
        TextTag::Title => tags.title.clone(),
        TextTag::Artist => tags.artist.clone(),
        TextTag::Album => tags.album.clone(),
        TextTag::AlbumArtist => tags.album_artist.clone(),
        TextTag::Genre => tags.genre.clone(),
        TextTag::Label => tags.label.clone(),
        TextTag::Year => tags.year.map(|year| year.to_string()),
        TextTag::TrackNumber => tags.track_number.map(|number| number.to_string()),
    }
    .unwrap_or_default()
}

/// Write each track's own changes into its files, then read the folders again once. Every edit
/// is checked before any file is touched; a file that cannot be rewritten is reported and the
/// others still change. What each track's first file held before is remembered, so the save
/// can be undone; `remember` is off while undoing.
fn write_each(
    service: &LibraryService,
    edits: &[TrackEdit],
    remember: bool,
) -> Result<EditedDto, String> {
    let mut planned = Vec::with_capacity(edits.len());
    for edit in edits {
        let tags = checked(&edit.changes)?;
        if !tags.is_empty() {
            planned.push((edit.track_id, tags));
        }
    }
    let mut files = 0;
    let mut left = Vec::new();
    let mut roots = Vec::new();
    let mut history = Vec::new();
    for (track_id, tags) in &planned {
        let paths = service
            .track_files(*track_id)
            .map_err(|error| error.to_string())?;
        let before = paths
            .first()
            .and_then(|(_, path)| oc_tag::read_file(path).ok())
            .map(|info| info.tags);
        let mut written = false;
        for (root_id, path) in &paths {
            match oc_tag::rewrite_tags(path, tags) {
                Ok(()) => {
                    files += 1;
                    written = true;
                    if !roots.contains(root_id) {
                        roots.push(*root_id);
                    }
                }
                Err(error) => left.push(format!("{}: {error}", path.display())),
            }
        }
        if let (true, Some(before)) = (written && remember, before) {
            for (tag, value) in tags {
                history.push(EditItem {
                    track_id: *track_id,
                    field: field_name(*tag).to_owned(),
                    before: value_of(&before, *tag),
                    after: value.clone(),
                });
            }
        }
    }
    for root_id in roots {
        service
            .scan_root(root_id, &ScanOptions::default())
            .map_err(|error| error.to_string())?;
    }
    if !history.is_empty() {
        service
            .record_edits(history)
            .map_err(|error| error.to_string())?;
    }
    Ok(EditedDto { files, left })
}

/// Write each track's own changes into its files; the save is remembered so it can be undone.
pub fn edit_each(service: &LibraryService, edits: &[TrackEdit]) -> Result<EditedDto, String> {
    write_each(service, edits, true)
}

/// One save of the editor, as the page lists it.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditBatchDto {
    pub id: i64,
    pub saved_ms: i64,
    pub tracks: usize,
    pub fields: Vec<String>,
}

/// Every save, newest first.
pub fn edit_history(service: &LibraryService) -> Result<Vec<EditBatchDto>, String> {
    Ok(service
        .edit_history()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|batch| EditBatchDto {
            id: batch.id,
            saved_ms: batch.saved_ms,
            tracks: batch.tracks,
            fields: batch.fields,
        })
        .collect())
}

/// Put back what the files held before the newest save, and forget that save.
pub fn undo_last(service: &LibraryService) -> Result<EditedDto, String> {
    let items = service
        .take_last_edits()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "There is nothing to undo.".to_owned())?;
    let mut edits: Vec<TrackEdit> = Vec::new();
    for item in items {
        let change = FieldChange {
            field: item.field,
            value: item.before,
        };
        match edits.iter_mut().find(|edit| edit.track_id == item.track_id) {
            Some(edit) => edit.changes.push(change),
            None => edits.push(TrackEdit {
                track_id: item.track_id,
                changes: vec![change],
            }),
        }
    }
    write_each(service, &edits, false)
}

/// Write the same `changes` into every file of every track in `track_ids`.
pub fn edit_tracks(
    service: &LibraryService,
    track_ids: &[i64],
    changes: &[FieldChange],
) -> Result<EditedDto, String> {
    checked(changes)?;
    let edits: Vec<TrackEdit> = track_ids
        .iter()
        .map(|&track_id| TrackEdit {
            track_id,
            changes: changes.to_vec(),
        })
        .collect();
    edit_each(service, &edits)
}

/// A track's title and artist, for checking the whole library against the cleanup rules.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackNameDto {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
}

pub fn track_names(service: &LibraryService) -> Result<Vec<TrackNameDto>, String> {
    Ok(service
        .track_names()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|(track_id, title, artist)| TrackNameDto {
            track_id,
            title,
            artist,
        })
        .collect())
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenreDto {
    pub name: String,
    pub tracks: i64,
}

/// Genres spelled alike, and the spelling suggested for all of them.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenreGroupDto {
    pub suggested: String,
    pub members: Vec<String>,
    pub tracks: i64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenresDto {
    pub genres: Vec<GenreDto>,
    pub groups: Vec<GenreGroupDto>,
}

/// Genres alike when they match ignoring case, spaces and punctuation: "Deep House",
/// "deep-house" and "`DeepHouse`".
fn genre_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Every genre with its track count, and the groups spelled alike. A group's suggestion is its
/// commonest spelling; on a tie, one that is not all lower case.
pub fn genres(service: &LibraryService) -> Result<GenresDto, String> {
    let counted = service.genres().map_err(|error| error.to_string())?;
    let mut groups: std::collections::BTreeMap<String, Vec<(String, i64)>> =
        std::collections::BTreeMap::new();
    for (name, tracks) in &counted {
        groups
            .entry(genre_key(name))
            .or_default()
            .push((name.clone(), *tracks));
    }
    let mut alike: Vec<GenreGroupDto> = groups
        .into_values()
        .filter(|members| members.len() > 1)
        .map(|mut members| {
            members.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let suggested = members
                .iter()
                .max_by_key(|(name, tracks)| (*tracks, *name != name.to_lowercase()))
                .map(|(name, _)| name.clone())
                .unwrap_or_default();
            GenreGroupDto {
                suggested,
                tracks: members.iter().map(|(_, tracks)| tracks).sum(),
                members: members.into_iter().map(|(name, _)| name).collect(),
            }
        })
        .collect();
    alike.sort_by(|a, b| {
        b.tracks
            .cmp(&a.tracks)
            .then_with(|| a.suggested.cmp(&b.suggested))
    });
    Ok(GenresDto {
        genres: counted
            .into_iter()
            .map(|(name, tracks)| GenreDto { name, tracks })
            .collect(),
        groups: alike,
    })
}

/// Give every track with one of the genres in `from` the genre `to`.
pub fn rename_genres(
    service: &LibraryService,
    from: &[String],
    to: &str,
) -> Result<EditedDto, String> {
    let track_ids = service
        .tracks_with_genres(from)
        .map_err(|error| error.to_string())?;
    edit_tracks(
        service,
        &track_ids,
        &[FieldChange {
            field: "genre".into(),
            value: to.to_owned(),
        }],
    )
}
