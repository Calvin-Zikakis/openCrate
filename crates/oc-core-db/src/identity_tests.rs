//! Roots, scan reconciliation and Location matching, with the fixtures the path identity
//! contract calls for. All data is synthetic; paths are inert byte strings.

use std::collections::BTreeSet;
use std::path::Path;

use oc_paths::{Availability, Flavor, LocationPath, MatchKind, MatchStatus, NameProfile, RelPath};
use tempfile::{tempdir, TempDir};

use crate::matching::Matcher;
use crate::roots::{list_roots, NewRoot, ProfileSource, RootKind};
use crate::scan::{DirPath, ObservedFile, ScanPass};
use crate::{Database, Error};

fn database() -> (TempDir, Database) {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    (directory, database)
}

fn add_root(database: &mut Database, path: &str, profile: NameProfile) -> i64 {
    database
        .add_root(
            &NewRoot {
                path: Path::new(path),
                label: "Test",
                kind: RootKind::Local,
                profile,
                profile_source: ProfileSource::UserSet,
                dir_token: None,
                volume_token: None,
            },
            1_000,
        )
        .unwrap()
}

fn rel(parts: &[&str]) -> RelPath {
    RelPath::from_components(parts.iter().map(|part| part.as_bytes()), Flavor::Posix).unwrap()
}

fn dir(parts: &[&str]) -> DirPath {
    parts.iter().map(|part| part.as_bytes().to_vec()).collect()
}

fn observed(parts: &[&str], size: i64) -> ObservedFile {
    ObservedFile {
        path: rel(parts),
        size,
        mtime_ms: 1,
        container: "AIFF".into(),
        file_token: None,
    }
}

/// A pass in which every directory above the given files was found and listed.
fn complete(files: &[(&[&str], i64)]) -> ScanPass {
    let mut dirs: BTreeSet<DirPath> = BTreeSet::new();
    dirs.insert(Vec::new());
    for (parts, _) in files {
        for depth in 1..parts.len() {
            dirs.insert(dir(&parts[..depth]));
        }
    }
    ScanPass {
        root_availability: Availability::Available,
        files: files
            .iter()
            .map(|(parts, size)| observed(parts, *size))
            .collect(),
        existing_dirs: dirs.iter().cloned().collect(),
        enumerated_dirs: dirs.into_iter().collect(),
        confirm_mass_missing: false,
    }
}

struct Row {
    id: i64,
    presence: String,
    missing_since: Option<i64>,
    lifecycle: String,
    raw: Vec<u8>,
    key: Vec<u8>,
}

fn row(database: &Database, parts: &[&str]) -> Row {
    database
        .connection
        .query_row(
            "SELECT id, presence, missing_since_ms, lifecycle, path_raw, path_key FROM file \
             WHERE path_raw = ?1",
            [rel(parts).as_stored()],
            |row| {
                Ok(Row {
                    id: row.get(0)?,
                    presence: row.get(1)?,
                    missing_since: row.get(2)?,
                    lifecycle: row.get(3)?,
                    raw: row.get(4)?,
                    key: row.get(5)?,
                })
            },
        )
        .unwrap()
}

fn file_count(database: &Database) -> i64 {
    database
        .connection
        .query_row("SELECT COUNT(*) FROM file", [], |row| row.get(0))
        .unwrap()
}

fn presence(database: &Database, parts: &[&str]) -> String {
    row(database, parts).presence
}

#[test]
fn a_root_records_its_raw_path_profile_and_first_alias() {
    let (_directory, mut database) = database();
    let id = database
        .add_root(
            &NewRoot {
                path: Path::new("/Volumes/Music/Library"),
                label: "Library",
                kind: RootKind::Removable,
                profile: NameProfile::FoldCaseNormalized,
                profile_source: ProfileSource::StatProbe,
                dir_token: Some(vec![1, 2, 3]),
                volume_token: None,
            },
            5_000,
        )
        .unwrap();
    let roots = list_roots(&database.connection).unwrap();
    assert_eq!(roots.len(), 1);
    let root = &roots[0];
    assert_eq!(root.id, id);
    assert_eq!(root.path_raw, b"/Volumes/Music/Library");
    assert_eq!(root.kind, RootKind::Removable);
    assert_eq!(root.profile, NameProfile::FoldCaseNormalized);
    assert_eq!(root.profile_source, ProfileSource::StatProbe);
    assert_eq!(root.dir_token.as_deref(), Some([1, 2, 3].as_slice()));
    assert_eq!(root.availability, Availability::Available);
    assert_eq!(root.last_available_ms, Some(5_000));
    let aliases = crate::roots::root_aliases(&database.connection).unwrap();
    assert_eq!(aliases.len(), 1);
    assert_eq!(
        aliases[0].aliases,
        [LocationPath::parse(b"/Volumes/Music/Library").unwrap()]
    );
    assert!(database
        .add_root(
            &NewRoot {
                path: Path::new("/Volumes/Music/Library"),
                label: "Again",
                kind: RootKind::Local,
                profile: NameProfile::Exact,
                profile_source: ProfileSource::Default,
                dir_token: None,
                volume_token: None,
            },
            1,
        )
        .is_err());
    assert!(matches!(
        database.add_root(
            &NewRoot {
                path: Path::new("relative/path"),
                label: "x",
                kind: RootKind::Local,
                profile: NameProfile::Exact,
                profile_source: ProfileSource::Default,
                dir_token: None,
                volume_token: None,
            },
            1,
        ),
        Err(Error::InvalidRoot)
    ));
}

#[test]
fn availability_is_stored_on_the_root_and_unknown_roots_are_refused() {
    let (_directory, mut database) = database();
    let id = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    database
        .set_availability(id, Availability::Unavailable, "not mounted", 2_000)
        .unwrap();
    let root = &list_roots(&database.connection).unwrap()[0];
    assert_eq!(root.availability, Availability::Unavailable);
    assert_eq!(root.availability_reason, "not mounted");
    assert_eq!(root.availability_checked_ms, Some(2_000));
    assert_eq!(
        root.last_available_ms,
        Some(1_000),
        "only available advances it"
    );
    database
        .set_availability(id, Availability::Available, "mounted", 3_000)
        .unwrap();
    assert_eq!(
        list_roots(&database.connection).unwrap()[0].last_available_ms,
        Some(3_000)
    );
    assert!(matches!(
        database.set_availability(99, Availability::Available, "", 1),
        Err(Error::UnknownRoot(99))
    ));
    assert!(matches!(
        database.add_alias(99, &LocationPath::parse(b"/x").unwrap(), 1),
        Err(Error::UnknownRoot(99))
    ));
}

#[test]
fn a_first_scan_inserts_and_a_repeat_changes_nothing() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    let files: [(&[&str], i64); 3] = [
        (&["A", "one.aif"], 10),
        (&["A", "two.aif"], 20),
        (&["B", "three.aif"], 30),
    ];
    let first = database.apply_scan(root, &complete(&files), 100).unwrap();
    assert_eq!((first.inserted, first.unchanged), (3, 0));
    assert_eq!(file_count(&database), 3);
    let second = database.apply_scan(root, &complete(&files), 200).unwrap();
    assert_eq!(
        (second.inserted, second.unchanged, second.updated),
        (0, 3, 0)
    );
    assert_eq!(file_count(&database), 3);
    let mut changed = files;
    changed[1].1 = 21;
    let third = database.apply_scan(root, &complete(&changed), 300).unwrap();
    assert_eq!((third.updated, third.unchanged), (1, 2));
    let stored = row(&database, &["A", "one.aif"]);
    assert_eq!(stored.presence, "present");
    assert_eq!(stored.lifecycle, "active");
}

#[test]
fn a_file_that_is_gone_from_a_listed_directory_is_marked_missing_and_can_come_back() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    let both: [(&[&str], i64); 2] = [(&["A", "one.aif"], 10), (&["A", "two.aif"], 20)];
    database.apply_scan(root, &complete(&both), 100).unwrap();
    let id = row(&database, &["A", "two.aif"]).id;
    let outcome = database
        .apply_scan(root, &complete(&both[..1]), 200)
        .unwrap();
    assert_eq!(outcome.marked_missing, 1);
    let gone = row(&database, &["A", "two.aif"]);
    assert_eq!(
        (gone.presence.as_str(), gone.missing_since),
        ("missing", Some(200))
    );
    assert_eq!(presence(&database, &["A", "one.aif"]), "present");
    let again = database
        .apply_scan(root, &complete(&both[..1]), 300)
        .unwrap();
    assert_eq!(
        again.marked_missing, 0,
        "already missing, nothing more to do"
    );
    assert_eq!(row(&database, &["A", "two.aif"]).missing_since, Some(200));
    let back = database.apply_scan(root, &complete(&both), 400).unwrap();
    assert_eq!((back.revived, back.inserted), (1, 0));
    let revived = row(&database, &["A", "two.aif"]);
    assert_eq!(revived.id, id, "the same row, so its links survive");
    assert_eq!(
        (revived.presence.as_str(), revived.missing_since),
        ("present", None)
    );
}

#[test]
fn nothing_goes_missing_unless_the_directory_was_listed() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    let all: [(&[&str], i64); 4] = [
        (&["A", "one.aif"], 1),
        (&["B", "two.aif"], 1),
        (&["C", "Deep", "three.aif"], 1),
        (&["D", "four.aif"], 1),
    ];
    database.apply_scan(root, &complete(&all), 100).unwrap();

    // A could not be listed; B was listed and is empty; C/Deep could not be listed; D is
    // gone from the root's listing altogether.
    let mut pass = complete(&[(&["B", "x.aif"], 1), (&["C", "y.aif"], 1)]);
    pass.existing_dirs.push(dir(&["A"]));
    pass.existing_dirs.push(dir(&["C", "Deep"]));
    pass.files.clear();
    let outcome = database.apply_scan(root, &pass, 200).unwrap();
    assert_eq!(outcome.marked_missing, 2, "B's file and D's file");
    assert_eq!(
        presence(&database, &["A", "one.aif"]),
        "present",
        "A not listed"
    );
    assert_eq!(presence(&database, &["B", "two.aif"]), "missing");
    assert_eq!(
        presence(&database, &["C", "Deep", "three.aif"]),
        "present",
        "Deep not listed"
    );
    assert_eq!(
        presence(&database, &["D", "four.aif"]),
        "missing",
        "D left the root"
    );
}

#[test]
fn an_unlisted_root_or_an_unavailable_root_changes_nothing() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    let files: [(&[&str], i64); 2] = [(&["A", "one.aif"], 1), (&["A", "two.aif"], 1)];
    database.apply_scan(root, &complete(&files), 100).unwrap();

    let mut unlisted = complete(&[]);
    unlisted.enumerated_dirs.clear();
    let outcome = database.apply_scan(root, &unlisted, 200).unwrap();
    assert_eq!((outcome.marked_missing, outcome.held_missing), (0, 0));

    for availability in [Availability::Unavailable, Availability::Unverified] {
        let mut pass = complete(&[]);
        pass.root_availability = availability;
        let outcome = database.apply_scan(root, &pass, 300).unwrap();
        assert!(outcome.skipped, "{availability:?}");
    }
    assert_eq!(presence(&database, &["A", "one.aif"]), "present");
    assert_eq!(presence(&database, &["A", "two.aif"]), "present");
}

fn forty() -> Vec<(Vec<String>, i64)> {
    (0..40)
        .map(|n| (vec!["Lib".to_owned(), format!("{n:02}.aif")], 1))
        .collect()
}

fn pass_of(files: &[(Vec<String>, i64)]) -> ScanPass {
    let owned: Vec<(Vec<&str>, i64)> = files
        .iter()
        .map(|(parts, size)| (parts.iter().map(String::as_str).collect(), *size))
        .collect();
    let borrowed: Vec<(&[&str], i64)> = owned.iter().map(|(p, s)| (p.as_slice(), *s)).collect();
    complete(&borrowed)
}

#[test]
fn a_mass_disappearance_is_held_until_the_owner_confirms() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    let files = forty();
    database.apply_scan(root, &pass_of(&files), 100).unwrap();

    // A small loss goes through: 5 of 40 is under the 20-file minimum.
    let small = database
        .apply_scan(root, &pass_of(&files[5..]), 200)
        .unwrap();
    assert_eq!((small.marked_missing, small.held_missing), (5, 0));

    // An empty mount point: the directory lists fine and holds nothing. 35 of 35 present.
    let mut empty = complete(&[]);
    empty.existing_dirs.clear();
    let held = database.apply_scan(root, &empty, 300).unwrap();
    assert_eq!((held.marked_missing, held.held_missing), (0, 35));
    let still_present: i64 = database
        .connection
        .query_row(
            "SELECT COUNT(*) FROM file WHERE presence = 'present'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(still_present, 35);

    empty.confirm_mass_missing = true;
    let confirmed = database.apply_scan(root, &empty, 400).unwrap();
    assert_eq!((confirmed.marked_missing, confirmed.held_missing), (35, 0));
    assert_eq!(file_count(&database), 40, "no row is ever deleted");
}

#[test]
fn a_scan_never_changes_lifecycle_or_the_track_link() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    let files: [(&[&str], i64); 1] = [(&["A", "one.aif"], 1)];
    database.apply_scan(root, &complete(&files), 100).unwrap();
    database
        .connection
        .execute_batch(
            "INSERT INTO artist (id, name, sort_key) VALUES (1, 'a', 'a');
             INSERT INTO release (id, artist_id, title, sort_key, artist_key, added_ms) VALUES (1, 1, 'r', 'r', 'a', 0);
             INSERT INTO track (id, release_id, artist_id, title, title_key, artist_key, added_ms) VALUES (7, 1, 1, 't', 't', 'a', 0);
             UPDATE file SET lifecycle = 'archived', track_id = 7;",
        )
        .unwrap();
    database.apply_scan(root, &complete(&[]), 200).unwrap();
    database.apply_scan(root, &complete(&files), 300).unwrap();
    let (lifecycle, track): (String, Option<i64>) = database
        .connection
        .query_row("SELECT lifecycle, track_id FROM file", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((lifecycle.as_str(), track), ("archived", Some(7)));
}

#[test]
fn spellings_are_one_file_only_when_the_roots_profile_says_so() {
    let (_directory, mut database) = database();
    let folding = add_root(&mut database, "/Volumes/Folding", NameProfile::FoldCase);
    let exact = add_root(&mut database, "/Volumes/Exact", NameProfile::Exact);

    database
        .apply_scan(folding, &complete(&[(&["Artist", "Track.AIF"], 5)]), 100)
        .unwrap();
    let id = row(&database, &["Artist", "Track.AIF"]).id;
    let renamed = database
        .apply_scan(folding, &complete(&[(&["Artist", "track.aif"], 5)]), 200)
        .unwrap();
    assert_eq!(
        (renamed.respelled, renamed.inserted, renamed.marked_missing),
        (1, 0, 0)
    );
    let after = row(&database, &["Artist", "track.aif"]);
    assert_eq!(after.id, id, "same row after a case-only rename");
    assert_eq!(after.raw, rel(&["Artist", "track.aif"]).as_stored());
    assert_eq!(after.presence, "present");

    database
        .apply_scan(exact, &complete(&[(&["Artist", "Track.AIF"], 5)]), 100)
        .unwrap();
    let other = database
        .apply_scan(exact, &complete(&[(&["Artist", "track.aif"], 5)]), 200)
        .unwrap();
    assert_eq!(other.inserted, 1, "an exact root keeps them apart");
    assert_eq!(other.marked_missing, 1);
}

#[test]
fn a_key_collision_demotes_the_root_to_exact_and_keeps_both_files() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::FoldCase);
    database
        .apply_scan(root, &complete(&[(&["A", "x.aif"], 1)]), 100)
        .unwrap();
    let both: [(&[&str], i64); 2] = [(&["A", "x.aif"], 1), (&["a", "x.aif"], 2)];
    let outcome = database.apply_scan(root, &complete(&both), 200).unwrap();
    assert!(outcome.demoted_profile);
    assert_eq!((outcome.inserted, outcome.unchanged), (1, 1));
    let stored = &list_roots(&database.connection).unwrap()[0];
    assert_eq!(stored.profile, NameProfile::Exact);
    assert_eq!(stored.profile_source, ProfileSource::Default);
    assert_eq!(file_count(&database), 2);
    for parts in [["A", "x.aif"], ["a", "x.aif"]] {
        let file = row(&database, &parts);
        assert_eq!(file.key, file.raw, "keys are the raw bytes now");
    }
    let again = database.apply_scan(root, &complete(&both), 300).unwrap();
    assert_eq!((again.demoted_profile, again.unchanged), (false, 2));
}

#[test]
fn changing_a_profile_rekeys_everything_or_refuses_without_changing_anything() {
    let (_directory, mut database) = database();
    let root = add_root(&mut database, "/Volumes/Music", NameProfile::Exact);
    let files: [(&[&str], i64); 3] = [
        (&["A", "x.aif"], 1),
        (&["B", "Caf\u{e9}.aif"], 1),
        (&["B", "Z.aif"], 1),
    ];
    database.apply_scan(root, &complete(&files), 100).unwrap();
    database
        .set_name_profile(
            root,
            NameProfile::FoldCaseNormalized,
            ProfileSource::UserSet,
        )
        .unwrap();
    let cafe = row(&database, &["B", "Caf\u{e9}.aif"]);
    let expected = NameProfile::FoldCaseNormalized.key(&rel(&["B", "Caf\u{e9}.aif"]));
    assert_eq!(cafe.key, expected.as_bytes());
    assert_eq!(row(&database, &["B", "Z.aif"]).key, b"b\0z.aif");

    // Two spellings that only differ by case cannot share a folded key.
    let (_other, mut collide) = self::database();
    let id = add_root(&mut collide, "/Volumes/Music", NameProfile::Exact);
    let pair: [(&[&str], i64); 2] = [(&["A", "x.aif"], 1), (&["a", "x.aif"], 1)];
    collide.apply_scan(id, &complete(&pair), 100).unwrap();
    assert!(matches!(
        collide.set_name_profile(id, NameProfile::FoldCase, ProfileSource::UserSet),
        Err(Error::NameCollision)
    ));
    assert_eq!(
        list_roots(&collide.connection).unwrap()[0].profile,
        NameProfile::Exact
    );
    assert_eq!(
        row(&collide, &["A", "x.aif"]).key,
        row(&collide, &["A", "x.aif"]).raw
    );
}

fn seed_matching() -> (TempDir, Database, i64) {
    let (directory, mut database) = database();
    let root = add_root(
        &mut database,
        "/Volumes/Music",
        NameProfile::FoldCaseNormalized,
    );
    let files: [(&[&str], i64); 4] = [
        (&["Artist", "Rel", "A-B.aif"], 1),
        (&["Artist", "Rel", "AB.aif"], 1),
        (&["Artist", "Rel", "Caf\u{e9}.aif"], 1),
        (&["Artist", "Rel", "Hash #1 + 100%.aif"], 1),
    ];
    database.apply_scan(root, &complete(&files), 100).unwrap();
    (directory, database, root)
}

fn matched(
    database: &Database,
    matcher: &Matcher,
    location: &str,
) -> crate::matching::LocationMatch {
    matcher
        .match_location(&database.connection, location.as_bytes())
        .unwrap()
}

#[test]
fn locations_match_the_exact_file_never_a_look_alike() {
    let (_directory, database, root) = seed_matching();
    let matcher = Matcher::load(&database.connection).unwrap();
    let id_of = |parts: &[&str]| row(&database, parts).id;

    let ab = matched(&database, &matcher, "/Volumes/Music/Artist/Rel/A-B.aif");
    assert_eq!(
        (ab.status, ab.kind, ab.root_id, ab.file_id),
        (
            MatchStatus::Matched,
            Some(MatchKind::ExactBytes),
            Some(root),
            Some(id_of(&["Artist", "Rel", "A-B.aif"]))
        )
    );
    let plain = matched(&database, &matcher, "/Volumes/Music/Artist/Rel/AB.aif");
    assert_eq!(plain.file_id, Some(id_of(&["Artist", "Rel", "AB.aif"])));
    assert_ne!(
        plain.file_id, ab.file_id,
        "punctuation is never folded away"
    );

    let special = matched(
        &database,
        &matcher,
        "/Volumes/Music/Artist/Rel/Hash #1 + 100%.aif",
    );
    assert_eq!(special.kind, Some(MatchKind::ExactBytes));
}

#[test]
fn case_and_normalization_aliases_match_only_where_the_profile_allows() {
    let (_directory, database, _root) = seed_matching();
    let matcher = Matcher::load(&database.connection).unwrap();
    let case = matched(&database, &matcher, "/Volumes/Music/artist/REL/a-b.AIF");
    assert_eq!(
        (case.status, case.kind),
        (MatchStatus::Matched, Some(MatchKind::KeyEqual))
    );
    let decomposed = matched(
        &database,
        &matcher,
        "/Volumes/Music/Artist/Rel/Cafe\u{301}.aif",
    );
    assert_eq!(
        (decomposed.status, decomposed.kind),
        (MatchStatus::Matched, Some(MatchKind::KeyEqual))
    );
    assert_eq!(
        decomposed.file_id,
        Some(row(&database, &["Artist", "Rel", "Caf\u{e9}.aif"]).id)
    );

    // The same spellings against an exact root are different files.
    let (_other, mut strict) = self::database();
    let root = add_root(&mut strict, "/Volumes/Music", NameProfile::Exact);
    strict
        .apply_scan(root, &complete(&[(&["Artist", "Rel", "A-B.aif"], 1)]), 1)
        .unwrap();
    let matcher = Matcher::load(&strict.connection).unwrap();
    let result = matched(&strict, &matcher, "/Volumes/Music/artist/rel/a-b.aif");
    assert_eq!(
        (result.status, result.file_id),
        (MatchStatus::NotIndexed, None)
    );
    assert_eq!(result.root_id, Some(root));
}

#[test]
fn locations_outside_the_library_or_malformed_are_never_guessed() {
    let (_directory, database, _root) = seed_matching();
    let matcher = Matcher::load(&database.connection).unwrap();
    let status = |location: &str| matched(&database, &matcher, location).status;
    assert_eq!(
        status("/Elsewhere/Artist/Rel/A-B.aif"),
        MatchStatus::Unmapped
    );
    assert_eq!(
        status("/Volumes/MusicOld/Artist/Rel/A-B.aif"),
        MatchStatus::Unmapped
    );
    assert_eq!(
        status("/Volumes/Music/Artist/Rel/Missing.aif"),
        MatchStatus::NotIndexed
    );
    assert_eq!(status("Artist/Rel/A-B.aif"), MatchStatus::Unrecognized);
    assert_eq!(
        status("/Volumes/Music/Artist/../Artist/Rel/A-B.aif"),
        MatchStatus::Unrecognized
    );
    assert_eq!(
        status("/Volumes/Music"),
        MatchStatus::Unrecognized,
        "the root is no file"
    );
    assert_eq!(
        status("/Volumes/Music/Artist/Rel//A-B.aif"),
        MatchStatus::Unrecognized
    );
}

#[test]
fn an_old_prefix_alias_keeps_an_older_export_matching() {
    let (_directory, mut database, root) = seed_matching();
    let before = Matcher::load(&database.connection).unwrap();
    let old = "/D:/Old Library/Artist/Rel/A-B.aif";
    assert_eq!(
        matched(&database, &before, old).status,
        MatchStatus::Unmapped
    );
    let alias = LocationPath::parse(b"/D:/Old Library").unwrap();
    assert!(database.add_alias(root, &alias, 2_000).unwrap());
    assert!(
        !database.add_alias(root, &alias, 2_001).unwrap(),
        "already known"
    );
    let after = Matcher::load(&database.connection).unwrap();
    let found = matched(&database, &after, old);
    assert_eq!(
        (found.status, found.root_id),
        (MatchStatus::Matched, Some(root))
    );
    assert_eq!(
        found.file_id,
        Some(row(&database, &["Artist", "Rel", "A-B.aif"]).id)
    );
    // A recycle-bin entry on the same drive is simply outside the library.
    assert_eq!(
        matched(&database, &after, "/D:/$RECYCLE.BIN/x.aif").status,
        MatchStatus::Unmapped
    );
    // Drive letters ignore case.
    assert_eq!(
        matched(&database, &after, "/d:/Old Library/Artist/Rel/A-B.aif").status,
        MatchStatus::Matched
    );
}

#[test]
fn two_roots_claiming_one_prefix_are_ambiguous_not_guessed() {
    let (_directory, mut database) = self::database();
    let first = add_root(&mut database, "/Volumes/One", NameProfile::Exact);
    let second = add_root(&mut database, "/Volumes/Two", NameProfile::Exact);
    let shared = LocationPath::parse(b"/D:/Shared").unwrap();
    database.add_alias(first, &shared, 1).unwrap();
    database.add_alias(second, &shared, 1).unwrap();
    let matcher = Matcher::load(&database.connection).unwrap();
    let result = matched(&database, &matcher, "/D:/Shared/x.aif");
    assert_eq!(
        (result.status, result.file_id, result.root_id),
        (MatchStatus::Ambiguous, None, None)
    );
}

#[test]
fn matching_ignores_presence_and_availability() {
    let (_directory, mut database, root) = seed_matching();
    database
        .apply_scan(root, &complete(&[(&["Artist", "Rel", "AB.aif"], 1)]), 200)
        .unwrap();
    assert_eq!(
        presence(&database, &["Artist", "Rel", "A-B.aif"]),
        "missing"
    );
    database
        .set_availability(root, Availability::Unavailable, "drive unplugged", 300)
        .unwrap();
    let matcher = Matcher::load(&database.connection).unwrap();
    let result = matched(&database, &matcher, "/Volumes/Music/Artist/Rel/A-B.aif");
    assert_eq!(
        result.status,
        MatchStatus::Matched,
        "match status never says missing"
    );
    assert_eq!(result.kind, Some(MatchKind::ExactBytes));
}
