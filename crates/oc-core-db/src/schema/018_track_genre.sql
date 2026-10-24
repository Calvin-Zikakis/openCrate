-- Each track's genre, as its tags spell it, for genre cleanup. The next scan reads every
-- file's tags again to fill it in. Files are only read.
ALTER TABLE track ADD COLUMN genre TEXT NOT NULL DEFAULT '';
CREATE INDEX track_by_genre ON track(genre);
UPDATE file SET tags_mtime_ms = NULL;
