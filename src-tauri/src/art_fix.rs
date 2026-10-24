//! Album art across the library: releases whose files carry no picture, and giving a release a
//! cover from the image beside its files, one the owner chooses, or the Cover Art Archive. A
//! cover goes into every file of the release through a copy that replaces the file, never in
//! place. Removing art strips every picture the same way.

use std::path::{Path, PathBuf};

use oc_core_db::service::LibraryService;
use oc_lookup::{cover_art_url, parse_search, pick, search_url};
use oc_tag::{embedded_cover, folder_cover, Cover};
use serde::Serialize;

use crate::artwork::ArtCache;
use crate::lookup::Lookups;

/// Files of a release looked at for a picture before it counts as having none.
const FILES_TRIED: usize = 6;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MissingArtDto {
    pub release_id: i64,
    pub artist: String,
    pub title: String,
    pub files: usize,
    /// An image such as cover.jpg sits beside the files.
    pub folder_image: bool,
}

/// Every release whose files carry no picture. Reads the first few files of each.
pub fn releases_without_art(service: &LibraryService) -> Result<Vec<MissingArtDto>, String> {
    let mut missing = Vec::new();
    for (release_id, artist, title) in service.releases_all().map_err(|error| error.to_string())? {
        let files = service
            .release_files(release_id, usize::MAX)
            .map_err(|error| error.to_string())?;
        if files
            .iter()
            .take(FILES_TRIED)
            .any(|path| embedded_cover(path).is_some())
        {
            continue;
        }
        let folder_image = files
            .first()
            .and_then(|path| path.parent())
            .and_then(folder_cover)
            .is_some();
        missing.push(MissingArtDto {
            release_id,
            artist,
            title,
            files: files.len(),
            folder_image,
        });
    }
    Ok(missing)
}

/// Where a cover comes from.
pub enum ArtSource {
    /// The image beside the release's files.
    Folder,
    /// An image file the owner chose.
    File(PathBuf),
    /// Image bytes already fetched, from the Cover Art Archive.
    Bytes(Cover),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtChangedDto {
    /// Files rewritten.
    pub files: usize,
    /// Files left as they were, and why.
    pub left: Vec<String>,
}

fn cover_from_file(path: &Path) -> Result<Cover, String> {
    let data = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if data.len() > oc_tag::MAX_COVER_BYTES {
        return Err(format!(
            "{} is too large for a cover (over 12 MB).",
            path.display()
        ));
    }
    // The same sniffing as a cover beside the files: only a real image is embedded.
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    std::fs::write(dir.path().join("cover.png"), &data).map_err(|error| error.to_string())?;
    folder_cover(dir.path())
        .ok_or_else(|| format!("{} is not a JPEG, PNG, WebP or GIF image.", path.display()))
}

fn rewrite_all(
    service: &LibraryService,
    cache: &ArtCache,
    release_id: i64,
    cover: Option<&Cover>,
) -> Result<ArtChangedDto, String> {
    let paths = service
        .release_files(release_id, usize::MAX)
        .map_err(|error| error.to_string())?;
    let mut files = 0;
    let mut left = Vec::new();
    for path in &paths {
        match oc_tag::rewrite_cover(path, cover) {
            Ok(()) => files += 1,
            Err(error) => left.push(format!("{}: {error}", path.display())),
        }
    }
    cache.forget(release_id);
    Ok(ArtChangedDto { files, left })
}

/// Give every file of a release the cover from `source`, replacing what each had.
pub fn set_release_art(
    service: &LibraryService,
    cache: &ArtCache,
    release_id: i64,
    source: &ArtSource,
) -> Result<ArtChangedDto, String> {
    let cover = match source {
        ArtSource::Folder => service
            .release_files(release_id, 1)
            .map_err(|error| error.to_string())?
            .first()
            .and_then(|path| path.parent())
            .and_then(folder_cover)
            .ok_or_else(|| "There is no image beside this release's files.".to_owned())?,
        ArtSource::File(path) => cover_from_file(path)?,
        ArtSource::Bytes(cover) => cover.clone(),
    };
    rewrite_all(service, cache, release_id, Some(&cover))
}

/// Strip every picture from every file of a release.
pub fn remove_release_art(
    service: &LibraryService,
    cache: &ArtCache,
    release_id: i64,
) -> Result<ArtChangedDto, String> {
    rewrite_all(service, cache, release_id, None)
}

/// Tracks compared for "the same cover" before the answer is "they differ".
const TRACKS_COMPARED: usize = 60;

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackCoversDto {
    /// "none" (no chosen track has a picture), "same" (each has the same one) or "mixed".
    pub state: &'static str,
    /// An image such as cover.jpg sits beside the files of at least one chosen track.
    pub folder_image: bool,
}

/// Whether the chosen tracks carry a picture, and the same one; read from each track's first file.
pub fn track_covers(service: &LibraryService, track_ids: &[i64]) -> Result<TrackCoversDto, String> {
    let mut covers = Vec::new();
    let mut folder_image = false;
    for track_id in track_ids.iter().take(TRACKS_COMPARED) {
        let path = service
            .track_file(*track_id)
            .map_err(|error| error.to_string())?;
        let Some(path) = path else { continue };
        folder_image = folder_image || path.parent().and_then(folder_cover).is_some();
        covers.push(embedded_cover(&path).map(|cover| cover.data));
    }
    let state = if track_ids.len() > TRACKS_COMPARED {
        "mixed"
    } else if covers.iter().all(Option::is_none) {
        "none"
    } else if covers.windows(2).all(|pair| pair[0] == pair[1]) {
        "same"
    } else {
        "mixed"
    };
    Ok(TrackCoversDto {
        state,
        folder_image,
    })
}

/// A cover for chosen tracks: the image the owner picked (held until saved), each track's own
/// image beside its files, or none.
pub enum TrackArt<'a> {
    Picked(&'a Cover),
    Folder,
    Remove,
}

/// Give every file of the chosen tracks a cover, or strip theirs. A track with no image beside
/// its files is left as it is when the cover comes from there.
pub fn set_tracks_art(
    service: &LibraryService,
    cache: &ArtCache,
    track_ids: &[i64],
    art: &TrackArt<'_>,
) -> Result<ArtChangedDto, String> {
    let mut files = 0;
    let mut left = Vec::new();
    for track_id in track_ids {
        let paths = service
            .track_files(*track_id)
            .map_err(|error| error.to_string())?;
        let folder;
        let cover = match art {
            TrackArt::Picked(cover) => Some(*cover),
            TrackArt::Remove => None,
            TrackArt::Folder => {
                folder = paths
                    .first()
                    .and_then(|(_, path)| path.parent())
                    .and_then(folder_cover);
                if folder.is_none() {
                    if let Some((_, path)) = paths.first() {
                        left.push(format!("{}: there is no image beside it", path.display()));
                    }
                    continue;
                }
                folder.as_ref()
            }
        };
        for (_, path) in &paths {
            match oc_tag::rewrite_cover(path, cover) {
                Ok(()) => files += 1,
                Err(error) => left.push(format!("{}: {error}", path.display())),
            }
        }
        cache.forget_track(*track_id);
        if let Some(release_id) = service
            .track_release(*track_id)
            .map_err(|error| error.to_string())?
        {
            cache.forget(release_id);
        }
    }
    Ok(ArtChangedDto { files, left })
}

/// Read and check an image the owner picked, to hold until the editor saves.
pub fn picked_cover(path: &Path) -> Result<Cover, String> {
    cover_from_file(path)
}

/// The front cover the Cover Art Archive holds for the MusicBrainz release that best matches
/// `artist` and `title`, with `files` tracks. Two or three requests; nothing is written.
pub fn find_online(
    lookups: &Lookups,
    artist: &str,
    title: &str,
    files: usize,
) -> Result<Cover, String> {
    let candidates = parse_search(&lookups.musicbrainz.get(&search_url(artist, title), None)?)
        .map_err(|error| error.to_string())?;
    let best = pick(&candidates, files).ok_or_else(|| {
        format!("MusicBrainz has no release that matches \"{title}\" by {artist} closely enough.")
    })?;
    let data = lookups
        .archive
        .get_bytes(&cover_art_url(&best.id))
        .map_err(|error| {
            if error.contains("no longer has") {
                format!("The Cover Art Archive has no cover for \"{title}\" by {artist}.")
            } else {
                error
            }
        })?;
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    std::fs::write(dir.path().join("cover.png"), &data).map_err(|error| error.to_string())?;
    folder_cover(dir.path())
        .ok_or_else(|| "The Cover Art Archive sent something that is not an image.".to_owned())
}
