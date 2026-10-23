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
