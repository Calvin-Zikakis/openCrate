-- Releases the owner took off the Import list without importing them (already in the library,
-- or not wanted), by their top-level name as raw bytes. They stay in the watch folder and come
-- back with "Put back". Re-runnable: a test rolls the version back past it.
CREATE TABLE IF NOT EXISTS dismissed_release (
    watch_folder_id INTEGER NOT NULL REFERENCES watch_folder(id),
    name BLOB NOT NULL,
    dismissed_ms INTEGER NOT NULL,
    PRIMARY KEY (watch_folder_id, name)
) STRICT, WITHOUT ROWID;
