# 0001: SQLite stays, on the local disk

Status: proposed 2026-10-22 and recorded in the Game Plan. Pending owner review.

## Context

The owner asked whether SQLite is a good idea for a library of about 500 GB. The audio is not stored in the database, so the question is about the number of tracks and the shape of the workload.

## Decision

Keep SQLite (`rusqlite`) as the app database. It holds metadata only. It always lives in the local app data directory, never on a network share.

- WAL mode, one writer thread, a pool of read connections.
- FTS5 for search. Windowed reads by position for long lists.
- Waveform peaks and thumbnails live in cache files. Fingerprints get their own table.
- A `VACUUM INTO` snapshot before every migration, plus scheduled snapshots, and an integrity check after an unclean exit.
- Migrations are versioned in `PRAGMA user_version`.

## Reasoning

- 500 GB is roughly 8,000 tracks as AIFF or 35,000 as 320 kbps MP3 at six minutes each (assumption A25), which is tens of megabytes of rows.
- rekordbox's own library is SQLite. beets considered Postgres and MySQL and stayed on SQLite ([research 001](../research/001-tag-lookup-options.md)).
- A server is an install and a running service for every DJ. Key-value stores mean writing sorting, filtering and search by hand. DuckDB is for analytics.
- WAL mode does not work over a network filesystem, and a sync tool can copy a database in the middle of a write, so the database stays in the app data directory and out of the synced library folder ([0003](0003-local-library-synced-copy.md)).

## Consequences

- The benchmark in `oc-core-db` measures a synthetic 100,000-track library against the budgets in [0004](0004-interface-and-speed-budgets.md). Results go in a research note (planned).
- `rusqlite` features: plain `bundled` now. When `oc-rekordbox` links rbxport's `rbl-db`, the workspace moves to `bundled-sqlcipher-vendored-openssl`, because Cargo builds one SQLite per binary. The app database stays unencrypted.
- Revisit if the app gains multi-user or networked features, or real libraries pass about a million tracks.
