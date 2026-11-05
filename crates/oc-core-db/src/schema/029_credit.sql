-- Who is credited on each track: its main artists, anyone featured and the remixer, read from
-- the artist field and the title. The track's own artist_id keeps pointing at the whole
-- artist string; these rows are what the Artists screen and an artist's page follow.
-- Re-runnable: a test rolls the version back past it.
-- `standing` settles collaborations at read time: a `half` of "A & B" counts only once that
-- name is an artist on its own (a `sure` credit anywhere), and the `whole` text counts only
-- while its halves do not. `grp` ties a whole to its halves within one track and role.
CREATE TABLE IF NOT EXISTS credit (
    track_id INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
    artist_id INTEGER NOT NULL REFERENCES artist(id),
    role TEXT NOT NULL CHECK (role IN ('artist', 'featured', 'remixer')),
    kind TEXT NOT NULL DEFAULT '',
    standing TEXT NOT NULL CHECK (standing IN ('sure', 'half', 'whole')),
    grp INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL,
    PRIMARY KEY (track_id, artist_id, role)
) STRICT, WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS credit_by_artist ON credit(artist_id, standing);

-- The credits that count. The one place the standing rule lives; every query reads this.
CREATE VIEW IF NOT EXISTS live_credit AS
SELECT c.track_id, c.artist_id, c.role, c.kind, c.position
FROM credit c
WHERE c.standing = 'sure'
   OR (c.standing = 'half'
       AND c.artist_id IN (SELECT s.artist_id FROM credit s WHERE s.standing = 'sure'))
   OR (c.standing = 'whole'
       AND EXISTS (SELECT 1 FROM credit h
                   WHERE h.track_id = c.track_id AND h.role = c.role AND h.grp = c.grp
                     AND h.standing = 'half'
                     AND h.artist_id NOT IN
                         (SELECT s.artist_id FROM credit s WHERE s.standing = 'sure')));
