-- Files the owner chose to keep out of rekordbox: no longer offered as new. Goes with the file.
CREATE TABLE rekordbox_hidden (
    file_id INTEGER PRIMARY KEY REFERENCES file(id) ON DELETE CASCADE,
    hidden_ms INTEGER NOT NULL
) STRICT;
