# 0001: SQLite stays, on the local disk

Status: proposed 2026-10-22 and recorded in the Game Plan. Pending owner review.
Recovery wording updated 2026-10-22 by accepted
[decision 0009](0009-reduced-foundation-and-recovery.md). The broader library
architecture below is planned, not fully implemented.

## Context

The owner asked whether SQLite is a good idea for a library of about 500 GB. The audio is not stored in the database, so the question is about the number of tracks and the shape of the workload.

## Decision

Keep SQLite (`rusqlite`) as the app database. It holds no audio. Callers must
choose a private local app-data directory outside synced folders, never a network
share. The lifecycle crate does not detect network filesystems.

- WAL with `synchronous = FULL`. The built crate owns a synchronous connection
  and supplies independent read-only readers. An application writer thread and
  reader pool are future work.
- FTS5 for future search and windowed reads by position for long lists. The
  current test verifies FTS5 availability only, not a library search API.
- Waveform peaks and thumbnails live in cache files. Fingerprints get their own table.
- SQLite's online backup API replaces the earlier `VACUUM INTO` proposal.
  A checked, flushed standalone snapshot precedes each upgrade of an existing
  schema, including committed WAL data. Initial version-0 creation has no
  snapshot. Scheduling and retention are later work.
- Restore validates a standalone snapshot and writes a new database in an unused
  local app-data directory. It never overwrites or selects the restored database
  for the caller. Creation and restore refuse orphan journals.
- A cooperative owner lock and application/schema checks guard opening an
  existing database. Explicit close marks clean; drop or crash leaves dirty.
  A dirty reopen runs integrity and foreign-key checks.
- Migrations and `PRAGMA user_version` updates commit together.

Current schema v1 contains only `app_state`. Library tables, caches, queries,
pooling and the benchmark remain unimplemented. See
[oc-core-db](../../crates/oc-core-db/README.md) for the public API and limits.

## Reasoning

- 500 GB is roughly 8,000 tracks as AIFF or 35,000 as 320 kbps MP3 at six minutes each (assumption A25), which is tens of megabytes of rows.
- rekordbox's own library is SQLite. beets considered Postgres and MySQL and stayed on SQLite ([research 001](../research/001-tag-lookup-options.md)).
- A server is an install and a running service for every DJ. Key-value stores mean writing sorting, filtering and search by hand. DuckDB is for analytics.
- WAL mode does not work over a network filesystem, and a sync tool can copy a database in the middle of a write, so the database stays in the app data directory and out of the synced library folder ([0003](0003-local-library-synced-copy.md)).

## Consequences

- A future benchmark will use a synthetic 100,000-track library. Backend
  allocations within the user-visible budgets in [0004](0004-interface-and-speed-budgets.md)
  still need definition. [Research 003](../research/003-reduced-foundation-validation.md)
  records lifecycle/codec correctness checks, not performance.
- `rusqlite` features are `bundled` and `backup` now. When `oc-rekordbox` links
  rbxport's `rbl-db`, the planned workspace build uses
  `bundled-sqlcipher-vendored-openssl`, because Cargo builds one SQLite per binary.
  The app database stays unencrypted; compatibility and packaging still need tests.
- Revisit if the app gains multi-user or networked features, or real libraries pass about a million tracks.
