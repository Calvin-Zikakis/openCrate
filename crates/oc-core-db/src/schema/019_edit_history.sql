-- Edits made in openCrate's track editor, newest last, so the last save can be undone. Each
-- item holds the value a track's file had before and the value written.
CREATE TABLE edit_history (
    id INTEGER PRIMARY KEY,
    saved_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE edit_history_item (
    history_id INTEGER NOT NULL REFERENCES edit_history(id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL,
    field TEXT NOT NULL,
    before TEXT NOT NULL,
    after TEXT NOT NULL,
    PRIMARY KEY (history_id, track_id, field)
) STRICT, WITHOUT ROWID;
