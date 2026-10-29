# oc-core-db

The library database: SQLite in the app data folder, its schema (v28, one migration per file in `src/schema`),
recovery, and the library service the desktop app talks to. It holds metadata only; the audio stays in the files.
The workspace uses bundled SQLite with the online backup feature. rekordbox's encrypted database belongs to
`oc-rekordbox`.

## Public API

`service::LibraryService::open(directory, readers)` opens a library for an application: one writer thread takes
every change in order, and a pool of cached readers answers queries without waiting for it (WAL). The UI layer
talks to the service, never to SQLite. Its methods follow the modules:

| Module | What it holds |
| --- | --- |
| `library` | Windowed queries for the track, release and artist lists: search, filters, sorts and pages of rows |
| `roots`, `scan`, `catalog` | Music folders and whether each is online; applying a scan; reading new or changed files' tags into artists, releases and tracks |
| `downloads`, `imports`, `publish_store` | Watch folders; what was imported, grouped into batches; the `oc-publish` journal on SQLite |
| `crates`, `fields`, `edits` | The owner's crates, custom fields, and the history of tag edits for undo |
| `mirror`, `matching`, `merge`, `offers` | The copy of rekordbox's library (from an export or its database), matching its paths to files, the three-way merge for Sync, and tracks offered to rekordbox |
| `health`, `moved`, `reorganize` | The library check, files moved outside openCrate, and the Reorganize tool's sittings and the moves put back |
| `synthetic` | A deterministic synthetic library (up to 100,000 tracks) for the demo, tests and benchmarks |

Underneath, `Database` owns the file:

| Item | Purpose |
| --- | --- |
| `Database::open(&Path) -> Result<Database>` | Own an app data folder and open or create `library.sqlite3` |
| `Database::status() -> OpenStatus` | Report `Created`, `Clean` or `CheckedAfterUncleanShutdown` for this open |
| `Database::reader() -> Result<rusqlite::Connection>` | Open an independent read-only, query-only connection |
| `Database::snapshot() -> Result<PathBuf>` | Write a checked standalone snapshot under `snapshots/` |
| `Database::close(self) -> Result<()>` | Mark clean and close the owner connection |
| `restore(&Path, &Path) -> Result<PathBuf>` | Restore a snapshot into an unused folder and return the new database's path |

These calls block and must not run on the UI thread.

## Safety and limits

- The caller supplies a private, local app data folder outside any synced library. The crate does not detect
  network file systems or sync software.
- An exclusive lock on `owner.lock` keeps a second openCrate from opening the same library. It does not stop other
  software from writing SQLite files. The database and lock files must be regular files, not links. The lock is
  released explicitly when the owner is dropped, so a child process still holding a copy of the descriptor cannot
  keep the folder locked.
- An existing database is checked read-only for its application ID, schema version and state before it is opened
  for writing. A foreign database, one from a newer version, or an invalid one is refused.
- The owner connection uses WAL, `synchronous = FULL`, foreign keys and a two-second busy timeout. An open marks the
  database dirty and a clean close marks it clean; after an unclean shutdown the next open runs integrity and
  foreign-key checks.
- A snapshot is taken before an existing schema is upgraded, using SQLite's online backup, and checked before it is
  kept. Each migration commits in one transaction with its version change, and migrations are safe to run again.
- Restore needs a standalone snapshot, never replaces an existing database, and does not switch the app to it.
  Leftover `-wal`, `-shm` or `-journal` files make creation and restore refuse; they are kept, not deleted.
- Search matches the start of each word through FTS5, ignoring case and accents, and every word must match.
  Queries are built from fixed fragments with every value bound.
- Snapshots are not pruned, and nothing restores automatically.

## Validation

From the workspace root:

```sh
cargo test -p oc-core-db --locked
cargo clippy -p oc-core-db --all-targets --locked -- -D warnings
```

The tests use temporary databases and synthetic libraries only. They cover ownership and refusals, readers during
a write, dirty reopen, snapshots and restore, failed migrations, an abrupt exit, scans, imports, crates, the
rekordbox mirror and merge, moved files and Reorganize sittings. The link test runs on Unix only.
