use std::fs;
use std::path::Path;
use std::process::Command;

use rusqlite::{Connection, TransactionBehavior};
use tempfile::tempdir;

use crate::{
    clean_shutdown, migrate, own_directory, read_connection, restore, schema_version, Database,
    Error, OpenStatus, APPLICATION_ID, DATABASE_NAME, MIGRATIONS,
};

fn latest() -> i32 {
    i32::try_from(MIGRATIONS.len()).unwrap()
}

fn with_extra(extra: &'static str) -> Vec<&'static str> {
    let mut migrations = MIGRATIONS.to_vec();
    migrations.push(extra);
    migrations
}

#[test]
fn creates_owned_wal_database_and_read_only_readers() {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    assert_eq!(database.status(), OpenStatus::Created);
    let reader = database.reader().unwrap();
    assert_eq!(schema_version(&reader).unwrap(), latest());
    assert_eq!(
        reader
            .pragma_query_value(None, "application_id", |row| row.get::<_, i32>(0))
            .unwrap(),
        APPLICATION_ID
    );
    assert_eq!(
        reader
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        reader
            .pragma_query_value(None, "foreign_keys", |row| row.get::<_, i32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get::<_, i32>(0))
            .unwrap(),
        2
    );
    assert!(!clean_shutdown(&reader).unwrap());
    assert!(reader.execute("DELETE FROM app_state", []).is_err());
    reader.pragma_update(None, "query_only", false).unwrap();
    assert!(reader.execute("DELETE FROM app_state", []).is_err());
    database.close().unwrap();
}

#[test]
fn bundled_sqlite_supports_fts5() {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    database
        .connection
        .execute_batch(
            "CREATE VIRTUAL TABLE temp.search_test USING fts5(title);
             INSERT INTO search_test VALUES ('Synthetic track');",
        )
        .unwrap();
    assert_eq!(
        database
            .connection
            .query_row(
                "SELECT count(*) FROM search_test WHERE search_test MATCH 'Synthetic'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .unwrap(),
        1
    );
    database.close().unwrap();
}

#[test]
fn read_snapshot_stays_consistent_while_writer_commits() {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    database
        .connection
        .execute_batch(
            "CREATE TABLE synthetic (id INTEGER PRIMARY KEY);
             INSERT INTO synthetic VALUES (1);",
        )
        .unwrap();
    let reader = database.reader().unwrap();
    let count = || {
        reader
            .query_row("SELECT count(*) FROM synthetic", [], |row| {
                row.get::<_, i32>(0)
            })
            .unwrap()
    };
    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(count(), 1);
    database
        .connection
        .execute("INSERT INTO synthetic VALUES (2)", [])
        .unwrap();
    assert_eq!(count(), 1);
    reader.execute_batch("COMMIT").unwrap();
    assert_eq!(count(), 2);
    database.close().unwrap();
}

#[test]
fn distinguishes_clean_close_from_dropped_owner() {
    let directory = tempdir().unwrap();
    Database::open(directory.path()).unwrap().close().unwrap();
    let database = Database::open(directory.path()).unwrap();
    assert_eq!(database.status(), OpenStatus::Clean);
    drop(database);
    let database = Database::open(directory.path()).unwrap();
    assert_eq!(database.status(), OpenStatus::CheckedAfterUncleanShutdown);
    database.close().unwrap();
}

#[test]
fn refuses_foreign_empty_and_future_databases_without_changing_them() {
    for kind in ["foreign", "empty", "future"] {
        let directory = tempdir().unwrap();
        let path = directory.path().join(DATABASE_NAME);
        if kind == "future" {
            Database::open(directory.path()).unwrap().close().unwrap();
            let connection = Connection::open(&path).unwrap();
            connection.pragma_update(None, "user_version", 999).unwrap();
            connection.close().unwrap();
        } else {
            let connection = Connection::open(&path).unwrap();
            if kind == "foreign" {
                connection.execute("CREATE TABLE other (id)", []).unwrap();
            }
            connection.close().unwrap();
        }
        let before = fs::read(&path).unwrap();
        let result = Database::open(directory.path());
        if kind == "future" {
            assert!(matches!(
                result,
                Err(Error::FutureSchema { found: 999, supported }) if supported == latest()
            ));
        } else {
            assert!(matches!(result, Err(Error::ForeignDatabase)));
        }
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn refuses_corrupt_database_and_invalid_lifecycle_state() {
    let directory = tempdir().unwrap();
    let path = directory.path().join(DATABASE_NAME);
    fs::write(&path, b"not a database").unwrap();
    assert!(matches!(
        Database::open(directory.path()),
        Err(Error::Sqlite(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"not a database");

    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    database
        .connection
        .execute("DELETE FROM app_state", [])
        .unwrap();
    drop(database);
    assert!(matches!(
        Database::open(directory.path()),
        Err(Error::InvalidState)
    ));
}

#[test]
fn owner_lock_blocks_second_instance_and_releases_on_close() {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    assert!(matches!(
        Database::open(directory.path()),
        Err(Error::AlreadyOpen)
    ));
    run_child("locked", directory.path(), 0);
    database.close().unwrap();
    Database::open(directory.path()).unwrap().close().unwrap();
}

#[test]
fn snapshot_includes_committed_wal_and_restores_without_sidecars() {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    database
        .connection
        .pragma_update(None, "wal_autocheckpoint", 0)
        .unwrap();
    database
        .connection
        .execute_batch(
            "CREATE TABLE synthetic (value TEXT NOT NULL);
             INSERT INTO synthetic VALUES ('before snapshot');",
        )
        .unwrap();
    assert!(
        fs::metadata(directory.path().join("library.sqlite3-wal"))
            .unwrap()
            .len()
            > 0
    );
    let snapshot = database.snapshot().unwrap();
    let second_snapshot = database.snapshot().unwrap();
    assert_ne!(snapshot, second_snapshot);
    database
        .connection
        .execute("UPDATE synthetic SET value = 'after snapshot'", [])
        .unwrap();
    let target = tempdir().unwrap();
    let restored_path = restore(&snapshot, target.path()).unwrap();
    let restored = Database::open(target.path()).unwrap();
    assert_eq!(restored.status(), OpenStatus::Clean);
    assert_eq!(
        restored
            .reader()
            .unwrap()
            .query_row("SELECT value FROM synthetic", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap(),
        "before snapshot"
    );
    assert_eq!(
        restored_path,
        oc_paths::canonical(target.path())
            .unwrap()
            .join(DATABASE_NAME)
    );
    assert_eq!(
        fs::read_dir(snapshot.parent().unwrap())
            .unwrap()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap()
            .len(),
        2
    );
    restored.close().unwrap();
    database.close().unwrap();
}

#[test]
fn restore_never_overwrites_existing_or_active_database() {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    let snapshot = database.snapshot().unwrap();
    assert!(matches!(
        restore(&snapshot, directory.path()),
        Err(Error::AlreadyOpen)
    ));
    database.close().unwrap();
    let before = fs::read(directory.path().join(DATABASE_NAME)).unwrap();
    assert!(matches!(
        restore(&snapshot, directory.path()),
        Err(Error::DestinationExists)
    ));
    assert_eq!(
        fs::read(directory.path().join(DATABASE_NAME)).unwrap(),
        before
    );
}

#[test]
fn orphan_journals_block_creation_and_restore_without_being_changed() {
    let source = tempdir().unwrap();
    let database = Database::open(source.path()).unwrap();
    let snapshot = database.snapshot().unwrap();
    for suffix in ["-wal", "-shm", "-journal"] {
        let target = tempdir().unwrap();
        let orphan = target.path().join(format!("{DATABASE_NAME}{suffix}"));
        fs::write(&orphan, b"preserve recovery material").unwrap();
        assert!(matches!(
            Database::open(target.path()),
            Err(Error::OrphanJournal)
        ));
        assert!(matches!(
            restore(&snapshot, target.path()),
            Err(Error::OrphanJournal)
        ));
        assert!(!target.path().join(DATABASE_NAME).exists());
        assert_eq!(fs::read(&orphan).unwrap(), b"preserve recovery material");
    }
    database.close().unwrap();
}

#[test]
fn restore_rejects_corrupt_foreign_future_and_missing_snapshots() {
    let source = tempdir().unwrap();
    let target = tempdir().unwrap();
    let path = source.path().join("source.sqlite3");
    assert!(restore(&path, target.path()).is_err());
    fs::write(&path, b"not a database").unwrap();
    assert!(restore(&path, target.path()).is_err());
    fs::remove_file(&path).unwrap();
    Connection::open(&path).unwrap().close().unwrap();
    assert!(matches!(
        restore(&path, target.path()),
        Err(Error::ForeignDatabase)
    ));
    let database = Database::open(source.path()).unwrap();
    let snapshot = database.snapshot().unwrap();
    let future = Connection::open(&snapshot).unwrap();
    future.pragma_update(None, "user_version", 999).unwrap();
    future.close().unwrap();
    assert!(matches!(
        restore(&snapshot, target.path()),
        Err(Error::FutureSchema { .. })
    ));
    assert!(!target.path().join(DATABASE_NAME).exists());
    database.close().unwrap();
}

#[test]
fn migration_snapshots_precede_version_and_schema_changes() {
    let directory = tempdir().unwrap();
    let mut database = Database::open(directory.path()).unwrap();
    let snapshots = directory.path().join("snapshots");
    migrate(
        &mut database.connection,
        &with_extra("CREATE TABLE next_schema (id INTEGER);"),
        &snapshots,
    )
    .unwrap();
    assert_eq!(schema_version(&database.connection).unwrap(), latest() + 1);
    let paths: Vec<_> = fs::read_dir(&snapshots)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 1);
    let backup = read_connection(&paths[0]).unwrap();
    assert_eq!(schema_version(&backup).unwrap(), latest());
    assert!(backup.prepare("SELECT * FROM next_schema").is_err());
    assert!(!clean_shutdown(&backup).unwrap());
    let restored = tempdir().unwrap();
    restore(&paths[0], restored.path()).unwrap();
    Database::open(restored.path()).unwrap().close().unwrap();
    database.close().unwrap();
}

#[test]
fn failed_migration_rolls_back_and_retains_restorable_snapshot() {
    let directory = tempdir().unwrap();
    let mut database = Database::open(directory.path()).unwrap();
    let snapshots = directory.path().join("snapshots");
    assert!(migrate(
        &mut database.connection,
        &with_extra("CREATE TABLE rollback_test (id); DELETE FROM app_state; INVALID SQL;"),
        &snapshots,
    )
    .is_err());
    assert_eq!(schema_version(&database.connection).unwrap(), latest());
    assert!(database
        .connection
        .prepare("SELECT * FROM rollback_test")
        .is_err());
    assert!(!clean_shutdown(&database.connection).unwrap());
    let snapshot = fs::read_dir(&snapshots)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let restored = tempdir().unwrap();
    restore(&snapshot, restored.path()).unwrap();
    Database::open(restored.path()).unwrap().close().unwrap();
    database.close().unwrap();
}

#[test]
fn unavailable_backup_location_prevents_migration() {
    let directory = tempdir().unwrap();
    let mut database = Database::open(directory.path()).unwrap();
    let snapshots = directory.path().join("snapshots");
    fs::write(&snapshots, b"not a directory").unwrap();
    assert!(migrate(
        &mut database.connection,
        &with_extra("CREATE TABLE forbidden (id);"),
        &snapshots,
    )
    .is_err());
    assert_eq!(schema_version(&database.connection).unwrap(), latest());
    assert!(database
        .connection
        .prepare("SELECT * FROM forbidden")
        .is_err());
    database.close().unwrap();
}

#[test]
fn process_exit_rolls_back_partial_migration_and_releases_lock() {
    let directory = tempdir().unwrap();
    Database::open(directory.path()).unwrap().close().unwrap();
    run_child("interrupt", directory.path(), 86);
    let database = Database::open(directory.path()).unwrap();
    assert_eq!(database.status(), OpenStatus::CheckedAfterUncleanShutdown);
    assert_eq!(schema_version(&database.connection).unwrap(), latest());
    assert!(database
        .connection
        .prepare("SELECT * FROM interrupted")
        .is_err());
    database.close().unwrap();
}

#[test]
fn unclean_open_checks_foreign_key_integrity() {
    let directory = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    database
        .connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             CREATE TABLE parent (id INTEGER PRIMARY KEY);
             CREATE TABLE child (parent_id INTEGER REFERENCES parent(id));
             INSERT INTO child VALUES (42);",
        )
        .unwrap();
    drop(database);
    assert!(matches!(
        Database::open(directory.path()),
        Err(Error::Integrity)
    ));
}

#[cfg(unix)]
#[test]
fn refuses_symlinked_database_and_locks_directory_aliases() {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let alias = tempdir().unwrap();
    let database = Database::open(directory.path()).unwrap();
    symlink(directory.path(), alias.path().join("alias")).unwrap();
    assert!(matches!(
        Database::open(&alias.path().join("alias")),
        Err(Error::AlreadyOpen)
    ));
    symlink(
        directory.path().join(DATABASE_NAME),
        alias.path().join(DATABASE_NAME),
    )
    .unwrap();
    assert!(matches!(
        Database::open(alias.path()),
        Err(Error::InvalidFile)
    ));
    database.close().unwrap();
}

// A lock belongs to the open file description, not to one descriptor. A child that another
// thread is spawning holds a copy of every open descriptor until it execs, so closing the
// owner's descriptor alone can leave the directory locked. `try_clone` makes the same copy.
#[test]
#[expect(
    clippy::used_underscore_binding,
    reason = "the field exists only to be dropped; this test must reach its descriptor"
)]
fn close_and_drop_release_owner_lock_while_a_descriptor_copy_survives() {
    for clean in [true, false] {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path()).unwrap();
        let copy = database._owner.0.try_clone().unwrap();
        if clean {
            database.close().unwrap();
        } else {
            drop(database);
        }
        let reopened = Database::open(directory.path()).unwrap();
        assert_eq!(
            reopened.status(),
            if clean {
                OpenStatus::Clean
            } else {
                OpenStatus::CheckedAfterUncleanShutdown
            }
        );
        reopened.close().unwrap();
        drop(copy);
    }
}

// A failed `Database::open` and `restore` hold the lock only through this value.
#[test]
fn dropped_directory_lock_is_released_while_a_descriptor_copy_survives() {
    let directory = tempdir().unwrap();
    let (_, owner) = own_directory(directory.path()).unwrap();
    let copy = owner.0.try_clone().unwrap();
    drop(owner);
    assert!(own_directory(directory.path()).is_ok());
    drop(copy);
}

fn run_child(action: &str, directory: &Path, expected_code: i32) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::process_helper", "--nocapture"])
        .env("OPENCRATE_TEST_CHILD_ACTION", action)
        .env("OPENCRATE_TEST_CHILD_DIRECTORY", directory)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn process_helper() {
    let Ok(action) = std::env::var("OPENCRATE_TEST_CHILD_ACTION") else {
        return;
    };
    let directory = std::env::var_os("OPENCRATE_TEST_CHILD_DIRECTORY").unwrap();
    match action.as_str() {
        "locked" => assert!(matches!(
            Database::open(Path::new(&directory)),
            Err(Error::AlreadyOpen)
        )),
        "interrupt" => {
            let mut database = Database::open(Path::new(&directory)).unwrap();
            database.snapshot().unwrap();
            let transaction = database
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            transaction
                .execute_batch(
                    "CREATE TABLE interrupted (id INTEGER);
                     INSERT INTO interrupted VALUES (1);
                     PRAGMA user_version = 99;",
                )
                .unwrap();
            std::process::exit(86);
        }
        _ => panic!("unknown child action"),
    }
}
