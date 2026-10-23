-- Releases from a download folder that have been imported, by their top-level name as raw
-- bytes. The originals stay in the download folder; this keeps them off the "new" list.
CREATE TABLE imported_release (
    watch_folder_id INTEGER NOT NULL REFERENCES watch_folder(id),
    name BLOB NOT NULL,
    files INTEGER NOT NULL,
    imported_ms INTEGER NOT NULL,
    PRIMARY KEY (watch_folder_id, name)
) STRICT, WITHOUT ROWID;
