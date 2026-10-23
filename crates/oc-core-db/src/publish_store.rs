//! The SQLite side of the publish protocol: `oc_publish::Store` over the `publish_operation`
//! table. `commit_record` runs the caller's hook (which makes the file active in the catalog)
//! and records the phase in one transaction, so a crash leaves both or neither.

use std::path::Path;

use oc_paths::{bytes_to_os_string, os_str_to_bytes};
use oc_publish::{identity_of, Identity, Operation, Phase, Rejection, Store};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::{Database, Error, Result};

/// Called inside the `commit_record` transaction. Must be idempotent by operation id and must
/// change only the database. An error rolls the whole commit back.
pub type RecordHook<'a> = dyn FnMut(&Transaction<'_>, &Operation) -> rusqlite::Result<()> + 'a;

enum Handle<'a> {
    Writer(&'a mut Database),
    Reader(&'a Database),
}

impl Handle<'_> {
    fn connection(&self) -> &Connection {
        match self {
            Self::Writer(database) => &database.connection,
            Self::Reader(database) => &database.connection,
        }
    }
}

pub struct PublishStore<'a> {
    database: Handle<'a>,
    on_record: Option<Box<RecordHook<'a>>>,
}

fn store_error(error: &rusqlite::Error) -> oc_publish::Error {
    oc_publish::Error::Store(error.to_string())
}

const fn phase_text(phase: Phase) -> (&'static str, Option<&'static str>) {
    match phase {
        Phase::Planned => ("planned", None),
        Phase::Publishing => ("publishing", None),
        Phase::Recorded => ("recorded", None),
        Phase::Cleaned => ("cleaned", None),
        Phase::Rejected(Rejection::SourceChanged) => ("rejected", Some("source_changed")),
        Phase::Rejected(Rejection::DestinationOccupied) => {
            ("rejected", Some("destination_occupied"))
        }
        Phase::Rejected(Rejection::DestinationChanged) => ("rejected", Some("destination_changed")),
    }
}

fn parse_phase(phase: &str, rejection: Option<&str>) -> Option<Phase> {
    Some(match (phase, rejection) {
        ("planned", None) => Phase::Planned,
        ("publishing", None) => Phase::Publishing,
        ("recorded", None) => Phase::Recorded,
        ("cleaned", None) => Phase::Cleaned,
        ("rejected", Some("source_changed")) => Phase::Rejected(Rejection::SourceChanged),
        ("rejected", Some("destination_occupied")) => {
            Phase::Rejected(Rejection::DestinationOccupied)
        }
        ("rejected", Some("destination_changed")) => Phase::Rejected(Rejection::DestinationChanged),
        _ => return None,
    })
}

fn upsert(connection: &Connection, operation: &Operation, now_ms: i64) -> rusqlite::Result<()> {
    let (phase, rejection) = phase_text(operation.phase);
    connection.execute(
        "INSERT INTO publish_operation (id, source_raw, destination_raw, size, sha256, phase, \
         rejection, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(id) DO UPDATE SET phase = excluded.phase, rejection = excluded.rejection, \
         updated_ms = excluded.updated_ms",
        params![
            i64::try_from(operation.id).unwrap_or(i64::MAX),
            os_str_to_bytes(operation.source.as_os_str()),
            os_str_to_bytes(operation.destination.as_os_str()),
            i64::try_from(operation.expected.size).unwrap_or(i64::MAX),
            operation.expected.sha256.as_slice(),
            phase,
            rejection,
            now_ms,
        ],
    )?;
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or_default()
}

type Row = (i64, Vec<u8>, Vec<u8>, i64, Vec<u8>, String, Option<String>);

fn decode(row: Row) -> Result<Operation> {
    let (id, source, destination, size, sha256, phase, rejection) = row;
    let (Ok(source), Ok(destination)) = (
        bytes_to_os_string(&source),
        bytes_to_os_string(&destination),
    ) else {
        return Err(Error::InvalidStoredValue("publish operation path"));
    };
    Ok(Operation {
        id: u64::try_from(id).map_err(|_| Error::InvalidStoredValue("publish operation id"))?,
        source: source.into(),
        destination: destination.into(),
        expected: Identity {
            size: u64::try_from(size)
                .map_err(|_| Error::InvalidStoredValue("publish operation size"))?,
            sha256: sha256
                .try_into()
                .map_err(|_| Error::InvalidStoredValue("publish operation hash"))?,
        },
        phase: parse_phase(&phase, rejection.as_deref())
            .ok_or(Error::InvalidStoredValue("publish operation phase"))?,
    })
}

impl<'a> PublishStore<'a> {
    pub fn new(database: &'a mut Database) -> Self {
        Self {
            database: Handle::Writer(database),
            on_record: None,
        }
    }

    /// A store that can only look: `get` and `unfinished`.
    #[must_use]
    pub fn new_for_read(database: &'a Database) -> Self {
        Self {
            database: Handle::Reader(database),
            on_record: None,
        }
    }

    /// Make files active in the catalog when an operation is recorded. See [`RecordHook`].
    #[must_use]
    pub fn with_record_hook(
        mut self,
        hook: impl FnMut(&Transaction<'_>, &Operation) -> rusqlite::Result<()> + 'a,
    ) -> Self {
        self.on_record = Some(Box::new(hook));
        self
    }

    /// Fix the identity of a staged file and create its durable `Planned` record.
    pub fn begin(&mut self, source: &Path, destination: &Path) -> Result<Operation> {
        let expected = identity_of(source)?
            .ok_or_else(|| Error::Io(std::io::Error::from(std::io::ErrorKind::NotFound)))?;
        let connection = self.database.connection();
        connection.execute(
            "INSERT INTO publish_operation (source_raw, destination_raw, size, sha256, phase, \
             updated_ms) VALUES (?1, ?2, ?3, ?4, 'planned', ?5)",
            params![
                os_str_to_bytes(source.as_os_str()),
                os_str_to_bytes(destination.as_os_str()),
                i64::try_from(expected.size).unwrap_or(i64::MAX),
                expected.sha256.as_slice(),
                now_ms(),
            ],
        )?;
        Ok(Operation {
            id: u64::try_from(connection.last_insert_rowid())
                .map_err(|_| Error::InvalidStoredValue("publish operation id"))?,
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
            expected,
            phase: Phase::Planned,
        })
    }

    /// One operation by id, whatever its phase.
    pub fn get(&self, id: u64) -> Result<Option<Operation>> {
        let row = self
            .database
            .connection()
            .query_row(
                "SELECT id, source_raw, destination_raw, size, sha256, phase, rejection \
                 FROM publish_operation WHERE id = ?1",
                [i64::try_from(id).unwrap_or(i64::MAX)],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()?;
        row.map(decode).transpose()
    }
}

impl Store for PublishStore<'_> {
    fn save(&mut self, operation: &Operation) -> oc_publish::Result<()> {
        let Handle::Writer(database) = &self.database else {
            return Err(oc_publish::Error::Store("this store is read only".into()));
        };
        upsert(&database.connection, operation, now_ms()).map_err(|e| store_error(&e))
    }

    fn commit_record(&mut self, operation: &Operation) -> oc_publish::Result<()> {
        if operation.phase != Phase::Recorded {
            return Err(oc_publish::Error::Store(
                "commit_record needs the Recorded phase".into(),
            ));
        }
        let Handle::Writer(database) = &mut self.database else {
            return Err(oc_publish::Error::Store("this store is read only".into()));
        };
        let transaction = database
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| store_error(&e))?;
        if let Some(hook) = self.on_record.as_mut() {
            hook(&transaction, operation).map_err(|e| store_error(&e))?;
        }
        upsert(&transaction, operation, now_ms()).map_err(|e| store_error(&e))?;
        transaction.commit().map_err(|e| store_error(&e))
    }

    fn unfinished(&self) -> oc_publish::Result<Vec<Operation>> {
        let mut statement = self
            .database
            .connection()
            .prepare(
                "SELECT id, source_raw, destination_raw, size, sha256, phase, rejection \
                 FROM publish_operation WHERE phase IN ('planned', 'publishing', 'recorded') \
                 ORDER BY id",
            )
            .map_err(|e| store_error(&e))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .map_err(|e| store_error(&e))?;
        let mut operations = Vec::new();
        for row in rows {
            let row = row.map_err(|e| store_error(&e))?;
            operations.push(decode(row).map_err(|e| oc_publish::Error::Store(e.to_string()))?);
        }
        Ok(operations)
    }
}
