# oc-core-db

Synchronous SQLite lifecycle and recovery for openCrate. Schema v1 contains only
`app_state`, a single clean-shutdown marker. There is no library schema/query
API, writer worker or reader pool. The workspace uses bundled SQLite with the
online backup feature, not SQLCipher.

## Public API

| Item | Purpose |
| --- | --- |
| `Database::open(&Path) -> Result<Database>` | Own an app-data directory and open or create `library.sqlite3` |
| `Database::status() -> OpenStatus` | Report `Created`, `Clean` or `CheckedAfterUncleanShutdown` for this open |
| `Database::reader() -> Result<rusqlite::Connection>` | Open an independent read-only, query-only connection |
| `Database::snapshot() -> Result<PathBuf>` | Write a checked standalone snapshot under `snapshots/` |
| `Database::close(self) -> Result<()>` | Mark clean and explicitly close the owner connection |
| `restore(&Path, &Path) -> Result<PathBuf>` | Restore a standalone snapshot into an unused app-data directory; return the new database path |
| `Error`, `Result<T>` | Typed lifecycle, SQLite and filesystem failures |

Move the owner to an application worker when one exists. These calls block and
must not run on the UI thread. The writer connection and migration runner are
private; this is not a general-purpose database write API.

## Safety and limits

- The caller must supply a private, local app-data directory outside any synced
  library. The crate does not detect network filesystems or sync software.
- The directory is canonicalized. A cooperative exclusive OS lock on
  `owner.lock`, using `std::fs::File::try_lock`, prevents another cooperating owner.
  It does not stop unrelated software from writing SQLite files. Database and
  lock files must be regular files, not symlinks.
- The lock is released by an explicit unlock when the owner (or a failed open, or
  `restore`) drops, not by the descriptor closing. A child process spawned by
  another thread holds a copy of the descriptor until it execs, which would
  otherwise keep the directory locked ([decision 0012](../../docs/decisions/0012-explicit-owner-lock-release.md)).
  Verified on macOS only.
- Existing databases are probed read-only for application ID, schema version and
  lifecycle state before opening for writes. Foreign, unsupported future and
  invalid databases are refused. An existing empty SQLite file is not adopted.
- The owner enables WAL, `synchronous = FULL`, foreign keys and a two-second busy
  timeout. A dirty reopen runs integrity and foreign-key checks. Successful open
  marks dirty; explicit close marks clean. Ordinary drop does not mark clean.
- Snapshots use SQLite's online backup API, including committed WAL data. They
  use standalone rollback-journal mode, pass integrity checks, and are flushed
  and published without overwriting. Parent-directory flushing is Unix-only.
- A snapshot precedes each upgrade of an existing schema. Initial creation from
  version 0 has no snapshot. Each migration and its application ID/version
  changes commit in one transaction. Only the v1 migration ships today.
- Restore requires a standalone snapshot, not an arbitrary copy of a live
  database missing its WAL. It validates the source and marks the new copy clean.
  It never replaces an existing database or switches the application to it.
  The caller must explicitly select the restored directory.
- The destination directory may already exist, but must be unused for the app
  database. Creation and restore refuse orphan `-wal`, `-shm` or `-journal`
  files and preserve them. Do not delete recovery material to bypass refusal.
- There is no automatic restore, snapshot retention, scheduling or performance
  benchmark. These tests do not establish power-loss durability on every OS.

## Validation

From the workspace root:

```sh
cargo test -p oc-core-db --locked
cargo clippy -p oc-core-db --all-targets --locked -- -D warnings
```

The reported `cargo test --workspace --locked` run on arm64 macOS with Rust
1.99.0 passed 20 database tests, including one child-process helper. Fixtures
are synthetic temporary databases, not a music library. They cover ownership,
refusal paths, read-only readers, read-transaction consistency during a writer
commit, FTS5 availability, dirty reopen, WAL snapshots, restore, failed migrations,
abrupt process exit and lock release while a copy of the descriptor survives
([research 006](../../docs/research/006-owner-lock-inheritance.md)). Upgrade tests inject a test-only second migration.
The symlink/alias test is Unix-only.

The FTS5 probe is not an implemented library search API. Windows CI is configured
but has not run remotely. See [research 003](../../docs/research/003-reduced-foundation-validation.md)
for commands, the corrected test assertion and validation limits.
