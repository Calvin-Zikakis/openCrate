use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use oc_publish::{Crash, Executor, FaultPoint, NoFaults, Outcome, Phase, Rejection, Store};
use tempfile::{tempdir, TempDir};

use crate::publish_store::PublishStore;
use crate::Database;

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
        let library = dir.path().join("library");
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&library).unwrap();
        Self {
            dir,
            staging,
            library,
        }
    }

    fn database(&self) -> Database {
        let database = Database::open(&self.dir.path().join("db")).unwrap();
        database
            .connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS published_file (op_id INTEGER PRIMARY KEY, calls INTEGER NOT NULL)",
            )
            .unwrap();
        database
    }

    fn staged(&self) -> PathBuf {
        let path = self.staging.join("01.aif");
        fs::write(&path, CONTENT).unwrap();
        path
    }

    fn destination(&self) -> PathBuf {
        self.library.join("01 - Artist - Title.aif")
    }
}

fn crash_at(target: FaultPoint) -> impl FnMut(FaultPoint) -> Result<(), Crash> {
    move |point| {
        if point == target {
            Err(Crash(point))
        } else {
            Ok(())
        }
    }
}

fn published_rows(database: &Database) -> i64 {
    database
        .connection
        .query_row("SELECT COUNT(*) FROM published_file", [], |row| row.get(0))
        .unwrap()
}

fn record(
    transaction: &rusqlite::Transaction<'_>,
    operation: &oc_publish::Operation,
) -> rusqlite::Result<()> {
    // Idempotent by operation id, as the contract demands.
    transaction.execute(
        "INSERT INTO published_file (op_id, calls) VALUES (?1, 1) \
         ON CONFLICT(op_id) DO UPDATE SET calls = calls + 1",
        [i64::try_from(operation.id).unwrap()],
    )?;
    Ok(())
}

fn calls(database: &Database) -> i64 {
    database
        .connection
        .query_row(
            "SELECT COALESCE(SUM(calls), 0) FROM published_file",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn assert_published(world: &World, database: &Database, id: u64) {
    assert_eq!(published_rows(database), 1, "exactly one active file");
    assert_eq!(calls(database), 1, "the catalog change ran exactly once");
    assert_eq!(fs::read(world.destination()).unwrap(), CONTENT);
    assert!(!world.staging.join("01.aif").exists());
    let store = PublishStore::new_for_read(database);
    assert_eq!(store.get(id).unwrap().unwrap().phase, Phase::Cleaned);
}

#[test]
fn a_publication_is_recorded_in_the_database_and_runs_the_catalog_change_once() {
    let world = World::new();
    let mut database = world.database();
    let source = world.staged();
    let mut store = PublishStore::new(&mut database).with_record_hook(record);
    let operation = store.begin(&source, &world.destination()).unwrap();
    assert_eq!(operation.phase, Phase::Planned);
    let outcome = Executor::new(&mut store, &mut NoFaults)
        .run(operation.clone())
        .unwrap();
    assert_eq!(outcome, Outcome::Published);
    assert_eq!(store.unfinished().unwrap().len(), 0);
    drop(store);
    assert_published(&world, &database, operation.id);
}

#[test]
fn a_crash_at_any_point_then_a_real_restart_finishes_the_job_once() {
    for point in FaultPoint::PUBLISH {
        let world = World::new();
        let source = world.staged();
        let id = {
            let mut database = world.database();
            let mut store = PublishStore::new(&mut database).with_record_hook(record);
            let operation = store.begin(&source, &world.destination()).unwrap();
            let crashed = Executor::new(&mut store, &mut crash_at(point)).run(operation.clone());
            assert!(crashed.is_err(), "{point:?}");
            operation.id
            // The database is dropped here without a clean close, as a crash would leave it.
        };

        let mut database = world.database();
        let mut store = PublishStore::new(&mut database).with_record_hook(record);
        let ended = Executor::new(&mut store, &mut NoFaults)
            .recover_all()
            .unwrap();
        assert_eq!(ended, [(id, Outcome::Published)], "{point:?}");
        drop(store);
        assert_published(&world, &database, id);

        // Another restart finds nothing to do.
        let mut store = PublishStore::new(&mut database).with_record_hook(record);
        assert_eq!(
            Executor::new(&mut store, &mut NoFaults)
                .recover_all()
                .unwrap()
                .len(),
            0
        );
        drop(store);
        assert_published(&world, &database, id);
    }
}

#[test]
fn a_catalog_change_that_fails_rolls_the_record_back_and_a_retry_succeeds() {
    let world = World::new();
    let mut database = world.database();
    let source = world.staged();
    let attempts = Cell::new(0);
    let operation = {
        let mut store =
            PublishStore::new(&mut database).with_record_hook(|transaction, operation| {
                attempts.set(attempts.get() + 1);
                if attempts.get() == 1 {
                    // Write something first, to prove it is undone with the failed commit.
                    record(transaction, operation)?;
                    return Err(rusqlite::Error::InvalidQuery);
                }
                record(transaction, operation)
            });
        let operation = store.begin(&source, &world.destination()).unwrap();
        let failed = Executor::new(&mut store, &mut NoFaults).run(operation.clone());
        assert!(failed.is_err());
        operation
    };
    assert_eq!(
        published_rows(&database),
        0,
        "nothing of the failed commit stays"
    );
    {
        let store = PublishStore::new_for_read(&database);
        assert_eq!(
            store.get(operation.id).unwrap().unwrap().phase,
            Phase::Publishing
        );
    }
    assert!(
        world.destination().exists(),
        "the link is there; recovery recognises it"
    );

    let mut store = PublishStore::new(&mut database).with_record_hook(record);
    let ended = Executor::new(&mut store, &mut NoFaults)
        .recover_all()
        .unwrap();
    assert_eq!(ended, [(operation.id, Outcome::Published)]);
    drop(store);
    assert_published(&world, &database, operation.id);
}

#[test]
fn a_rejection_is_stored_with_its_reason_and_is_not_retried() {
    let world = World::new();
    let mut database = world.database();
    let source = world.staged();
    fs::write(world.destination(), b"someone else's file").unwrap();
    let mut store = PublishStore::new(&mut database).with_record_hook(record);
    let operation = store.begin(&source, &world.destination()).unwrap();
    let outcome = Executor::new(&mut store, &mut NoFaults)
        .run(operation.clone())
        .unwrap();
    assert_eq!(outcome, Outcome::Rejected(Rejection::DestinationOccupied));
    assert_eq!(
        store.get(operation.id).unwrap().unwrap().phase,
        Phase::Rejected(Rejection::DestinationOccupied)
    );
    assert_eq!(store.unfinished().unwrap().len(), 0);
    assert_eq!(
        Executor::new(&mut store, &mut NoFaults)
            .recover_all()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        fs::read(world.destination()).unwrap(),
        b"someone else's file"
    );
    assert_eq!(fs::read(&source).unwrap(), CONTENT);
    drop(store);
    assert_eq!(published_rows(&database), 0);
}

#[test]
fn unfinished_operations_come_back_oldest_first_and_a_broken_table_is_an_error() {
    let world = World::new();
    let mut database = world.database();
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        let source = world.staging.join(format!("{name}.aif"));
        fs::write(&source, name).unwrap();
        let mut store = PublishStore::new(&mut database);
        ids.push(
            store
                .begin(&source, &world.library.join(format!("{name}.aif")))
                .unwrap()
                .id,
        );
    }
    let store = PublishStore::new_for_read(&database);
    let listed: Vec<u64> = store.unfinished().unwrap().iter().map(|o| o.id).collect();
    assert_eq!(listed, ids);
    assert!(store
        .unfinished()
        .unwrap()
        .iter()
        .all(|o| o.phase == Phase::Planned));

    database
        .connection
        .execute_batch("DROP TABLE publish_operation")
        .unwrap();
    let store = PublishStore::new_for_read(&database);
    assert!(
        store.unfinished().is_err(),
        "an unreadable table must not read as nothing to recover"
    );
}

#[test]
fn paths_are_stored_as_raw_bytes_and_read_back_exactly() {
    let world = World::new();
    let mut database = world.database();
    let name = "Caf\u{e9} - \u{30c8}\u{30e9}\u{30c3}\u{30af}.aif";
    let source: &Path = &world.staging.join(name);
    fs::write(source, CONTENT).unwrap();
    let destination = world.library.join(name);
    let mut store = PublishStore::new(&mut database);
    let operation = store.begin(source, &destination).unwrap();
    let again = store.get(operation.id).unwrap().unwrap();
    assert_eq!(
        (again.source.as_path(), again.destination.as_path()),
        (source, destination.as_path())
    );
    assert_eq!(again.expected, operation.expected);
}
