CREATE TABLE app_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    clean_shutdown INTEGER NOT NULL CHECK (clean_shutdown IN (0, 1))
) STRICT;

INSERT INTO app_state (id, clean_shutdown) VALUES (1, 1);
