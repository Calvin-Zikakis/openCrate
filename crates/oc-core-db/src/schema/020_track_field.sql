-- The owner's own fields on a track (an energy, a mood, anything), kept in openCrate's
-- database rather than in the file's tags.
CREATE TABLE track_field (
    track_id INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (track_id, name)
) STRICT, WITHOUT ROWID;
CREATE INDEX track_field_by_name ON track_field(name);
