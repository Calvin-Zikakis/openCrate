use std::fs;
use std::path::{Path, PathBuf};

use tempfile::{tempdir, TempDir};

use crate::*;

const CONTENT: &[u8] = b"pretend this is a long audio file";

struct World {
    dir: TempDir,
    staging: PathBuf,
    library: PathBuf,
}

impl World {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        let staging = dir.path().join("staging");
        let library = dir.path().join("library/Artist/Release");
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&library).unwrap();
        Self {
            dir,
            staging,
            library,
        }
    }

    fn staged(&self, bytes: &[u8]) -> PathBuf {
        let path = self.staging.join("01 - Title.aif");
        fs::write(&path, bytes).unwrap();
        path
    }

    fn destination(&self) -> PathBuf {
        self.library.join("01 - Artist - Title.aif")
    }
}

fn crash_at(target: FaultPoint) -> impl FnMut(FaultPoint) -> std::result::Result<(), Crash> {
    move |point| {
        if point == target {
            Err(Crash(point))
        } else {
            Ok(())
        }
    }
}

fn run_with(
    store: &mut MemoryStore,
    operation: Operation,
    mut faults: impl Faults,
) -> crate::Result<Outcome> {
    Executor::new(store, &mut faults).run(operation)
}

/// After any recovery: one active file for the operation, the content intact at the destination,
/// the staged name gone, and the record final.
fn assert_converged(world: &World, store: &MemoryStore, id: u64) {
    assert_eq!(store.active, [id], "exactly one active file");
    assert_eq!(fs::read(world.destination()).unwrap(), CONTENT);
    assert!(!world.staging.join("01 - Title.aif").exists());
    assert_eq!(store.get(id).unwrap().phase, Phase::Cleaned);
    assert_eq!(
        fs::read_dir(&world.library).unwrap().count(),
        1,
        "no stray names"
    );
}

#[test]
fn a_clean_run_publishes_once_and_leaves_only_the_destination() {
    let world = World::new();
    let source = world.staged(CONTENT);
    let operation = plan(1, &source, &world.destination()).unwrap();
    let mut store = MemoryStore::default();
    let outcome = run_with(&mut store, operation, NoFaults).unwrap();
    assert_eq!(outcome, Outcome::Published);
    assert_converged(&world, &store, 1);
}

#[test]
fn a_crash_at_every_point_converges_to_one_published_file() {
    for point in FaultPoint::PUBLISH {
        let world = World::new();
        let source = world.staged(CONTENT);
        let operation = plan(7, &source, &world.destination()).unwrap();
        let mut store = MemoryStore::default();
        let crashed = run_with(&mut store, operation.clone(), crash_at(point));
        assert!(
            matches!(crashed, Err(Error::Crash(found)) if found == point),
            "{point:?}"
        );

        // Restart: the only thing that survives is the store and the disk.
        let ended = Executor::new(&mut store, &mut NoFaults)
            .recover_all()
            .unwrap();
        assert_eq!(ended, [(7, Outcome::Published)], "{point:?}");
        assert_converged(&world, &store, 7);
        // Recovering again changes nothing.
        assert_eq!(
            Executor::new(&mut store, &mut NoFaults)
                .recover_all()
                .unwrap()
                .len(),
            0
        );
        assert_converged(&world, &store, 7);
    }
}

#[test]
fn a_second_crash_during_recovery_still_converges() {
    for first in FaultPoint::PUBLISH {
        for second in FaultPoint::PUBLISH {
            let world = World::new();
            let source = world.staged(CONTENT);
            let operation = plan(3, &source, &world.destination()).unwrap();
            let mut store = MemoryStore::default();
            let _ = run_with(&mut store, operation.clone(), crash_at(first));
            // The recovery itself loses power at `second` (or finishes if it never gets there).
            let mut faults = crash_at(second);
            let _ = Executor::new(&mut store, &mut faults).recover_all();
            // And a third start with no faults must finish the job exactly once.
            Executor::new(&mut store, &mut NoFaults)
                .recover_all()
                .unwrap();
            assert_converged(&world, &store, 3);
        }
    }
}

#[test]
fn something_already_at_the_destination_is_never_overwritten() {
    for other in [b"someone else's file".as_slice(), CONTENT] {
        let world = World::new();
        let source = world.staged(CONTENT);
        fs::write(world.destination(), other).unwrap();
        let operation = plan(1, &source, &world.destination()).unwrap();
        let mut store = MemoryStore::default();
        let outcome = run_with(&mut store, operation, NoFaults).unwrap();
        assert_eq!(outcome, Outcome::Rejected(Rejection::DestinationOccupied));
        assert_eq!(fs::read(world.destination()).unwrap(), other);
        assert_eq!(
            fs::read(&source).unwrap(),
            CONTENT,
            "the staged file is kept"
        );
        assert_eq!(store.active.len(), 0);
    }
}

#[test]
fn a_file_that_arrives_between_the_check_and_the_link_is_not_clobbered() {
    let world = World::new();
    let source = world.staged(CONTENT);
    let destination = world.destination();
    let operation = plan(1, &source, &destination).unwrap();
    let mut store = MemoryStore::default();
    let racer = destination.clone();
    let faults = move |point: FaultPoint| {
        if point == FaultPoint::BeforeLink {
            fs::write(&racer, b"arrived concurrently").unwrap();
        }
        Ok(())
    };
    let outcome = run_with(&mut store, operation, faults).unwrap();
    assert_eq!(outcome, Outcome::Rejected(Rejection::DestinationOccupied));
    assert_eq!(fs::read(&destination).unwrap(), b"arrived concurrently");
    assert_eq!(fs::read(&source).unwrap(), CONTENT);
    assert_eq!(store.active.len(), 0);
}

#[test]
fn a_staged_file_that_changed_since_planning_is_not_published() {
    let world = World::new();
    let source = world.staged(CONTENT);
    let operation = plan(1, &source, &world.destination()).unwrap();
    fs::write(&source, b"edited after planning").unwrap();
    let mut store = MemoryStore::default();
    let outcome = run_with(&mut store, operation, NoFaults).unwrap();
    assert_eq!(outcome, Outcome::Rejected(Rejection::SourceChanged));
    assert!(!world.destination().exists());
    assert_eq!(store.active.len(), 0);
}

#[test]
fn a_destination_deleted_after_recording_keeps_the_staged_copy() {
    let world = World::new();
    let source = world.staged(CONTENT);
    let destination = world.destination();
    let operation = plan(1, &source, &destination).unwrap();
    let mut store = MemoryStore::default();
    let _ = run_with(
        &mut store,
        operation,
        crash_at(FaultPoint::AfterRecordCommit),
    );
    // Between the crash and the restart, the library file is removed (and, being a second name
    // for the staged file, this does not touch the staged name).
    fs::remove_file(&destination).unwrap();
    let ended = Executor::new(&mut store, &mut NoFaults)
        .recover_all()
        .unwrap();
    assert_eq!(
        ended,
        [(1, Outcome::Rejected(Rejection::DestinationChanged))]
    );
    assert_eq!(
        fs::read(&source).unwrap(),
        CONTENT,
        "the only copy left is not deleted"
    );
}

#[test]
fn the_staged_name_is_never_removed_while_the_destination_is_missing_or_different() {
    let world = World::new();
    let source = world.staged(CONTENT);
    let destination = world.destination();
    let operation = plan(1, &source, &destination).unwrap();
    let mut store = MemoryStore::default();
    let _ = run_with(
        &mut store,
        operation,
        crash_at(FaultPoint::AfterRecordCommit),
    );
    fs::remove_file(&destination).unwrap();
    fs::write(&destination, b"a different file now lives here").unwrap();
    let ended = Executor::new(&mut store, &mut NoFaults)
        .recover_all()
        .unwrap();
    assert_eq!(
        ended,
        [(1, Outcome::Rejected(Rejection::DestinationChanged))]
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"a different file now lives here"
    );
    assert_eq!(fs::read(&source).unwrap(), CONTENT);
}

#[test]
fn several_operations_recover_independently() {
    let world = World::new();
    let mut store = MemoryStore::default();
    let mut expected = Vec::new();
    for (id, point) in (1_u64..).zip(FaultPoint::PUBLISH) {
        let source = world.staging.join(format!("{id}.aif"));
        let destination = world.library.join(format!("{id}.aif"));
        fs::write(&source, format!("track {id}")).unwrap();
        let operation = plan(id, &source, &destination).unwrap();
        let _ = run_with(&mut store, operation.clone(), crash_at(point));
        expected.push((id, source, destination));
    }
    Executor::new(&mut store, &mut NoFaults)
        .recover_all()
        .unwrap();
    let mut active = store.active.clone();
    active.sort_unstable();
    assert_eq!(
        active,
        (1..=8).collect::<Vec<_>>(),
        "one active file each, no repeats"
    );
    for (id, source, destination) in expected {
        assert_eq!(
            fs::read(&destination).unwrap(),
            format!("track {id}").as_bytes()
        );
        assert!(!source.exists(), "{id}");
        assert_eq!(store.get(id).unwrap().phase, Phase::Cleaned);
    }
}

#[test]
fn linking_refuses_an_existing_name_and_a_missing_source() {
    let world = World::new();
    let source = world.staged(CONTENT);
    let destination = world.destination();
    link_no_clobber(&source, &destination).unwrap();
    assert!(matches!(
        link_no_clobber(&source, &destination),
        Err(Error::DestinationExists)
    ));
    assert!(link_no_clobber(&world.staging.join("nope"), &world.library.join("x")).is_err());
}

fn stage_and_check(world: &World, from: &Path) -> (PathBuf, Identity) {
    let staged = world.staging.join("copied.aif");
    let identity = stage_copy(from, &staged, &mut NoFaults).unwrap();
    (staged, identity)
}

#[test]
fn a_copy_into_staging_is_verified_and_keeps_the_source() {
    let world = World::new();
    let origin = world.dir.path().join("download.aif");
    fs::write(&origin, CONTENT).unwrap();
    let (staged, identity) = stage_and_check(&world, &origin);
    assert_eq!(fs::read(&staged).unwrap(), CONTENT);
    assert_eq!(
        fs::read(&origin).unwrap(),
        CONTENT,
        "the download is only read"
    );
    assert_eq!(identity.size, CONTENT.len() as u64);
    assert_eq!(sweep_partials(&world.staging).unwrap(), 0);
    assert_eq!(fs::read_dir(&world.staging).unwrap().count(), 1);
}

#[test]
fn a_crash_while_copying_leaves_only_a_part_file_that_is_swept_and_a_retry_succeeds() {
    for point in FaultPoint::COPY {
        let world = World::new();
        let origin = world.dir.path().join("download.aif");
        fs::write(&origin, vec![7_u8; 3 << 20]).unwrap();
        let staged = world.staging.join("copied.aif");
        let crashed = stage_copy(&origin, &staged, &mut crash_at(point));
        assert!(matches!(crashed, Err(Error::Crash(found)) if found == point));
        assert!(!staged.exists() || point == FaultPoint::BeforeCopyRename && !staged.exists());
        assert_eq!(
            fs::read_dir(&world.staging).unwrap().count(),
            1,
            "just the .part file"
        );
        assert_eq!(sweep_partials(&world.staging).unwrap(), 1);
        assert_eq!(fs::read_dir(&world.staging).unwrap().count(), 0);

        let identity = stage_copy(&origin, &staged, &mut NoFaults).unwrap();
        assert_eq!(identity.size, 3 << 20);
        assert_eq!(fs::metadata(&staged).unwrap().len(), 3 << 20);
        assert_eq!(fs::read(&origin).unwrap().len(), 3 << 20);
    }
}

#[test]
fn copy_then_publish_ends_with_one_file_in_the_library_and_the_download_untouched() {
    let world = World::new();
    let origin = world.dir.path().join("download.aif");
    fs::write(&origin, CONTENT).unwrap();
    let staged = world.staging.join("01 - Title.aif");
    stage_copy(&origin, &staged, &mut NoFaults).unwrap();
    let operation = plan(1, &staged, &world.destination()).unwrap();
    let mut store = MemoryStore::default();
    assert_eq!(
        run_with(&mut store, operation, NoFaults).unwrap(),
        Outcome::Published
    );
    assert_converged(&world, &store, 1);
    assert_eq!(fs::read(&origin).unwrap(), CONTENT);
}
