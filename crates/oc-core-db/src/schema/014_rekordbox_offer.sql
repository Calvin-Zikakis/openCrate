-- Files openCrate offered to rekordbox in its XML file, kept until a read shows rekordbox has
-- them (offered is not confirmed). Goes with the file.
CREATE TABLE rekordbox_offer (
    file_id INTEGER PRIMARY KEY REFERENCES file(id) ON DELETE CASCADE,
    offered_ms INTEGER NOT NULL
) STRICT;
