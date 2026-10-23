-- The owner's own order for the whole track list ("Custom order"), kept by openCrate only.
-- Tracks without a row (added later) follow the arranged ones, newest first.
CREATE TABLE library_order (
    track_id INTEGER PRIMARY KEY REFERENCES track(id) ON DELETE CASCADE,
    position INTEGER NOT NULL
) STRICT;
CREATE INDEX library_order_position ON library_order(position);
