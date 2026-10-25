-- Moved files pairs a missing file with present ones of the same size: an index keeps that from
-- reading every file once per missing file in a large library.
CREATE INDEX IF NOT EXISTS file_by_size ON file(size);
