use std::fs;
use std::path::{Path, PathBuf};

use rbl_db::fixture::{self, Shape};
use rbl_db::{Library, LibraryLocation, OpenMode};

use super::write::{
    add_to_playlist, check_path_form, create_folder, create_playlist, merge_duplicates, relocate,
    set_fields, Duplicate, FieldEdit, Member, Playlist, TrackText,
};
use super::*;

const NOW: i64 = 1_792_800_000_000;

struct World {
    _live: tempfile::TempDir,
    backups: tempfile::TempDir,
    music: tempfile::TempDir,
    location: LibraryLocation,
}

fn world() -> World {
    let live = tempfile::tempdir().unwrap();
    let location = fixture::build(live.path(), Shape::default()).unwrap();
    World {
        _live: live,
        backups: tempfile::tempdir().unwrap(),
        music: tempfile::tempdir().unwrap(),
        location,
    }
}

/// A short, silent WAV file: enough for rekordbox's import to read.
fn wav(path: &Path) -> PathBuf {
    let samples = 4_410u32;
    let data = samples * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&88_200u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    bytes.resize(bytes.len() + data as usize, 0);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    // rekordbox stores the path as the operating system gives it; resolve temp-folder links.
    dunce::canonicalize(path).unwrap()
}

/// A path as rekordbox stores it: forward slashes, on Windows too.
fn rekordbox_form(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn members_of(location: &LibraryLocation, playlist: &str) -> Vec<String> {
    let library = Library::open(location.clone(), OpenMode::ReadOnly).unwrap();
    let mut statement = library
        .connection()
        .prepare(
            "SELECT ContentID FROM djmdSongPlaylist
             WHERE PlaylistID = ?1 AND rb_local_deleted = 0 ORDER BY TrackNo",
        )
        .unwrap();
    statement
        .query_map([playlist], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn top_level_named(location: &LibraryLocation, name: &str) -> Vec<String> {
    let library = Library::open(location.clone(), OpenMode::ReadOnly).unwrap();
    let mut statement = library
        .connection()
        .prepare(
            "SELECT ID FROM djmdPlaylist WHERE ParentID = 'root' AND Name = ?1 AND rb_local_deleted = 0",
        )
        .unwrap();
    statement
        .query_map([name], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn track_count(location: &LibraryLocation) -> u32 {
    Library::open(location.clone(), OpenMode::ReadOnly)
        .unwrap()
        .live_track_count()
        .unwrap()
}

#[test]
fn tracks_rekordbox_has_are_added_to_a_playlist_after_a_backup() {
    let world = world();
    let playlist = fixture::playlist_id(0);
    let before = members_of(&world.location, &playlist);
    let wanted = [
        fixture::track_id(20),
        fixture::track_id(21),
        before[0].clone(),
    ];
    let done = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &Playlist::Id(playlist.clone()),
        &wanted
            .iter()
            .cloned()
            .map(Member::Known)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    // The one already there is not added twice.
    assert_eq!((done.added, done.imported), (2, 0));
    let after = members_of(&world.location, &playlist);
    assert_eq!(after.len(), before.len() + 2);
    assert_eq!(after[after.len() - 2..], wanted[..2]);
    // The backup came first: it holds the playlist as it was, and is listed for a restore.
    assert_eq!(done.backup.reason, "before-write");
    let saved = LibraryLocation {
        master_db: done.backup.master_db.clone(),
        ..world.location.clone()
    };
    assert_eq!(members_of(&saved, &playlist), before);
    assert_eq!(list_backups(world.backups.path()).len(), 1);
}

#[test]
fn new_files_are_added_to_rekordbox_and_to_imported_which_is_made_once() {
    let world = world();
    let first = wav(&world
        .music
        .path()
        .join("Studio 001/Between Spaces/01 - Night Drive.wav"));
    let second = wav(&world
        .music
        .path()
        .join("Studio 001/Between Spaces/02 - Soft Focus.wav"));
    let imported = Playlist::TopLevel("Imported".into());

    let done = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &imported,
        &[Member::File(first.clone())],
    )
    .unwrap();
    assert_eq!((done.added, done.imported), (1, 1));
    assert_eq!(track_count(&world.location), 41);
    let made = top_level_named(&world.location, "Imported");
    assert_eq!(made.len(), 1);

    // A second send appends to the same playlist; a file rekordbox now has is not added again.
    let done = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW + 1_000,
        &imported,
        &[Member::File(first), Member::File(second)],
    )
    .unwrap();
    assert_eq!((done.added, done.imported), (1, 1));
    assert_eq!(track_count(&world.location), 42);
    assert_eq!(top_level_named(&world.location, "Imported"), made);
    assert_eq!(members_of(&world.location, &made[0]).len(), 2);
}

fn parent_and_name(location: &LibraryLocation, playlist: &str) -> (String, String) {
    let library = Library::open(location.clone(), OpenMode::ReadOnly).unwrap();
    library
        .connection()
        .query_row(
            "SELECT ParentID, Name FROM djmdPlaylist WHERE ID = ?1",
            [playlist],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[test]
fn a_new_playlist_is_made_in_a_folder_and_a_taken_name_gets_a_number() {
    let world = world();
    let mut folder = String::new();
    prepare_tracks(&world.location, |writer| {
        folder = writer.create_folder("Sets", rbl_db::write::ROOT).unwrap();
    });
    let new = Playlist::New {
        name: "Warm-up".into(),
        parent: folder.clone(),
    };
    let members = [
        Member::Known(fixture::track_id(2)),
        Member::Known(fixture::track_id(1)),
    ];
    let done = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &new,
        &members,
    )
    .unwrap();
    assert_eq!((done.added, done.name.as_str()), (2, "Warm-up"));
    assert_eq!(
        parent_and_name(&world.location, &done.playlist),
        (folder.clone(), "Warm-up".to_owned())
    );
    assert_eq!(
        members_of(&world.location, &done.playlist),
        vec![fixture::track_id(2), fixture::track_id(1)]
    );

    // The same again: a second playlist, beside the first, not added to it.
    let again = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW + 1_000,
        &new,
        &members,
    )
    .unwrap();
    assert_ne!(again.playlist, done.playlist);
    assert_eq!(again.name, "Warm-up (2)");
    assert_eq!(members_of(&world.location, &done.playlist).len(), 2);

    // A playlist is not a folder to make one in.
    let error = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW + 2_000,
        &Playlist::New {
            name: "Warm-up".into(),
            parent: done.playlist.clone(),
        },
        &members,
    )
    .unwrap_err();
    assert!(error.to_string().contains("not a folder"), "{error}");
}

#[test]
fn a_folder_is_made_after_a_backup_inside_another_and_a_taken_name_gets_a_number() {
    let world = world();
    let root = super::write::ROOT;
    let error = create_folder(
        &world.location,
        world.backups.path(),
        true,
        NOW,
        "Sets",
        root,
    )
    .unwrap_err();
    assert!(error.to_string().contains("running"), "{error}");
    assert_eq!(list_backups(world.backups.path()).len(), 0);

    let sets = create_folder(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        "Sets",
        root,
    )
    .unwrap();
    assert_eq!(sets.name, "Sets");
    assert_eq!(
        parent_and_name(&world.location, &sets.id),
        (root.to_owned(), "Sets".to_owned())
    );
    assert_eq!(list_backups(world.backups.path()).len(), 1);
    let inner = create_folder(
        &world.location,
        world.backups.path(),
        false,
        NOW + 1_000,
        "Friday",
        &sets.id,
    )
    .unwrap();
    assert_eq!(
        parent_and_name(&world.location, &inner.id),
        (sets.id.clone(), "Friday".to_owned())
    );
    let again = create_folder(
        &world.location,
        world.backups.path(),
        false,
        NOW + 2_000,
        "Sets",
        root,
    )
    .unwrap();
    assert_eq!(again.name, "Sets (2)");

    // A playlist cannot hold a folder.
    let playlist = top_level_named(&world.location, "Playlist 1")[0].clone();
    let error = create_folder(
        &world.location,
        world.backups.path(),
        false,
        NOW + 3_000,
        "Nope",
        &playlist,
    )
    .unwrap_err();
    assert!(error.to_string().contains("not a folder"), "{error}");
}

#[test]
fn an_empty_playlist_is_made_in_a_folder_and_a_taken_name_gets_a_number() {
    let world = world();
    let root = super::write::ROOT;
    let sets = create_folder(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        "Sets",
        root,
    )
    .unwrap();
    let friday = create_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW + 1_000,
        "Friday",
        &sets.id,
    )
    .unwrap();
    assert_eq!(
        parent_and_name(&world.location, &friday.id),
        (sets.id.clone(), "Friday".to_owned())
    );
    let again = create_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW + 2_000,
        "Friday",
        &sets.id,
    )
    .unwrap();
    assert_eq!(again.name, "Friday (2)");
    assert_eq!(list_backups(world.backups.path()).len(), 3);
    // A playlist goes in a folder, never in another playlist, and never while rekordbox runs.
    for (running, parent) in [(false, friday.id.as_str()), (true, root)] {
        assert!(create_playlist(
            &world.location,
            world.backups.path(),
            running,
            NOW + 3_000,
            "Nope",
            parent
        )
        .is_err());
    }
}

#[test]
fn a_running_rekordbox_is_refused_before_anything_is_copied_or_written() {
    let world = world();
    let error = add_to_playlist(
        &world.location,
        world.backups.path(),
        true,
        NOW,
        &Playlist::TopLevel("Imported".into()),
        &[Member::Known(fixture::track_id(1))],
    )
    .unwrap_err();
    assert!(error.to_string().contains("running"), "{error}");
    assert_eq!(list_backups(world.backups.path()).len(), 0);
    assert_eq!(top_level_named(&world.location, "Imported").len(), 0);
}

#[test]
fn an_unknown_playlist_or_track_is_refused_and_nothing_is_written() {
    let world = world();
    let error = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &Playlist::Id("999999".into()),
        &[Member::Known(fixture::track_id(1))],
    )
    .unwrap_err();
    assert!(error.to_string().contains("playlist"), "{error}");

    let playlist = fixture::playlist_id(1);
    let before = members_of(&world.location, &playlist);
    let error = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &Playlist::Id(playlist.clone()),
        &[
            Member::Known(fixture::track_id(2)),
            Member::Known("123456789".into()),
        ],
    )
    .unwrap_err();
    assert!(error.to_string().contains("123456789"), "{error}");
    assert_eq!(members_of(&world.location, &playlist), before);
}

#[test]
fn a_file_rekordbox_cannot_read_is_reported_and_the_rest_still_go_in() {
    let world = world();
    let good = wav(&world.music.path().join("good.wav"));
    let bad = world.music.path().join("not-audio.wav");
    fs::write(&bad, b"not audio at all").unwrap();
    let done = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &Playlist::TopLevel("Imported".into()),
        &[Member::File(bad.clone()), Member::File(good)],
    )
    .unwrap();
    assert_eq!((done.added, done.imported), (1, 1));
    assert_eq!(done.refused.len(), 1);
    assert_eq!(done.refused[0].0, bad);
}

#[test]
fn no_new_playlist_is_made_when_rekordbox_takes_none_of_the_files() {
    let world = world();
    let bad = world.music.path().join("not-audio.wav");
    fs::write(&bad, b"not audio at all").unwrap();
    let error = add_to_playlist(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &Playlist::New {
            name: "Warm-up".into(),
            parent: super::write::ROOT.into(),
        },
        &[Member::File(bad)],
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("no playlist was made"),
        "{error}"
    );
    assert_eq!(top_level_named(&world.location, "Warm-up").len(), 0);
}

#[test]
fn paths_stored_with_backslashes_are_not_written_to() {
    assert!(check_path_form(["C:/Music/a.mp3", "D:/b.flac"]).is_ok());
    assert!(check_path_form(Vec::<&str>::new()).is_ok());
    let error = check_path_form(["C:/Music/a.mp3", r"C:\Music\b.mp3"]).unwrap_err();
    assert!(error.to_string().contains("backslash"), "{error}");
}

fn read_back(location: &LibraryLocation, backups: &Path, content: &str) -> crate::xml::Attributes {
    let snapshot = take_snapshot(location, backups, "check", false, NOW + 9_000).unwrap();
    read_snapshot(&snapshot.master_db, &location.passphrase)
        .unwrap()
        .tracks
        .into_iter()
        .find(|track| track.id.to_string() == content)
        .unwrap()
        .attributes
}

#[test]
fn track_fields_are_written_after_a_backup_and_read_back() {
    let world = world();
    let content = fixture::track_id(3);
    let edit = |field, value: &str| FieldEdit {
        content: content.clone(),
        field,
        value: value.to_owned(),
        expected: None,
    };
    let backup = set_fields(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &[
            edit(TrackText::Title, "Night Drive (Extended)"),
            edit(TrackText::Artist, "Studio 001"),
            edit(TrackText::Album, "Between Spaces"),
            edit(TrackText::Year, "2021"),
            edit(TrackText::Label, "Test Label"),
        ],
    )
    .unwrap();
    assert_eq!(backup.reason, "before-write");
    let attributes = read_back(&world.location, world.backups.path(), &content);
    let get = |key: &str| attributes.get(key).cloned().unwrap_or_default();
    assert_eq!(get("Name"), "Night Drive (Extended)");
    assert_eq!(get("Artist"), "Studio 001");
    assert_eq!(get("Album"), "Between Spaces");
    assert_eq!(get("Year"), "2021");
    assert_eq!(get("Label"), "Test Label");

    // An emptied field is cleared.
    set_fields(
        &world.location,
        world.backups.path(),
        false,
        NOW + 1_000,
        &[edit(TrackText::Label, ""), edit(TrackText::Year, "")],
    )
    .unwrap();
    let attributes = read_back(&world.location, world.backups.path(), &content);
    assert_eq!(attributes.get("Label"), None);
    assert_eq!(attributes.get("Year"), None);
}

#[test]
fn field_edits_are_refused_while_rekordbox_runs_or_for_a_track_it_lacks() {
    let world = world();
    let edit = |content: String| FieldEdit {
        content,
        field: TrackText::Title,
        value: "X".into(),
        expected: None,
    };
    let error = set_fields(
        &world.location,
        world.backups.path(),
        true,
        NOW,
        &[edit(fixture::track_id(1))],
    )
    .unwrap_err();
    assert!(error.to_string().contains("running"), "{error}");
    let error = set_fields(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &[edit("424242".into())],
    )
    .unwrap_err();
    assert!(error.to_string().contains("424242"), "{error}");
    assert_eq!(list_backups(world.backups.path()).len(), 0);
}

fn folder_path(location: &LibraryLocation, content: &str) -> String {
    Library::open(location.clone(), OpenMode::ReadOnly)
        .unwrap()
        .connection()
        .query_row(
            "SELECT FolderPath FROM djmdContent WHERE ID = ?1",
            [content],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn moved_files_are_relocated_and_keep_their_playlists() {
    let world = world();
    let content = fixture::track_id(0);
    let playlist = fixture::playlist_id(0);
    let before = members_of(&world.location, &playlist);
    assert!(before.contains(&content));
    let moved = wav(&world.music.path().join("Studio 001/01 - Soft Focus.wav"));
    let backup = relocate(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &[(content.clone(), moved.clone())],
    )
    .unwrap();
    assert_eq!(backup.reason, "before-write");
    assert_eq!(
        folder_path(&world.location, &content),
        rekordbox_form(&moved)
    );
    assert_eq!(
        members_of(&world.location, &playlist),
        before,
        "playlists keep the track"
    );
}

#[test]
fn a_relocation_to_a_missing_file_is_refused_and_rekordbox_is_put_back() {
    let world = world();
    let content = fixture::track_id(0);
    let was = folder_path(&world.location, &content);
    let real = wav(&world.music.path().join("a.wav"));
    let missing = world.music.path().join("nowhere.wav");
    let error = relocate(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &[(fixture::track_id(1), real), (content.clone(), missing)],
    )
    .unwrap_err();
    assert!(error.to_string().contains("nowhere.wav"), "{error}");
    assert_eq!(folder_path(&world.location, &content), was);
    assert_ne!(
        folder_path(&world.location, &fixture::track_id(1)),
        rekordbox_form(&dunce::canonicalize(world.music.path().join("a.wav")).unwrap()),
        "the one that worked is undone too"
    );
}

/// Give tracks My Tags, a rating and cues the way rekordbox would.
fn prepare_tracks(location: &LibraryLocation, edit: impl FnOnce(&mut rbl_db::write::Writer)) {
    let backups = tempfile::tempdir().unwrap();
    let mut writer = rbl_db::write::Writer::open(location.clone(), backups.path()).unwrap();
    writer.disable_automatic_backups();
    edit(&mut writer);
}

fn live(location: &LibraryLocation, content: &str) -> bool {
    Library::open(location.clone(), OpenMode::ReadOnly)
        .unwrap()
        .connection()
        .query_row(
            "SELECT rb_local_deleted = 0 FROM djmdContent WHERE ID = ?1",
            [content],
            |row| row.get(0),
        )
        .unwrap()
}

fn my_tags(location: &LibraryLocation, content: &str) -> Vec<String> {
    rbl_db::details::my_tag_names(
        Library::open(location.clone(), OpenMode::ReadOnly)
            .unwrap()
            .connection(),
        content,
    )
}

fn rating(location: &LibraryLocation, content: &str) -> i64 {
    Library::open(location.clone(), OpenMode::ReadOnly)
        .unwrap()
        .connection()
        .query_row(
            "SELECT COALESCE(Rating, 0) FROM djmdContent WHERE ID = ?1",
            [content],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_duplicate_gives_its_playlist_places_tags_and_rating_to_the_copy_kept() {
    let world = world();
    let (kept, dropped) = (fixture::track_id(20), fixture::track_id(2));
    let playlist = fixture::playlist_id(0);
    prepare_tracks(&world.location, |writer| {
        writer
            .set_my_tags(&dropped, &[fixture::MY_TAG_PEAK.to_owned()])
            .unwrap();
        writer
            .set_my_tags(&kept, &[fixture::MY_TAG_WARM_UP.to_owned()])
            .unwrap();
        writer.set_rating(&dropped, 4).unwrap();
    });
    let before = members_of(&world.location, &playlist);
    let position = before.iter().position(|id| *id == dropped).unwrap();
    let backup = merge_duplicates(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &[Duplicate {
            kept: kept.clone(),
            dropped: dropped.clone(),
        }],
    )
    .unwrap();
    assert_eq!(backup.reason, "before-write");
    let after = members_of(&world.location, &playlist);
    assert_eq!(after.len(), before.len());
    assert_eq!(after[position], kept, "the kept copy takes the same place");
    assert!(!after.contains(&dropped));
    assert!(
        !live(&world.location, &dropped),
        "the dropped entry is removed"
    );
    assert!(live(&world.location, &kept));
    let mut tags = my_tags(&world.location, &kept);
    tags.sort();
    assert_eq!(tags, ["Peak", "Warm-up"]);
    assert_eq!(
        rating(&world.location, &kept),
        4,
        "an unrated copy takes the rating"
    );
}

#[test]
fn a_copy_already_in_the_playlist_is_not_put_in_twice() {
    let world = world();
    let playlist = fixture::playlist_id(0);
    let before = members_of(&world.location, &playlist);
    let (kept, dropped) = (before[0].clone(), before[1].clone());
    merge_duplicates(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &[Duplicate {
            kept: kept.clone(),
            dropped,
        }],
    )
    .unwrap();
    let after = members_of(&world.location, &playlist);
    assert_eq!(after.len(), before.len() - 1);
    assert_eq!(after.iter().filter(|id| **id == kept).count(), 1);
}

#[test]
fn cues_only_the_dropped_copy_has_stop_the_merge_before_anything_is_written() {
    let world = world();
    let (kept, dropped) = (fixture::track_id(20), fixture::track_id(2));
    prepare_tracks(&world.location, |writer| {
        writer.add_cue(&dropped, 1, 12_000).unwrap();
    });
    let error = merge_duplicates(
        &world.location,
        world.backups.path(),
        false,
        NOW,
        &[Duplicate {
            kept,
            dropped: dropped.clone(),
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("cues"), "{error}");
    assert!(live(&world.location, &dropped));
    assert_eq!(list_backups(world.backups.path()).len(), 0);
    let error = merge_duplicates(
        &world.location,
        world.backups.path(),
        true,
        NOW,
        &[Duplicate {
            kept: fixture::track_id(21),
            dropped: fixture::track_id(22),
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("running"), "{error}");
}

/// Every kind of write, on a copy of the installed library (never the library itself), read back
/// with openCrate's own reader. Prints aggregates only.
/// `cargo test -p oc-rekordbox -- --ignored --nocapture live_copy_takes_every_write`
#[test]
#[ignore = "copies the installed rekordbox library; run by hand"]
fn live_copy_takes_every_write() {
    assert!(!rekordbox_running(), "quit rekordbox and its agent first");
    let installed = installed_library().unwrap();
    let copies = tempfile::tempdir().unwrap();
    let snapshot = take_snapshot(&installed, copies.path(), "copy", false, NOW).unwrap();
    let live_bytes = fs::read(&installed.master_db).unwrap();
    // The copy is a different file rekordbox has never heard of.
    let location = LibraryLocation {
        master_db: snapshot.master_db.clone(),
        is_real_install: false,
        ..installed.clone()
    };
    let backups = tempfile::tempdir().unwrap();
    let music = tempfile::tempdir().unwrap();
    let file = wav(&music.path().join("openCrate test/01 - Copy Check.wav"));
    let before = track_count(&location);

    let done = add_to_playlist(
        &location,
        backups.path(),
        false,
        NOW,
        &Playlist::TopLevel("openCrate copy check".into()),
        &[Member::File(file.clone())],
    )
    .unwrap();
    assert_eq!((done.added, done.imported), (1, 1));
    assert_eq!(track_count(&location), before + 1);
    let content = members_of(&location, &done.playlist)[0].clone();
    set_fields(
        &location,
        backups.path(),
        false,
        NOW + 1_000,
        &[FieldEdit {
            content: content.clone(),
            field: TrackText::Title,
            value: "Copy Check (Edited)".into(),
            expected: None,
        }],
    )
    .unwrap();
    let attributes = read_back(&location, backups.path(), &content);
    assert_eq!(
        attributes.get("Name").map(String::as_str),
        Some("Copy Check (Edited)")
    );
    // The file moves and rekordbox follows it, keeping the playlist place.
    let moved = music
        .path()
        .join("openCrate test/moved/01 - Copy Check.wav");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(&file, &moved).unwrap();
    relocate(
        &location,
        backups.path(),
        false,
        NOW + 2_000,
        &[(content.clone(), moved.clone())],
    )
    .unwrap();
    assert_eq!(folder_path(&location, &content), rekordbox_form(&moved));
    assert_eq!(
        members_of(&location, &done.playlist),
        std::slice::from_ref(&content)
    );
    // A second copy is imported, put in the playlist, then merged into the first.
    let second = wav(&music.path().join("openCrate test/02 - Copy Check.wav"));
    let done = add_to_playlist(
        &location,
        backups.path(),
        false,
        NOW + 3_000,
        &Playlist::Id(done.playlist.clone()),
        &[Member::File(second)],
    )
    .unwrap();
    assert_eq!((done.added, done.imported), (1, 1));
    let dropped = members_of(&location, &done.playlist)[1].clone();
    merge_duplicates(
        &location,
        backups.path(),
        false,
        NOW + 4_000,
        &[Duplicate {
            kept: content.clone(),
            dropped: dropped.clone(),
        }],
    )
    .unwrap();
    assert_eq!(
        members_of(&location, &done.playlist),
        std::slice::from_ref(&content)
    );
    assert!(!live(&location, &dropped));
    assert_eq!(
        fs::read(&installed.master_db).unwrap(),
        live_bytes,
        "the installed library is untouched"
    );
    eprintln!(
        "rekordbox {:?}: copy took a playlist, an imported file and a title edit; {before} -> {} tracks",
        rekordbox_app_version(),
        track_count(&location)
    );
}

/// Writes to the installed library itself, for checking in rekordbox by eye: a playlist named
/// "openCrate check" holding a new synthetic track, retitled. Only on a test rekordbox. Backups go
/// to `OPENCRATE_CHECK_BACKUPS`, from which the library can be restored afterwards.
/// `OPENCRATE_CHECK_BACKUPS=... cargo test -p oc-rekordbox -- --ignored --nocapture live_installed_write`
#[test]
#[ignore = "writes to the installed rekordbox library; run by hand on a test install only"]
fn live_installed_write() {
    assert!(!rekordbox_running(), "quit rekordbox and its agent first");
    let backups = PathBuf::from(
        std::env::var("OPENCRATE_CHECK_BACKUPS").expect("set OPENCRATE_CHECK_BACKUPS"),
    );
    let installed = installed_library().unwrap();
    let audio = backups.join("audio");
    let file = wav(&audio.join("openCrate Check/01 - openCrate Check.wav"));
    let before = track_count(&installed);
    let done = add_to_playlist(
        &installed,
        &backups,
        false,
        NOW,
        &Playlist::TopLevel("openCrate check".into()),
        &[Member::File(file)],
    )
    .unwrap();
    let content = members_of(&installed, &done.playlist)[0].clone();
    set_fields(
        &installed,
        &backups,
        false,
        NOW + 1_000,
        &[FieldEdit {
            content,
            field: TrackText::Title,
            value: "openCrate Check (Edited)".into(),
            expected: None,
        }],
    )
    .unwrap();
    eprintln!(
        "wrote to the installed library: {before} -> {} tracks; backup before the first write: {}",
        track_count(&installed),
        done.backup.dir.display()
    );
}

/// Puts a backup back into the installed library, with openCrate's own restore.
/// `OPENCRATE_CHECK_BACKUPS=... OPENCRATE_CHECK_RESTORE=<backup folder> cargo test -p oc-rekordbox -- --ignored --nocapture live_installed_restore`
#[test]
#[ignore = "restores the installed rekordbox library; run by hand on a test install only"]
fn live_installed_restore() {
    let backups = PathBuf::from(
        std::env::var("OPENCRATE_CHECK_BACKUPS").expect("set OPENCRATE_CHECK_BACKUPS"),
    );
    let backup = PathBuf::from(
        std::env::var("OPENCRATE_CHECK_RESTORE").expect("set OPENCRATE_CHECK_RESTORE"),
    );
    let installed = installed_library().unwrap();
    let saved = restore(
        &backup,
        &installed,
        &backups,
        rekordbox_running(),
        NOW + 5_000,
    )
    .unwrap();
    eprintln!(
        "restored; {} tracks now; the library it replaced is kept at {}",
        track_count(&installed),
        saved.dir.display()
    );
}
