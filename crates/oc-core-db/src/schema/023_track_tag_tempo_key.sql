-- The BPM and key each file's tags give, kept beside rekordbox's own (`bpm`, `musical_key`), which
-- only exist once rekordbox has analysed a track. The library shows rekordbox's when it has them
-- and the tags' until then. Filled on the next scan, which reads every file's tags again; files are
-- only read.
ALTER TABLE track ADD COLUMN tag_bpm REAL;
ALTER TABLE track ADD COLUMN tag_key TEXT NOT NULL DEFAULT '';
UPDATE file SET tags_mtime_ms = NULL;
-- Sorting by BPM orders by the value shown, so the index follows it.
DROP INDEX track_bpm;
CREATE INDEX track_bpm ON track(COALESCE(bpm, tag_bpm) IS NULL, COALESCE(bpm, tag_bpm), id);
