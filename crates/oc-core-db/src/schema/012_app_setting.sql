-- Library-wide choices that must survive a restart and apply on every computer that opens this
-- library, such as how filed music is laid out in folders. Values are text; the app validates.
CREATE TABLE app_setting (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
