-- What a download folder held when it was added: its top-level names (release folders and
-- loose files), as raw bytes. A release is "already there" only if its name is in this list;
-- modified times cannot decide that, because copying a file keeps its old time.
CREATE TABLE watch_baseline (
    watch_folder_id INTEGER NOT NULL REFERENCES watch_folder(id),
    name BLOB NOT NULL,
    PRIMARY KEY (watch_folder_id, name)
) STRICT, WITHOUT ROWID;
ALTER TABLE watch_folder ADD COLUMN baseline_taken INTEGER NOT NULL DEFAULT 0 CHECK (baseline_taken IN (0, 1));
