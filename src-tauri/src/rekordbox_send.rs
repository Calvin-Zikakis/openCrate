//! New tracks for rekordbox on the XML route: openCrate writes one stable XML file in its own
//! data folder; the owner imports it in rekordbox. Nothing of rekordbox's is written here.

use std::collections::HashMap;
use std::path::PathBuf;

use oc_core_db::offers::Candidate;
use oc_core_db::service::LibraryService;
use oc_rekordbox::xml::{write_offer, NewTrack};
use serde::Serialize;

/// Never "Imported": importing a playlist of that name could replace the owner's own.
pub const PLAYLIST: &str = "openCrate new tracks";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDto {
    pub file_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
    pub container: String,
    pub filed_ms: i64,
    pub offered_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTracksDto {
    pub tracks: Vec<CandidateDto>,
    /// When rekordbox was last read; tracks filed after it may already be in rekordbox.
    pub mirror_read_ms: Option<i64>,
    pub newest_filed_ms: Option<i64>,
    pub xml_path: String,
    pub playlist: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentDto {
    pub xml_path: String,
    pub tracks: usize,
}

pub fn xml_path(service: &LibraryService) -> PathBuf {
    service
        .data_directory()
        .join("rekordbox")
        .join("opencrate.xml")
}

pub fn new_tracks(service: &LibraryService) -> Result<NewTracksDto, String> {
    let (found, mirror_read_ms) = service
        .rekordbox_candidates()
        .map_err(|error| error.to_string())?;
    Ok(NewTracksDto {
        newest_filed_ms: found.iter().map(|candidate| candidate.filed_ms).max(),
        tracks: found
            .into_iter()
            .map(|candidate| CandidateDto {
                file_id: candidate.file_id,
                title: candidate.title,
                artist: candidate.artist,
                album: candidate.album,
                duration_ms: candidate.duration_ms,
                container: candidate.container,
                filed_ms: candidate.filed_ms,
                offered_ms: candidate.offered_ms,
            })
            .collect(),
        mirror_read_ms,
        xml_path: xml_path(service).to_string_lossy().into_owned(),
        playlist: PLAYLIST,
    })
}

fn new_track(candidate: Candidate) -> NewTrack {
    NewTrack {
        path: candidate.path,
        name: candidate.title,
        artist: candidate.artist,
        album: candidate.album,
        year: candidate.year,
        total_seconds: u32::try_from(candidate.duration_ms / 1000).unwrap_or(0),
        size: u64::try_from(candidate.size).unwrap_or(0),
        kind: candidate.container,
    }
}

/// Write the chosen tracks, in the order given, to the stable XML file and remember them as
/// offered. Only current candidates can be sent.
pub fn send(service: &LibraryService, file_ids: &[i64], now_ms: i64) -> Result<SentDto, String> {
    if file_ids.is_empty() {
        return Err("Choose at least one track to send.".into());
    }
    let (found, _) = service
        .rekordbox_candidates()
        .map_err(|error| error.to_string())?;
    let mut by_id: HashMap<i64, Candidate> = found
        .into_iter()
        .map(|candidate| (candidate.file_id, candidate))
        .collect();
    let mut tracks = Vec::with_capacity(file_ids.len());
    for id in file_ids {
        let candidate = by_id.remove(id).ok_or_else(|| {
            "A chosen track is no longer new to rekordbox. Look again.".to_owned()
        })?;
        tracks.push(new_track(candidate));
    }
    let text = write_offer(&tracks, PLAYLIST, env!("CARGO_PKG_VERSION"));
    let path = xml_path(service);
    write_atomically(&path, text.as_bytes()).map_err(|error| error.to_string())?;
    service
        .record_offers(file_ids.to_vec(), now_ms)
        .map_err(|error| error.to_string())?;
    Ok(SentDto {
        xml_path: path.to_string_lossy().into_owned(),
        tracks: tracks.len(),
    })
}

/// A whole file or the previous one: written beside it, flushed, then renamed over it.
fn write_atomically(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let folder = path
        .parent()
        .ok_or_else(|| std::io::Error::other("the XML file has no folder"))?;
    std::fs::create_dir_all(folder)?;
    let temporary = folder.join(".opencrate.xml.part");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use oc_core_db::service::ScanOptions;
    use oc_tag::fixtures::{aiff, write_tags};
    use oc_tag::TagInfo;

    use super::*;

    struct Setup {
        _data: tempfile::TempDir,
        _folder: tempfile::TempDir,
        service: LibraryService,
    }

    /// A library with files filed the way an import files them.
    fn setup(titles: &[&str]) -> Setup {
        let folder = tempfile::tempdir().unwrap();
        let music = folder.path().join("Music");
        let staging = folder.path().join("staging");
        fs::create_dir_all(&music).unwrap();
        fs::create_dir_all(&staging).unwrap();
        let music = music.canonicalize().unwrap();
        let data = tempfile::tempdir().unwrap();
        let service = LibraryService::open(data.path(), 2).unwrap();
        let root = service.add_root(&music, "Music").unwrap();
        for (index, title) in titles.iter().enumerate() {
            let staged = staging.join(format!("{index}.aif"));
            fs::write(&staged, aiff(2)).unwrap();
            write_tags(
                &staged,
                &TagInfo {
                    artist: Some("Studio 001 & Friends".into()),
                    title: Some((*title).into()),
                    album: Some("Between Spaces".into()),
                    ..TagInfo::default()
                },
            );
            let destination = music.join(format!(
                "Studio 001/Between Spaces/{index:02} - {title}.aif"
            ));
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            service.publish_file(staged, destination).unwrap();
        }
        service.scan_root(root, &ScanOptions::default()).unwrap();
        Setup {
            _data: data,
            _folder: folder,
            service,
        }
    }

    #[test]
    fn sending_writes_one_stable_file_and_records_offers() {
        let setup = setup(&["Night Drive", "Late \"Train\""]);
        let listed = new_tracks(&setup.service).unwrap();
        assert_eq!(listed.tracks.len(), 2);
        assert_eq!(listed.mirror_read_ms, None);
        assert!(listed.newest_filed_ms.is_some());
        let ids: Vec<i64> = listed.tracks.iter().map(|track| track.file_id).collect();
        let sent = send(&setup.service, &ids, 42).unwrap();
        assert_eq!(sent.tracks, 2);
        assert_eq!(sent.xml_path, listed.xml_path);
        let text = fs::read_to_string(&sent.xml_path).unwrap();
        let collection = oc_rekordbox::xml::read(std::io::Cursor::new(text.as_bytes())).unwrap();
        let names: Vec<&str> = collection
            .tracks
            .iter()
            .map(|track| track.attributes["Name"].as_str())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Late \"Train\""));
        assert!(collection
            .tracks
            .iter()
            .all(|track| track.attributes["Artist"] == "Studio 001 & Friends"));
        let again = new_tracks(&setup.service).unwrap();
        assert!(again
            .tracks
            .iter()
            .all(|track| track.offered_ms == Some(42)));
        assert!(!Path::new(&sent.xml_path)
            .with_file_name(".opencrate.xml.part")
            .exists());
    }

    #[test]
    fn unknown_or_empty_selections_are_refused() {
        let setup = setup(&["Night Drive"]);
        assert!(send(&setup.service, &[], 1).is_err());
        assert!(send(&setup.service, &[999_999], 1).is_err());
        assert!(
            !xml_path(&setup.service).exists(),
            "nothing is written for a refused send"
        );
    }

    #[test]
    fn sending_again_replaces_the_file_with_the_newest_choice() {
        let setup = setup(&["One", "Two"]);
        let ids: Vec<i64> = new_tracks(&setup.service)
            .unwrap()
            .tracks
            .iter()
            .map(|track| track.file_id)
            .collect();
        send(&setup.service, &ids, 1).unwrap();
        send(&setup.service, &ids[..1], 2).unwrap();
        let text = fs::read_to_string(xml_path(&setup.service)).unwrap();
        let collection = oc_rekordbox::xml::read(std::io::Cursor::new(text.as_bytes())).unwrap();
        assert_eq!(collection.tracks.len(), 1);
        let offered: Vec<Option<i64>> = new_tracks(&setup.service)
            .unwrap()
            .tracks
            .iter()
            .map(|track| track.offered_ms)
            .collect();
        assert!(
            offered.contains(&Some(1)) && offered.contains(&Some(2)),
            "both offers are kept"
        );
    }
}
