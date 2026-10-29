-- Every file openCrate files into a music folder is remembered, grouped into batches: an import
-- that starts within a quarter of an hour of the previous one joins its batch, so "Recently
-- imported" shows one sitting's worth. A file is tied to its track through the file table by
-- path once the folder has been scanned. Re-runnable: a test rolls the version back past it.
CREATE TABLE IF NOT EXISTS import_batch (
    id INTEGER PRIMARY KEY,
    started_ms INTEGER NOT NULL,
    last_ms INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS import_file (
    id INTEGER PRIMARY KEY,
    batch_id INTEGER NOT NULL REFERENCES import_batch(id),
    root_id INTEGER NOT NULL REFERENCES library_root(id),
    path_key BLOB NOT NULL,
    imported_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS import_file_by_batch ON import_file(batch_id);
CREATE INDEX IF NOT EXISTS import_file_by_path ON import_file(root_id, path_key);
