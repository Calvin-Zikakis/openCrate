-- The durable record of each publication from staging into the library (review finding R1).
-- One row per file, written before the external effect it announces, so a restart can finish
-- or reject it from the row and the disk together. Paths are raw bytes, as everywhere else.
CREATE TABLE publish_operation (
    id INTEGER PRIMARY KEY,
    source_raw BLOB NOT NULL,
    destination_raw BLOB NOT NULL,
    size INTEGER NOT NULL,
    sha256 BLOB NOT NULL CHECK (length(sha256) = 32),
    phase TEXT NOT NULL CHECK (phase IN ('planned', 'publishing', 'recorded', 'cleaned', 'rejected')),
    rejection TEXT CHECK (rejection IN ('source_changed', 'destination_occupied', 'destination_changed')),
    updated_ms INTEGER NOT NULL,
    CHECK ((phase = 'rejected') = (rejection IS NOT NULL))
) STRICT;
CREATE INDEX publish_operation_unfinished ON publish_operation(id)
    WHERE phase IN ('planned', 'publishing', 'recorded');
