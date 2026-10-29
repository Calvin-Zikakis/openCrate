-- Reorganizing the library (an experimental Clean up tool) moves files by the folder layout in
-- sittings. Every move is written down with where the file came from, so the last sitting can be
-- put back. Paths are relative to the music folder, as raw bytes. Re-runnable: a test rolls the
-- version back past it.
CREATE TABLE IF NOT EXISTS reorganize_session (
    id INTEGER PRIMARY KEY,
    started_ms INTEGER NOT NULL,
    undone_ms INTEGER
) STRICT;
CREATE TABLE IF NOT EXISTS reorganize_move (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES reorganize_session(id),
    file_id INTEGER NOT NULL,
    root_id INTEGER NOT NULL REFERENCES library_root(id),
    from_raw BLOB NOT NULL,
    to_raw BLOB NOT NULL,
    moved_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS reorganize_move_by_session ON reorganize_move(session_id);
