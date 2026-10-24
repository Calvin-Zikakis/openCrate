-- The base of the merge with rekordbox: each synced field of each track rekordbox holds, as
-- both sides last agreed on it. A side that differs from it has changed since.
CREATE TABLE merge_base (
    rb_key INTEGER NOT NULL,
    field TEXT NOT NULL CHECK (field IN ('title', 'artist', 'album', 'year', 'label')),
    value TEXT NOT NULL,
    agreed_ms INTEGER NOT NULL,
    PRIMARY KEY (rb_key, field)
) STRICT, WITHOUT ROWID;
