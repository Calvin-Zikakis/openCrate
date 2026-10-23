-- The owner's own crates: ordered lists of library tracks, kept by openCrate only. They are
-- not rekordbox playlists and are never written to rekordbox. A track leaves a crate when it
-- leaves the library.
CREATE TABLE crate (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    position INTEGER NOT NULL,
    created_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE crate_item (
    crate_id INTEGER NOT NULL REFERENCES crate(id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    added_ms INTEGER NOT NULL,
    PRIMARY KEY (crate_id, track_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX crate_item_order ON crate_item(crate_id, position);
CREATE INDEX crate_item_by_track ON crate_item(track_id);
