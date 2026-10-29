-- Any Reorganize sitting can be put back, not only the last, and partly: a file a later sitting
-- moved again stays until that one is put back. Each move put back is written down here, so a
-- sitting can be finished later. Re-runnable: a test rolls the version back past it.
CREATE TABLE IF NOT EXISTS reorganize_move_undone (
    move_id INTEGER PRIMARY KEY REFERENCES reorganize_move(id),
    undone_ms INTEGER NOT NULL
) STRICT;
