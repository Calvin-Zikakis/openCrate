-- Each track's own year and label, as its tags give them. rekordbox keeps both per track and
-- the editor writes them per track; the release's values only held what its first file said, so
-- a later edit of either never reached the comparison with rekordbox. Filled from the release now,
-- and exactly from each file's tags on the next scan. Files are only read.
ALTER TABLE track ADD COLUMN year INTEGER;
ALTER TABLE track ADD COLUMN label TEXT NOT NULL DEFAULT '';
UPDATE track SET year = (SELECT year FROM release WHERE release.id = track.release_id),
                 label = (SELECT label FROM release WHERE release.id = track.release_id);
UPDATE file SET tags_mtime_ms = NULL;
