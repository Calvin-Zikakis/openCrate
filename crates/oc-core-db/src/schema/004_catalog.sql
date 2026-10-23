-- Tags are read once per file version: tags_mtime_ms is the modified time the tags were read
-- at (NULL until read), so a changed file is picked up again. One artist per folded name,
-- one release per artist and folded title.

ALTER TABLE file ADD COLUMN tags_mtime_ms INTEGER;
ALTER TABLE file ADD COLUMN tags_error TEXT;

CREATE UNIQUE INDEX artist_by_key ON artist(sort_key);
CREATE UNIQUE INDEX release_by_title ON release(artist_id, sort_key);
