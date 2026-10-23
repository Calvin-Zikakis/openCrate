-- Waveform overviews, one byte per slice, computed once per version of a file (Game Plan:
-- peaks are stored and never recomputed by the player). A changed file gets new peaks.
CREATE TABLE waveform (
    file_id INTEGER PRIMARY KEY REFERENCES file(id) ON DELETE CASCADE,
    size INTEGER NOT NULL,
    mtime_ms INTEGER NOT NULL,
    peaks BLOB NOT NULL
) STRICT;
