-- Library roots and files follow the path identity contract: raw path bytes are
-- stored (NUL-joined components), path_key is the derived per-root lookup key.

CREATE TABLE library_root (
    id INTEGER PRIMARY KEY,
    path_raw BLOB NOT NULL UNIQUE,
    label TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('local', 'removable', 'network')),
    name_profile TEXT NOT NULL CHECK (name_profile IN ('exact', 'fold_case', 'fold_case_normalized')),
    name_profile_source TEXT NOT NULL CHECK (name_profile_source IN ('os_query', 'stat_probe', 'user_set', 'default')),
    key_scheme INTEGER NOT NULL,
    dir_token BLOB,
    volume_token BLOB,
    availability TEXT NOT NULL CHECK (availability IN ('available', 'unavailable', 'unverified')),
    availability_reason TEXT NOT NULL DEFAULT '',
    availability_checked_ms INTEGER,
    last_available_ms INTEGER,
    created_ms INTEGER NOT NULL
) STRICT;

-- shape is 'posix', 'unc' or 'drive:X'; components is NUL-joined, empty for a bare drive.
CREATE TABLE root_alias (
    id INTEGER PRIMARY KEY,
    root_id INTEGER NOT NULL REFERENCES library_root(id),
    shape TEXT NOT NULL,
    components BLOB NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('root_path', 'user')),
    added_ms INTEGER NOT NULL,
    UNIQUE (root_id, shape, components)
) STRICT;

CREATE TABLE artist (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    sort_key TEXT NOT NULL
) STRICT;
CREATE INDEX artist_sort ON artist(sort_key, id);

CREATE TABLE release (
    id INTEGER PRIMARY KEY,
    artist_id INTEGER NOT NULL REFERENCES artist(id),
    title TEXT NOT NULL,
    sort_key TEXT NOT NULL,
    artist_key TEXT NOT NULL,
    year INTEGER,
    label TEXT NOT NULL DEFAULT '',
    added_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX release_recent ON release(added_ms DESC, id DESC);
CREATE INDEX release_title ON release(sort_key, id);
CREATE INDEX release_artist_order ON release(artist_key, sort_key, id);
CREATE INDEX release_by_artist ON release(artist_id);

CREATE TABLE track (
    id INTEGER PRIMARY KEY,
    release_id INTEGER NOT NULL REFERENCES release(id),
    artist_id INTEGER NOT NULL REFERENCES artist(id),
    disc_no INTEGER NOT NULL DEFAULT 1,
    track_no INTEGER NOT NULL DEFAULT 0,
    title TEXT NOT NULL,
    title_key TEXT NOT NULL,
    artist_key TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '',
    duration_ms INTEGER NOT NULL DEFAULT 0,
    bpm REAL,
    musical_key TEXT NOT NULL DEFAULT '',
    added_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX track_recent ON track(added_ms DESC, id DESC);
CREATE INDEX track_title ON track(title_key, id);
CREATE INDEX track_artist_order ON track(artist_key, release_id, disc_no, track_no, id);
CREATE INDEX track_bpm ON track(bpm IS NULL, bpm, id);
CREATE INDEX track_by_release ON track(release_id, disc_no, track_no);
CREATE INDEX track_by_artist ON track(artist_id);

-- Offline is derived from the root; a file carries lifecycle and presence only.
CREATE TABLE file (
    id INTEGER PRIMARY KEY,
    track_id INTEGER REFERENCES track(id),
    root_id INTEGER NOT NULL REFERENCES library_root(id),
    path_raw BLOB NOT NULL,
    path_key BLOB NOT NULL,
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('staged', 'active', 'archived')),
    presence TEXT NOT NULL CHECK (presence IN ('present', 'missing')),
    size INTEGER NOT NULL,
    mtime_ms INTEGER NOT NULL,
    container TEXT NOT NULL,
    codec TEXT NOT NULL DEFAULT '',
    bitrate_kbps INTEGER,
    sample_rate INTEGER,
    bit_depth INTEGER,
    channels INTEGER,
    duration_ms INTEGER,
    file_token BLOB,
    last_seen_ms INTEGER NOT NULL,
    missing_since_ms INTEGER,
    created_ms INTEGER NOT NULL,
    UNIQUE (root_id, path_key),
    CHECK ((presence = 'missing') = (missing_since_ms IS NOT NULL))
) STRICT;
CREATE INDEX file_by_track ON file(track_id, lifecycle, container);

-- Rowids equal track.id, release.id and artist.id; the code that writes those
-- rows keeps these tables in step. Matching is on word prefixes, ignoring accents.
CREATE VIRTUAL TABLE track_fts USING fts5(
    title, artist, release, version,
    tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3'
);
CREATE VIRTUAL TABLE release_fts USING fts5(
    title, artist,
    tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3'
);
CREATE VIRTUAL TABLE artist_fts USING fts5(
    name,
    tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3'
);
