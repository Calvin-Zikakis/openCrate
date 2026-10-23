-- Download folders from Settings. Not library roots: openCrate only reads them, and no
-- file row ever points into one. An unreachable folder is online = 0, never deleted.
CREATE TABLE watch_folder (
    id INTEGER PRIMARY KEY,
    path_raw BLOB NOT NULL UNIQUE,
    label TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    online INTEGER NOT NULL DEFAULT 1 CHECK (online IN (0, 1)),
    checked_ms INTEGER,
    last_seen_ms INTEGER,
    added_ms INTEGER NOT NULL
) STRICT;
