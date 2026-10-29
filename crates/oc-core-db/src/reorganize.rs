//! Sittings of the Reorganize tool: which files moved where, so any sitting can be put back.

use rusqlite::{params, Connection, TransactionBehavior};

use crate::{Database, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReorganizeMove {
    pub file_id: i64,
    pub root_id: i64,
    /// Inside the music folder, "/"-separated raw bytes.
    pub from_raw: Vec<u8>,
    pub to_raw: Vec<u8>,
}

/// A move as written down: its id, and whether it was put back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedMove {
    pub id: i64,
    pub session_id: i64,
    pub put_back: bool,
    pub item: ReorganizeMove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReorganizeSession {
    pub id: i64,
    pub started_ms: i64,
    /// Set once every move of the sitting is put back.
    pub undone_ms: Option<i64>,
    pub moves: i64,
    /// Moves put back so far.
    pub put_back: i64,
}

const SESSION_COLUMNS: &str = "SELECT s.id, s.started_ms, s.undone_ms, \
     (SELECT COUNT(*) FROM reorganize_move m WHERE m.session_id = s.id), \
     (SELECT COUNT(*) FROM reorganize_move m JOIN reorganize_move_undone u ON u.move_id = m.id \
      WHERE m.session_id = s.id) \
     FROM reorganize_session s";

fn session_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReorganizeSession> {
    Ok(ReorganizeSession {
        id: row.get(0)?,
        started_ms: row.get(1)?,
        undone_ms: row.get(2)?,
        moves: row.get(3)?,
        put_back: row.get(4)?,
    })
}

/// The newest sittings, newest first, at most `limit`.
pub fn sessions(connection: &Connection, limit: usize) -> Result<Vec<ReorganizeSession>> {
    let mut statement =
        connection.prepare(&format!("{SESSION_COLUMNS} ORDER BY s.id DESC LIMIT ?1"))?;
    let rows = statement.query_map([i64::try_from(limit).unwrap_or(i64::MAX)], session_of)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// One sitting, if it exists.
pub fn session(connection: &Connection, session_id: i64) -> Result<Option<ReorganizeSession>> {
    let mut statement = connection.prepare(&format!("{SESSION_COLUMNS} WHERE s.id = ?1"))?;
    let mut rows = statement.query_map([session_id], session_of)?;
    Ok(rows.next().transpose()?)
}

fn recorded(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecordedMove> {
    Ok(RecordedMove {
        id: row.get(0)?,
        session_id: row.get(1)?,
        put_back: row.get(2)?,
        item: ReorganizeMove {
            file_id: row.get(3)?,
            root_id: row.get(4)?,
            from_raw: row.get(5)?,
            to_raw: row.get(6)?,
        },
    })
}

const MOVE_COLUMNS: &str =
    "SELECT m.id, m.session_id, u.move_id IS NOT NULL, m.file_id, m.root_id, \
     m.from_raw, m.to_raw FROM reorganize_move m \
     LEFT JOIN reorganize_move_undone u ON u.move_id = m.id";

/// A sitting's moves, newest first: the order to put them back in.
pub fn session_moves(connection: &Connection, session_id: i64) -> Result<Vec<RecordedMove>> {
    let mut statement = connection.prepare(&format!(
        "{MOVE_COLUMNS} WHERE m.session_id = ?1 ORDER BY m.id DESC"
    ))?;
    let rows = statement.query_map([session_id], recorded)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Moves of later sittings, not put back, of these files: why a file stays where it is.
pub fn later_moves(
    connection: &Connection,
    session_id: i64,
    file_id: i64,
) -> Result<Vec<RecordedMove>> {
    let mut statement = connection.prepare(&format!(
        "{MOVE_COLUMNS} WHERE m.session_id > ?1 AND m.file_id = ?2 AND u.move_id IS NULL ORDER BY m.id"
    ))?;
    let rows = statement.query_map([session_id, file_id], recorded)?;
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

    /// Moves of a sitting were put back. Once every one is, the sitting counts as put back.
    pub fn mark_reorganize_undone(
        &mut self,
        session_id: i64,
        move_ids: &[i64],
        now_ms: i64,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for move_id in move_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO reorganize_move_undone (move_id, undone_ms) \
                 SELECT id, ?2 FROM reorganize_move WHERE id = ?1 AND session_id = ?3",
                params![move_id, now_ms, session_id],
            )?;
        }
        transaction.execute(
            "UPDATE reorganize_session SET undone_ms = ?2 WHERE id = ?1 AND undone_ms IS NULL \
             AND NOT EXISTS (SELECT 1 FROM reorganize_move m \
               LEFT JOIN reorganize_move_undone u ON u.move_id = m.id \
               WHERE m.session_id = ?1 AND u.move_id IS NULL)",
            params![session_id, now_ms],
        )?;
        transaction.commit()?;
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
    fn sittings_are_put_back_move_by_move_and_count_as_put_back_once_all_are() {
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
        assert_eq!(sessions(&database.connection, 10).unwrap(), Vec::new());
        let moving = |file_id: i64, from: &str, to: &str| ReorganizeMove {
            file_id,
            root_id,
            from_raw: from.as_bytes().to_vec(),
            to_raw: to.as_bytes().to_vec(),
        };
        let first = database.begin_reorganize(1_000).unwrap();
        database
            .record_reorganize(first, &[moving(1, "a/x.aif", "b/x.aif")], 1_001)
            .unwrap();
        database
            .record_reorganize(first, &[moving(2, "a/y.aif", "b/y.aif")], 1_002)
            .unwrap();
        let second = database.begin_reorganize(3_000).unwrap();
        database
            .record_reorganize(second, &[moving(1, "b/x.aif", "c/x.aif")], 3_001)
            .unwrap();

        let listed = sessions(&database.connection, 10).unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|item| (item.id, item.moves, item.put_back))
                .collect::<Vec<_>>(),
            [(second, 1, 0), (first, 2, 0)],
            "newest first"
        );
        let moves = session_moves(&database.connection, first).unwrap();
        assert_eq!(
            moves
                .iter()
                .map(|item| item.item.file_id)
                .collect::<Vec<_>>(),
            [2, 1],
            "newest first"
        );
        assert_eq!(moves[1].item.from_raw, b"a/x.aif");
        // File 1 was moved again by the second sitting; file 2 was not.
        assert_eq!(
            later_moves(&database.connection, first, 1).unwrap().len(),
            1
        );
        assert_eq!(
            later_moves(&database.connection, first, 2).unwrap(),
            Vec::new()
        );

        // The first sitting put back partly: file 2 only. It is not put back yet.
        database
            .mark_reorganize_undone(first, &[moves[0].id], 4_000)
            .unwrap();
        let partly = session(&database.connection, first).unwrap().unwrap();
        assert_eq!((partly.put_back, partly.undone_ms), (1, None));
        // A move of another sitting is not taken as this one's.
        let later = session_moves(&database.connection, second).unwrap();
        database
            .mark_reorganize_undone(first, &[later[0].id], 4_001)
            .unwrap();
        assert_eq!(
            session(&database.connection, second)
                .unwrap()
                .unwrap()
                .put_back,
            0
        );
        // The second sitting put back frees file 1; then the first can finish.
        database
            .mark_reorganize_undone(second, &[later[0].id], 5_000)
            .unwrap();
        assert_eq!(
            later_moves(&database.connection, first, 1).unwrap(),
            Vec::new()
        );
        assert_eq!(
            session(&database.connection, second)
                .unwrap()
                .unwrap()
                .undone_ms,
            Some(5_000)
        );
        database
            .mark_reorganize_undone(first, &[moves[1].id], 6_000)
            .unwrap();
        let done = session(&database.connection, first).unwrap().unwrap();
        assert_eq!((done.put_back, done.undone_ms), (2, Some(6_000)));
        assert!(session_moves(&database.connection, first)
            .unwrap()
            .iter()
            .all(|item| item.put_back));
        assert_eq!(session(&database.connection, 99).unwrap(), None);
    }
}
