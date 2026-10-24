//! New tracks for rekordbox on the XML route: openCrate writes one stable XML file in its own
//! data folder; the owner imports it in rekordbox. Nothing of rekordbox's is written here.

use std::collections::HashMap;
use std::path::PathBuf;

use oc_core_db::offers::{Candidate, CrateTrack};
use oc_core_db::service::LibraryService;
use oc_rekordbox::location::encode_location;
use oc_rekordbox::xml::{write_document, Entry, NewTrack, OfferPlaylist};
use serde::{Deserialize, Serialize};

/// Never "Imported": importing a playlist of that name could replace the owner's own.
pub const PLAYLIST: &str = "openCrate new tracks";
/// Crates the owner puts in the file go in this folder, each as a playlist.
pub const CRATES_FOLDER: &str = "openCrate crates";
/// What the file holds, so it can be written again whole: the last new tracks sent and the
/// crates switched on.
const STATE: &str = "rekordbox_xml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileState {
    new: Vec<i64>,
    crates: Vec<i64>,
}

fn load_state(service: &LibraryService) -> Result<FileState, String> {
    Ok(service
        .setting(STATE)
        .map_err(|error| error.to_string())?
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

fn save_state(service: &LibraryService, state: &FileState) -> Result<(), String> {
    let text = serde_json::to_string(state).map_err(|error| error.to_string())?;
    service
        .set_setting(STATE, Some(text))
        .map_err(|error| error.to_string())
}

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
    /// Kept out of rekordbox by the owner.
    pub hidden: bool,
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
                hidden: candidate.hidden,
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
    let mut chosen: Vec<i64> = Vec::with_capacity(file_ids.len());
    for id in file_ids {
        if !chosen.contains(id) {
            chosen.push(*id);
        }
    }
    if chosen.is_empty() {
        return Err("Choose at least one track to send.".into());
    }
    let (found, _) = service
        .rekordbox_candidates()
        .map_err(|error| error.to_string())?;
    let offerable: std::collections::HashSet<i64> = found
        .iter()
        .filter(|candidate| !candidate.hidden)
        .map(|candidate| candidate.file_id)
        .collect();
    if chosen.iter().any(|id| !offerable.contains(id)) {
        return Err("A chosen track is no longer new to rekordbox. Look again.".into());
    }
    // The state the file is made from and the offers shown change together; the file follows,
    // and is made again from the state whenever anything changes, so a failed write heals.
    let mut state = load_state(service)?;
    state.new.clone_from(&chosen);
    let text = serde_json::to_string(&state).map_err(|error| error.to_string())?;
    service
        .record_offers_with_setting(STATE, text, chosen.clone(), now_ms)
        .map_err(|error| error.to_string())?;
    let path = regenerate(service)?;
    Ok(SentDto {
        xml_path: path.to_string_lossy().into_owned(),
        tracks: chosen.len(),
    })
}

/// Keep tracks out of rekordbox, or bring them back, and write the file again.
pub fn set_hidden(
    service: &LibraryService,
    file_ids: &[i64],
    hidden: bool,
    now_ms: i64,
) -> Result<NewTracksDto, String> {
    service
        .set_rekordbox_hidden(file_ids.to_vec(), hidden, now_ms)
        .map_err(|error| error.to_string())?;
    regenerate(service)?;
    new_tracks(service)
}

/// The document's entries, each written once however many playlists hold it.
#[derive(Default)]
struct Entries {
    list: Vec<Entry>,
    by_location: HashMap<String, usize>,
}

impl Entries {
    fn add(&mut self, entry: Entry) -> usize {
        let location = match &entry {
            Entry::New(track) => encode_location(&track.path),
            Entry::Echo(track) => track
                .attributes
                .get("Location")
                .cloned()
                .unwrap_or_else(|| encode_location(&track.location_bytes)),
        };
        if let Some(index) = self.by_location.get(&location) {
            return *index;
        }
        self.list.push(entry);
        self.by_location.insert(location, self.list.len() - 1);
        self.list.len() - 1
    }
}

/// Every crate with its tracks, gathered once per request.
type Gathered = Vec<(oc_core_db::crates::CrateRow, Vec<CrateTrack>)>;

fn gather(service: &LibraryService) -> Result<Gathered, String> {
    let mut found = Vec::new();
    for row in service.crates().map_err(|error| error.to_string())? {
        let (_, tracks) = service
            .crate_for_rekordbox(row.id)
            .map_err(|error| error.to_string())?;
        found.push((row, tracks));
    }
    Ok(found)
}

/// Write openCrate's rekordbox file from what it should hold: the last new tracks sent that
/// rekordbox still lacks, and every crate switched on, as it is now (from `gathered`, which
/// may hold crates not in the file).
fn write_file(
    service: &LibraryService,
    state: &FileState,
    gathered: &Gathered,
) -> Result<PathBuf, String> {
    let (found, _) = service
        .rekordbox_candidates()
        .map_err(|error| error.to_string())?;
    let mut by_id: HashMap<i64, Candidate> = found
        .into_iter()
        .filter(|candidate| !candidate.hidden)
        .map(|candidate| (candidate.file_id, candidate))
        .collect();
    let mut entries = Entries::default();
    let mut playlists = Vec::new();
    let new: Vec<usize> = state
        .new
        .iter()
        .filter_map(|id| by_id.remove(id))
        .map(|candidate| entries.add(Entry::New(new_track(candidate))))
        .collect();
    if !new.is_empty() {
        playlists.push(OfferPlaylist {
            folder: None,
            name: PLAYLIST.to_owned(),
            entries: new,
        });
    }
    for crate_id in &state.crates {
        // A crate deleted since is left out.
        let Some((row, tracks)) = gathered.iter().find(|(row, _)| row.id == *crate_id) else {
            continue;
        };
        let members = tracks
            .iter()
            .filter_map(|track| match track {
                CrateTrack::Echo(entry) => Some(Entry::Echo(entry.clone())),
                CrateTrack::New(entry) => Some(Entry::New(entry.clone())),
                CrateTrack::DatabaseOnly { .. } | CrateTrack::NoFile { .. } => None,
            })
            .map(|entry| entries.add(entry))
            .collect();
        playlists.push(OfferPlaylist {
            folder: Some(CRATES_FOLDER.to_owned()),
            name: row.name.clone(),
            entries: members,
        });
    }
    let text = write_document(&entries.list, &playlists, env!("CARGO_PKG_VERSION"));
    let path = xml_path(service);
    write_atomically(&path, text.as_bytes()).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Write openCrate's rekordbox file again. Gathers only the crates in the file.
pub fn regenerate(service: &LibraryService) -> Result<PathBuf, String> {
    let state = load_state(service)?;
    let mut gathered = Vec::new();
    for crate_id in &state.crates {
        if let Ok((name, tracks)) = service.crate_for_rekordbox(*crate_id) {
            gathered.push((
                oc_core_db::crates::CrateRow {
                    id: *crate_id,
                    name,
                    tracks: i64::try_from(tracks.len()).unwrap_or(0),
                },
                tracks,
            ));
        }
    }
    write_file(service, &state, &gathered)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrateStatusDto {
    pub crate_id: i64,
    pub name: String,
    pub in_file: bool,
    /// Tracks rekordbox has, written back as its own entries.
    pub known: usize,
    /// Tracks rekordbox does not have yet.
    pub new: usize,
    /// Tracks rekordbox has according to a direct read only; left out until an XML export is read.
    pub needs_export: usize,
    /// Tracks whose files cannot be reached; left out.
    pub no_file: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CratesDto {
    pub crates: Vec<CrateStatusDto>,
    pub xml_path: String,
    pub folder: &'static str,
}

fn crates_from(service: &LibraryService, state: &FileState, gathered: &Gathered) -> CratesDto {
    let crates = gathered
        .iter()
        .map(|(row, tracks)| {
            let count = |wanted: fn(&CrateTrack) -> bool| {
                tracks.iter().filter(|track| wanted(track)).count()
            };
            CrateStatusDto {
                crate_id: row.id,
                name: row.name.clone(),
                in_file: state.crates.contains(&row.id),
                known: count(|track| matches!(track, CrateTrack::Echo(_))),
                new: count(|track| matches!(track, CrateTrack::New(_))),
                needs_export: count(|track| matches!(track, CrateTrack::DatabaseOnly { .. })),
                no_file: count(|track| matches!(track, CrateTrack::NoFile { .. })),
            }
        })
        .collect();
    CratesDto {
        crates,
        xml_path: xml_path(service).to_string_lossy().into_owned(),
        folder: CRATES_FOLDER,
    }
}

pub fn crates(service: &LibraryService) -> Result<CratesDto, String> {
    let state = load_state(service)?;
    Ok(crates_from(service, &state, &gather(service)?))
}

/// Put a crate in openCrate's rekordbox file, or take it out, and write the file again.
pub fn set_crate(
    service: &LibraryService,
    crate_id: i64,
    in_file: bool,
) -> Result<CratesDto, String> {
    let gathered = gather(service)?;
    if !gathered.iter().any(|(row, _)| row.id == crate_id) {
        return Err("That crate was not found.".into());
    }
    let mut state = load_state(service)?;
    state.crates.retain(|id| *id != crate_id);
    if in_file {
        state.crates.push(crate_id);
    }
    save_state(service, &state)?;
    write_file(service, &state, &gathered)?;
    Ok(crates_from(service, &state, &gathered))
}

/// Write the file again with the crates as they are now, and list them.
pub fn update_file(service: &LibraryService) -> Result<CratesDto, String> {
    let state = load_state(service)?;
    let gathered = gather(service)?;
    write_file(service, &state, &gathered)?;
    Ok(crates_from(service, &state, &gathered))
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
        let music = oc_paths::canonical(&music).unwrap();
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
            // The title is in the tags; the file name is plain, as an import makes it (a quote is
            // not allowed in a Windows file name).
            let destination = music.join(format!("Studio 001/Between Spaces/{index:02}.aif"));
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

    fn read_back(service: &LibraryService) -> oc_rekordbox::xml::Collection {
        let text = fs::read_to_string(xml_path(service)).unwrap();
        oc_rekordbox::xml::read(std::io::Cursor::new(text.as_bytes())).unwrap()
    }

    fn playlist_names(collection: &oc_rekordbox::xml::Collection) -> Vec<String> {
        use oc_rekordbox::xml::PlaylistNode;
        fn walk(nodes: &[PlaylistNode], prefix: &str, out: &mut Vec<String>) {
            for node in nodes {
                match node {
                    PlaylistNode::Folder {
                        attributes,
                        children,
                    } => {
                        let name = &attributes["Name"];
                        let next = if name == "ROOT" {
                            String::new()
                        } else {
                            format!("{prefix}{name}/")
                        };
                        walk(children, &next, out);
                    }
                    PlaylistNode::Playlist {
                        attributes,
                        track_ids,
                    } => {
                        out.push(format!(
                            "{prefix}{} ({})",
                            attributes["Name"],
                            track_ids.len()
                        ));
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(&collection.playlists, "", &mut out);
        out
    }

    #[test]
    fn a_crate_switched_on_joins_the_new_tracks_in_one_file() {
        let setup = setup(&["Night Drive", "Soft Focus"]);
        let tracks: Vec<i64> = setup
            .service
            .read_page(
                &oc_core_db::library::LibraryQuery::new(oc_core_db::library::View::Tracks),
                0,
                10,
            )
            .unwrap()
            .items
            .iter()
            .map(|item| item.id)
            .collect();
        let warm = setup.service.create_crate("Warm-up").unwrap();
        setup.service.add_to_crate(warm, tracks).unwrap();
        let listed = crates(&setup.service).unwrap();
        assert_eq!(listed.crates.len(), 1);
        assert!(!listed.crates[0].in_file);
        assert_eq!(listed.crates[0].new, 2);
        let after = set_crate(&setup.service, warm, true).unwrap();
        assert!(after.crates[0].in_file);
        assert_eq!(
            playlist_names(&read_back(&setup.service)),
            ["openCrate crates/Warm-up (2)"]
        );
        let ids: Vec<i64> = new_tracks(&setup.service)
            .unwrap()
            .tracks
            .iter()
            .map(|track| track.file_id)
            .collect();
        send(&setup.service, &ids[..1], 5).unwrap();
        let both = read_back(&setup.service);
        assert_eq!(
            playlist_names(&both),
            ["openCrate new tracks (1)", "openCrate crates/Warm-up (2)"]
        );
        assert_eq!(
            both.tracks.len(),
            2,
            "a track in two playlists is written once"
        );
        set_crate(&setup.service, warm, false).unwrap();
        assert_eq!(
            playlist_names(&read_back(&setup.service)),
            ["openCrate new tracks (1)"]
        );
        assert!(set_crate(&setup.service, 999, true).is_err());
    }

    #[test]
    fn a_hidden_track_leaves_the_file_and_comes_back() {
        let setup = setup(&["Keep", "Not For Rekordbox"]);
        let listed = new_tracks(&setup.service).unwrap();
        let ids: Vec<i64> = listed.tracks.iter().map(|track| track.file_id).collect();
        send(&setup.service, &ids, 1).unwrap();
        let hide = listed
            .tracks
            .iter()
            .find(|track| track.title == "Not For Rekordbox")
            .unwrap()
            .file_id;
        let after = set_hidden(&setup.service, &[hide], true, 2).unwrap();
        assert!(after
            .tracks
            .iter()
            .any(|track| track.file_id == hide && track.hidden));
        assert_eq!(
            playlist_names(&read_back(&setup.service)),
            ["openCrate new tracks (1)"]
        );
        assert!(
            send(&setup.service, &[hide], 3).is_err(),
            "a hidden track is not sent"
        );
        set_hidden(&setup.service, &[hide], false, 4).unwrap();
        assert_eq!(
            playlist_names(&read_back(&setup.service)),
            ["openCrate new tracks (2)"]
        );
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
        let id = new_tracks(&setup.service).unwrap().tracks[0].file_id;
        let sent = send(&setup.service, &[id, id], 1).unwrap();
        assert_eq!(sent.tracks, 1, "a track named twice is sent once");
        assert_eq!(read_back(&setup.service).tracks.len(), 1);
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
