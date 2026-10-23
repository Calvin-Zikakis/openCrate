-- A read-only mirror of a rekordbox export. Each source is replaced wholesale in one
-- transaction and carries a generation; nothing here is ever edited by the user.

CREATE TABLE rb_source (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('xml', 'master_db')),
    path_raw BLOB NOT NULL,
    file_mtime_ms INTEGER,
    generation INTEGER NOT NULL DEFAULT 0,
    entries INTEGER NOT NULL DEFAULT 0,
    last_read_ms INTEGER,
    last_attempt_ms INTEGER,
    last_error TEXT,
    state_digest INTEGER,
    export_attributes TEXT NOT NULL DEFAULT '{}',
    UNIQUE (kind, path_raw)
) STRICT;

-- rb_key is the export's TrackID: scoped to the source, never a global identity.
CREATE TABLE rb_track (
    id INTEGER PRIMARY KEY,
    rb_source_id INTEGER NOT NULL REFERENCES rb_source(id),
    rb_key INTEGER NOT NULL,
    location_raw BLOB NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    artist TEXT NOT NULL DEFAULT '',
    bpm REAL,
    musical_key TEXT NOT NULL DEFAULT '',
    rating INTEGER,
    attributes TEXT NOT NULL DEFAULT '{}',
    match_status TEXT NOT NULL CHECK (match_status IN
        ('matched', 'unmapped', 'not_indexed', 'ambiguous', 'unrecognized')),
    match_kind TEXT CHECK (match_kind IN ('exact_bytes', 'key_equal', 'volume_resolved')),
    root_id INTEGER REFERENCES library_root(id),
    file_id INTEGER REFERENCES file(id),
    UNIQUE (rb_source_id, rb_key),
    CHECK ((match_status = 'matched') = (file_id IS NOT NULL))
) STRICT;
CREATE INDEX rb_track_by_file ON rb_track(file_id);
CREATE INDEX rb_track_by_status ON rb_track(rb_source_id, match_status);

-- Times stay fractional seconds as rekordbox wrote them; nothing is rounded to milliseconds.
CREATE TABLE rb_cue (
    rb_track_id INTEGER NOT NULL REFERENCES rb_track(id),
    position INTEGER NOT NULL,
    kind INTEGER NOT NULL,
    number INTEGER,
    start_seconds REAL NOT NULL,
    end_seconds REAL,
    name TEXT NOT NULL DEFAULT '',
    attributes TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (rb_track_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE rb_tempo (
    rb_track_id INTEGER NOT NULL REFERENCES rb_track(id),
    position INTEGER NOT NULL,
    start_seconds REAL NOT NULL,
    bpm REAL NOT NULL,
    meter TEXT,
    beat INTEGER,
    attributes TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (rb_track_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE playlist (
    id INTEGER PRIMARY KEY,
    rb_source_id INTEGER NOT NULL REFERENCES rb_source(id),
    parent_id INTEGER REFERENCES playlist(id),
    kind TEXT NOT NULL CHECK (kind IN ('folder', 'playlist')),
    name TEXT NOT NULL,
    position INTEGER NOT NULL,
    attributes TEXT NOT NULL DEFAULT '{}'
) STRICT;
CREATE INDEX playlist_by_parent ON playlist(rb_source_id, parent_id, position);

-- rb_key points at a TrackID of the same source; an ID missing from the export stays here
-- as an unresolved reference, not a missing file. Duplicates are kept.
CREATE TABLE playlist_item (
    playlist_id INTEGER NOT NULL REFERENCES playlist(id),
    position INTEGER NOT NULL,
    rb_key INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, position)
) STRICT, WITHOUT ROWID;
CREATE INDEX playlist_item_by_key ON playlist_item(rb_key);

-- Where a track's tempo and key came from, so an old value is never shown as current.
ALTER TABLE track ADD COLUMN derived_source_id INTEGER REFERENCES rb_source(id);
ALTER TABLE track ADD COLUMN derived_generation INTEGER;
