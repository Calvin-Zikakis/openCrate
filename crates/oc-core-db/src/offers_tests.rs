use std::fs;
use std::path::{Path, PathBuf};

use oc_rekordbox::xml::{self, write_offer, NewTrack};
use oc_tag::fixtures::{aiff, write_tags};
use oc_tag::TagInfo;
use tempfile::{tempdir, TempDir};

use crate::service::{LibraryService, ScanOptions};

struct Library {
    _data: TempDir,
    folder: TempDir,
    staging: PathBuf,
    service: LibraryService,
    root: i64,
}

fn library() -> Library {
    let folder = tempdir().unwrap();
    let music = folder.path().join("Music");
    let staging = folder.path().join("staging");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    let root = service.add_root(&music, "Music").unwrap();
    Library {
        _data: data,
        folder,
        staging,
        service,
        root,
    }
}

fn tagged(path: &Path, title: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, aiff(2)).unwrap();
    write_tags(
        path,
        &TagInfo {
            artist: Some("Studio 001".into()),
            title: Some(title.into()),
            album: Some("Between Spaces".into()),
            ..TagInfo::default()
        },
    );
}

impl Library {
    /// Resolved, as the library stores its roots; imports file under that form.
    fn music(&self) -> PathBuf {
        self.folder.path().join("Music").canonicalize().unwrap()
    }

    /// Put a file in the library the way an import does: staged, then published.
    fn file(&self, relative: &str, title: &str) -> PathBuf {
        let staged = self.staging.join(format!("{title}.aif"));
        tagged(&staged, title);
        let destination = self.music().join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        self.service
            .publish_file(staged, destination.clone())
            .unwrap();
        destination
    }

    fn scan(&self) {
        self.service
            .scan_root(self.root, &ScanOptions::default())
            .unwrap();
    }

    fn titles(&self) -> Vec<String> {
        let (found, _) = self.service.rekordbox_candidates().unwrap();
        found.into_iter().map(|candidate| candidate.title).collect()
    }
}

fn rekordbox_has(paths: &[&Path]) -> xml::Collection {
    let tracks: Vec<NewTrack> = paths
        .iter()
        .map(|path| NewTrack {
            path: path.as_os_str().as_encoded_bytes().to_vec(),
            name: String::new(),
            artist: String::new(),
            album: String::new(),
            year: None,
            total_seconds: 0,
            size: 0,
            kind: "AIFF".into(),
        })
        .collect();
    xml::read(std::io::Cursor::new(write_offer(
        &tracks,
        "Collection",
        "1",
    )))
    .unwrap()
}

#[test]
fn a_filed_file_is_a_candidate_until_rekordbox_has_it() {
    let library = library();
    tagged(
        &library.music().join("Old/Already There.aif"),
        "Already There",
    );
    let filed = library.file(
        "Studio 001/Between Spaces/01 - Night Drive.aif",
        "Night Drive",
    );
    library.scan();
    let (found, read_ms) = library.service.rekordbox_candidates().unwrap();
    assert_eq!(read_ms, None);
    assert_eq!(found.len(), 1);
    let candidate = &found[0];
    assert_eq!(candidate.title, "Night Drive");
    assert_eq!(candidate.artist, "Studio 001");
    assert_eq!(candidate.album, "Between Spaces");
    assert_eq!(candidate.container, "AIFF");
    assert_eq!(candidate.path, filed.as_os_str().as_encoded_bytes());
    assert!(candidate.size > 0 && candidate.filed_ms > 0);
    assert_eq!(candidate.offered_ms, None);

    library
        .service
        .import_database(Path::new("/rekordbox/master.db"), rekordbox_has(&[&filed]))
        .unwrap();
    let (found, read_ms) = library.service.rekordbox_candidates().unwrap();
    assert!(found.is_empty(), "rekordbox has it now");
    assert!(read_ms.is_some());
}

#[test]
fn deleted_and_unfiled_files_are_not_candidates() {
    let library = library();
    let gone = library.file("A/01 - Gone.aif", "Gone");
    library.file("A/02 - Kept.aif", "Kept");
    library.scan();
    assert_eq!(library.titles().len(), 2);
    fs::remove_file(&gone).unwrap();
    library.scan();
    assert_eq!(library.titles(), ["Kept"]);
}

#[test]
fn offers_are_remembered_and_cleared_with_the_file() {
    let library = library();
    library.file("A/01 - Sent.aif", "Sent");
    library.file("A/02 - Not Sent.aif", "Not Sent");
    library.scan();
    let (found, _) = library.service.rekordbox_candidates().unwrap();
    let sent = found.iter().find(|c| c.title == "Sent").unwrap().file_id;
    library.service.record_offers(vec![sent], 1234).unwrap();
    library.service.record_offers(vec![sent], 5678).unwrap();
    let (found, _) = library.service.rekordbox_candidates().unwrap();
    let offered: Vec<(String, Option<i64>)> =
        found.into_iter().map(|c| (c.title, c.offered_ms)).collect();
    assert!(offered.contains(&("Sent".into(), Some(5678))));
    assert!(offered.contains(&("Not Sent".into(), None)));
    library.service.remove_root(library.root).unwrap();
    assert_eq!(library.service.offer_count().unwrap(), 0);
}

fn track_ids(library: &Library) -> std::collections::HashMap<String, i64> {
    library
        .service
        .read_page(
            &crate::library::LibraryQuery::new(crate::library::View::Tracks),
            0,
            50,
        )
        .unwrap()
        .items
        .into_iter()
        .map(|item| (item.title, item.id))
        .collect()
}

/// An XML export, as rekordbox would write it, holding `path` with one cue and one beat.
fn export_with(path: &Path, folder: &Path) -> PathBuf {
    let location = oc_rekordbox::location::encode_location(path.as_os_str().as_encoded_bytes());
    let text = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0"><PRODUCT Name="rekordbox" Version="7.2.19" Company="AlphaTheta"/>
<COLLECTION Entries="1">
<TRACK TrackID="77" Name="Night Drive" Artist="Studio 001" Rating="204" Colour="0xFF007F" Location="{location}">
<TEMPO Inizio="0.025" Bpm="124.00" Metro="4/4" Battito="1"/>
<POSITION_MARK Name="Drop" Type="0" Start="64.000" Num="1" Red="40" Green="226" Blue="20"/>
</TRACK></COLLECTION><PLAYLISTS><NODE Type="0" Name="ROOT" Count="0"/></PLAYLISTS></DJ_PLAYLISTS>"#
    );
    let file = folder.join("export.xml");
    fs::write(&file, text).unwrap();
    file
}

#[test]
fn a_crate_echoes_what_rekordbox_has_and_offers_the_rest() {
    let library = library();
    let night = library.file("A/01 - Night Drive.aif", "Night Drive");
    library.file("A/02 - Soft Focus.aif", "Soft Focus");
    library.file("A/03 - Late Train.aif", "Late Train");
    library.scan();
    library
        .service
        .import_export(&export_with(&night, library.folder.path()))
        .unwrap();
    let ids = track_ids(&library);
    let warm = library.service.create_crate("Warm-up").unwrap();
    library
        .service
        .add_to_crate(
            warm,
            vec![ids["Soft Focus"], ids["Night Drive"], ids["Late Train"]],
        )
        .unwrap();
    let (name, tracks) = library.service.crate_for_rekordbox(warm).unwrap();
    assert_eq!(name, "Warm-up");
    assert_eq!(tracks.len(), 3);
    let crate::offers::CrateTrack::New(soft) = &tracks[0] else {
        panic!("{:?}", tracks[0])
    };
    assert_eq!(soft.name, "Soft Focus");
    assert!(soft.path.ends_with(b"02 - Soft Focus.aif"));
    let crate::offers::CrateTrack::Echo(echo) = &tracks[1] else {
        panic!("{:?}", tracks[1])
    };
    assert_eq!(echo.attributes["Rating"], "204");
    assert_eq!(echo.attributes["Colour"], "0xFF007F");
    assert_eq!(echo.location, echo.attributes["Location"]);
    assert_eq!(echo.cues.len(), 1);
    assert_eq!(echo.cues[0].attributes["Name"], "Drop");
    assert_eq!(echo.tempos.len(), 1);
    assert!(matches!(&tracks[2], crate::offers::CrateTrack::New(_)));
}

#[test]
fn a_crate_from_a_database_read_or_without_files_says_so() {
    let library = library();
    let night = library.file("A/01 - Night Drive.aif", "Night Drive");
    let gone = library.file("A/02 - Gone.aif", "Gone");
    library.scan();
    library
        .service
        .import_database(Path::new("/rekordbox/master.db"), rekordbox_has(&[&night]))
        .unwrap();
    let ids = track_ids(&library);
    let warm = library.service.create_crate("Warm-up").unwrap();
    library
        .service
        .add_to_crate(warm, vec![ids["Night Drive"], ids["Gone"]])
        .unwrap();
    fs::remove_file(&gone).unwrap();
    library.scan();
    let (_, tracks) = library.service.crate_for_rekordbox(warm).unwrap();
    assert!(
        matches!(&tracks[0], crate::offers::CrateTrack::DatabaseOnly { title } if title == "Night Drive")
    );
    assert!(
        matches!(&tracks[1], crate::offers::CrateTrack::NoFile { title } if title == "Gone"),
        "{:?}",
        tracks[1]
    );
    assert!(library.service.crate_for_rekordbox(999).is_err());
}

#[test]
fn a_hidden_track_stays_listed_as_hidden_until_brought_back() {
    let library = library();
    library.file("A/01 - Keep.aif", "Keep");
    library.file("A/02 - Not For Rekordbox.aif", "Not For Rekordbox");
    library.scan();
    let (found, _) = library.service.rekordbox_candidates().unwrap();
    let hide = found
        .iter()
        .find(|c| c.title == "Not For Rekordbox")
        .unwrap()
        .file_id;
    library
        .service
        .set_rekordbox_hidden(vec![hide], true, 10)
        .unwrap();
    let (found, _) = library.service.rekordbox_candidates().unwrap();
    let hidden: Vec<(String, bool)> = found.into_iter().map(|c| (c.title, c.hidden)).collect();
    assert!(hidden.contains(&("Not For Rekordbox".into(), true)));
    assert!(hidden.contains(&("Keep".into(), false)));
    library
        .service
        .set_rekordbox_hidden(vec![hide], false, 11)
        .unwrap();
    let (found, _) = library.service.rekordbox_candidates().unwrap();
    assert!(found.iter().all(|c| !c.hidden));
}

#[test]
fn an_offer_and_its_setting_are_written_together() {
    let library = library();
    library.file("A/01 - Sent.aif", "Sent");
    library.scan();
    let (found, _) = library.service.rekordbox_candidates().unwrap();
    let sent = found[0].file_id;
    library
        .service
        .record_offers_with_setting("rekordbox_xml", "{\"new\":[1]}".into(), vec![sent], 77)
        .unwrap();
    assert_eq!(
        library.service.setting("rekordbox_xml").unwrap().as_deref(),
        Some("{\"new\":[1]}")
    );
    let (found, _) = library.service.rekordbox_candidates().unwrap();
    assert_eq!(found[0].offered_ms, Some(77));
    // A file that is not in the library makes the whole write fail: the setting stays as it was.
    assert!(library
        .service
        .record_offers_with_setting("rekordbox_xml", "{\"new\":[2]}".into(), vec![999_999], 78)
        .is_err());
    assert_eq!(
        library.service.setting("rekordbox_xml").unwrap().as_deref(),
        Some("{\"new\":[1]}")
    );
}

/// A database read in which rekordbox has `night` as track 777, in playlist 900123 inside folder
/// 950000.
fn database_with(night: &Path) -> xml::Collection {
    let mut collection = rekordbox_has(&[night]);
    collection.tracks[0].id = 777;
    let named = |name: &str, kind: &str, id: &str| {
        xml::Attributes::from([
            ("Name".to_owned(), name.to_owned()),
            ("Type".to_owned(), kind.to_owned()),
            (oc_rekordbox::db::REKORDBOX_ID.to_owned(), id.to_owned()),
        ])
    };
    collection.playlists = vec![xml::PlaylistNode::Folder {
        attributes: named("ROOT", "0", "root"),
        children: vec![xml::PlaylistNode::Folder {
            attributes: named("House", "0", "950000"),
            children: vec![xml::PlaylistNode::Playlist {
                attributes: named("Deep", "1", "900123"),
                track_ids: vec![777],
            }],
        }],
    }];
    collection.unresolved_track_ids.clear();
    collection
}

fn playlist_named(library: &Library, name: &str) -> i64 {
    library
        .service
        .playlists()
        .unwrap()
        .into_iter()
        .find(|row| row.name == name)
        .unwrap()
        .id
}

#[test]
fn a_playlist_from_a_database_read_knows_its_rekordbox_id() {
    let library = library();
    let night = library.file("A/01 - Night Drive.aif", "Night Drive");
    library.scan();
    library
        .service
        .import_database(Path::new("/rekordbox/master.db"), database_with(&night))
        .unwrap();
    let deep = playlist_named(&library, "Deep");
    assert_eq!(
        library.service.rekordbox_playlist_key(deep).unwrap(),
        Some("900123".to_owned())
    );
    let house = playlist_named(&library, "House");
    assert_eq!(
        library.service.rekordbox_playlist_key(house).unwrap(),
        Some("950000".to_owned())
    );
}

#[test]
fn a_playlist_from_an_xml_export_has_no_rekordbox_id() {
    let library = library();
    let night = library.file("A/01 - Night Drive.aif", "Night Drive");
    library.scan();
    let track = NewTrack {
        path: night.as_os_str().as_encoded_bytes().to_vec(),
        name: "Night Drive".into(),
        artist: String::new(),
        album: String::new(),
        year: None,
        total_seconds: 0,
        size: 0,
        kind: "AIFF".into(),
    };
    // Even an export carrying the attribute: only a database read names rekordbox's rows.
    let text = write_offer(&[track], "Deep", "1")
        .replace("Name=\"Deep\"", "Name=\"Deep\" RekordboxId=\"900123\"");
    let export = library.folder.path().join("rekordbox.xml");
    fs::write(&export, text).unwrap();
    library.service.import_export(&export).unwrap();
    let deep = playlist_named(&library, "Deep");
    assert_eq!(library.service.rekordbox_playlist_key(deep).unwrap(), None);
}

#[test]
fn tracks_map_to_their_rekordbox_row_or_to_the_file_to_add() {
    let library = library();
    let night = library.file("A/01 - Night Drive.aif", "Night Drive");
    let soft = library.file("A/02 - Soft Focus.aif", "Soft Focus");
    library.scan();
    library
        .service
        .import_database(Path::new("/rekordbox/master.db"), database_with(&night))
        .unwrap();
    let ids = track_ids(&library);
    let members = library
        .service
        .rekordbox_members(&[ids["Night Drive"], ids["Soft Focus"]])
        .unwrap();
    assert_eq!(
        members,
        [
            crate::offers::RekordboxMember::Known("777".into()),
            crate::offers::RekordboxMember::File(soft),
        ]
    );
}
