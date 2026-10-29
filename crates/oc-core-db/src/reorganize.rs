//! Sittings of the Reorganize tool: which files moved where, so the last sitting can be put back.

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::{Database, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReorganizeMove {
    pub file_id: i64,
    pub root_id: i64,
    /// Inside the music folder, "/"-separated raw bytes.
    pub from_raw: Vec<u8>,
    pub to_raw: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReorganizeSession {
    pub id: i64,
    pub started_ms: i64,
    pub undone_ms: Option<i64>,
    pub moves: i64,
}

/// The newest sitting, with how many moves it holds.
pub fn last_session(connection: &Connection) -> Result<Option<ReorganizeSession>> {
    Ok(connection
        .query_row(
            "SELECT s.id, s.started_ms, s.undone_ms, \
             (SELECT COUNT(*) FROM reorganize_move m WHERE m.session_id = s.id) \
             FROM reorganize_session s ORDER BY s.id DESC LIMIT 1",
            [],
            |row| {
                Ok(ReorganizeSession {
                    id: row.get(0)?,
                    started_ms: row.get(1)?,
                    undone_ms: row.get(2)?,
                    moves: row.get(3)?,
                })
            },
        )
        .optional()?)
}

/// A sitting's moves, newest first: the order to put them back in.
pub fn session_moves(connection: &Connection, session_id: i64) -> Result<Vec<ReorganizeMove>> {
    let mut statement = connection.prepare(
        "SELECT file_id, root_id, from_raw, to_raw FROM reorganize_move \
         WHERE session_id = ?1 ORDER BY id DESC",
    )?;
    let rows = statement.query_map([session_id], |row| {
        Ok(ReorganizeMove {
            file_id: row.get(0)?,
            root_id: row.get(1)?,
            from_raw: row.get(2)?,
            to_raw: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

impl Database {
    /// Start a sitting.
    pub fn begin_reorganize(&mut self, now_ms: i64) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO reorganize_session (started_ms) VALUES (?1)",
            [now_ms],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Write down moves made in a sitting.
    pub fn record_reorganize(
        &mut self,
        session_id: i64,
        moves: &[ReorganizeMove],
        now_ms: i64,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for item in moves {
            transaction.execute(
                "INSERT INTO reorganize_move (session_id, file_id, root_id, from_raw, to_raw, moved_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session_id, item.file_id, item.root_id, item.from_raw, item.to_raw, now_ms],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// A sitting was put back.
    pub fn mark_reorganize_undone(&mut self, session_id: i64, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE reorganize_session SET undone_ms = ?2 WHERE id = ?1",
            params![session_id, now_ms],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::roots::{NewRoot, ProfileSource, RootKind};
    use crate::Database;

    #[test]
    fn sessions_record_moves_newest_first_and_remember_being_undone() {
        let directory = tempdir().unwrap();
        let mut database = Database::open(directory.path()).unwrap();
        let music = tempdir().unwrap();
        let root_path = oc_paths::check_new_root(music.path(), &[], &[]).unwrap();
        let root_id = database
            .add_root(
                &NewRoot {
                    path: &root_path,
                    label: "Music",
                    kind: RootKind::Local,
                    profile: oc_paths::probe_root(&root_path, 1_000).unwrap().profile(),
                    profile_source: ProfileSource::StatProbe,
                    dir_token: oc_paths::FileId::of(&root_path).unwrap().persistent_token(),
                    volume_token: None,
                },
                1,
            )
            .unwrap();
        assert_eq!(last_session(&database.connection).unwrap(), None);
        let first = database.begin_reorganize(1_000).unwrap();
        let moving = |file_id: i64, from: &str, to: &str| ReorganizeMove {
            file_id,
            root_id,
            from_raw: from.as_bytes().to_vec(),
            to_raw: to.as_bytes().to_vec(),
        };
        database
            .record_reorganize(first, &[moving(1, "a/x.aif", "b/x.aif")], 1_001)
            .unwrap();
        database
            .record_reorganize(first, &[moving(2, "a/y.aif", "b/y.aif")], 1_002)
            .unwrap();
        let session = last_session(&database.connection).unwrap().unwrap();
        assert_eq!(
            (
                session.id,
                session.started_ms,
                session.moves,
                session.undone_ms
            ),
            (first, 1_000, 2, None)
        );
        let moves = session_moves(&database.connection, first).unwrap();
        assert_eq!(
            moves.iter().map(|item| item.file_id).collect::<Vec<_>>(),
            [2, 1],
            "newest first"
        );
        assert_eq!(moves[1].from_raw, b"a/x.aif");
        database.mark_reorganize_undone(first, 2_000).unwrap();
        assert_eq!(
            last_session(&database.connection)
                .unwrap()
                .unwrap()
                .undone_ms,
            Some(2_000)
        );
        let second = database.begin_reorganize(3_000).unwrap();
        assert!(second > first);
        let session = last_session(&database.connection).unwrap().unwrap();
        assert_eq!((session.id, session.moves), (second, 0));
    }
}
