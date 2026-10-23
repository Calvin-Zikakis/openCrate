# rekordbox database read, backup and restore: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** openCrate reads the owner's rekordbox library straight from `master.db` (playlists, tracks, ratings, My Tags, colours, comments, cues) behind the opt-in database setting, always from a backup copy taken while rekordbox is closed, and can restore any backup. Nothing is written to rekordbox's library in this slice except by an explicit restore.

**Architecture:** The owner's rbxport fork supplies `rbl-db` (with `rbl-core`, `rbl-anlz`), vendored and pinned. `oc-rekordbox::db` wraps it: take a snapshot (copy `master.db` with its `-wal` and `-shm` into a dated backup folder, check it), read the copy into the existing `xml::Collection` shape, and manage backups. The host feeds that collection into the existing mirror as a `master_db` source, so the Playlists, Library and player views work unchanged.

**Tech Stack:** Rust (rusqlite 0.40 built with SQLCipher through `rbl-db`), Tauri 2, React, Playwright.

**Spec:** [docs/design/rekordbox-sync.md](../../design/rekordbox-sync.md), sections "How openCrate talks to rekordbox", "The database setting", "Safety", "Left to the builder". This is the first of several plans; later ones cover the XML send to "Imported", the base snapshot and merge, database writes, folder cleanup and duplicates.

## Global Constraints

- Never touch the owner's Windows library or NAS. The Mac laptop's rekordbox 7 may be used, backed up first (owner, 2026-10-23).
- The database route is off by default, switched on in Settings behind a warning the owner must confirm; the setting records the rekordbox version it was confirmed against, and a different version turns it off until confirmed again.
- rekordbox must be closed for any database read or write: check the process before copying.
- Every read uses a copy, never the live file. Backups: keep the last 20, plus every pinned backup until the owner deletes it.
- Version gate: writes only for tested versions (none in this slice). Reads are allowed when the schema probe is `Full`.
- Tests use `rbl_db::fixture` databases and temporary directories only. No track names from any real library in the repository.
- Clippy `-D warnings`, no `unwrap`/`expect`/`panic` outside tests, no emojis in docs.

## Review Focus

1. rekordbox running when the owner presses Read: refuse with a plain message, take no copy.
2. A `-wal` holding committed rows not yet in `master.db`: the copy must include them (copy all three files; read the copy with WAL intact).
3. A copy that fails `quick_check` or the schema probe: no mirror change, the bad backup folder removed, the previous mirror kept.
4. Restore while rekordbox runs, or of a backup whose files are missing: refused before any file is touched; a restore first backs up the current state.
5. Deleted rows (`rb_local_deleted = 1`) and playlist entries pointing at deleted tracks: never shown as tracks; kept as unresolved references like the XML route.

---

### Task 1: Vendor rbxport's database crates

**Files:**
- Create: `vendor/rbxport/` (crates `rbl-core`, `rbl-anlz`, `rbl-db` copied from `~/repositories/rbxport-beat-analysis` at commit `1a0a8246`, plus a trimmed workspace `Cargo.toml`, `LICENSE`, `VENDORED.md`)
- Modify: `Cargo.toml` (`exclude = ["vendor"]`, `rbl-db` path dependency), `crates/oc-rekordbox/Cargo.toml`
- Create: `docs/decisions/0015-vendored-rbl-db.md`

- [ ] Copy the three crates' `Cargo.toml`, `src/` (and any `include_str!` assets) only; the vendored workspace manifest keeps rbxport's `[workspace.package]`, dependencies and lints so the code builds as written.
- [ ] `VENDORED.md`: source repository, commit, licence (GPL-2.0-or-later, used under GPL-3.0), and "local changes: none".
- [ ] Add `rbl-db` to `oc-rekordbox`; run `cargo build --workspace` and record the clean build time with SQLCipher in the decision record.
- [ ] Smoke test `rbl_db_opens_a_fixture` in `crates/oc-rekordbox/src/db/tests.rs`: `fixture::build(temp, Shape::default())` then `Library::open(location, OpenMode::ReadOnly)` gives `live_track_count() == 40`.
- [ ] Full gate: fmt, clippy, all Rust tests (the app database now runs on SQLCipher's SQLite without a key).

### Task 2: Snapshot

**Files:** Create `crates/oc-rekordbox/src/db/mod.rs`, `crates/oc-rekordbox/src/db/snapshot.rs`, tests in `crates/oc-rekordbox/src/db/tests.rs`

**Interfaces (produces):**
- `pub struct Snapshot { pub dir: PathBuf, pub master_db: PathBuf, pub taken_ms: i64, pub db_version: Option<i64>, pub app_version: Option<String>, pub tracks: u32, pub playlists: u32 }`
- `pub fn take_snapshot(location: &LibraryLocation, backups: &Path, reason: &str, running: bool, now_ms: i64) -> Result<Snapshot, DbError>`; the folder is `backups/<YYYYMMDD-HHMMSS>-<reason>/` holding `master.db`, `master.db-wal`, `master.db-shm` when present, and `snapshot.json`.
- `pub fn rekordbox_app_version() -> Option<String>` (macOS: `CFBundleShortVersionString` of `/Applications/rekordbox 7/rekordbox.app`; elsewhere `None` for now).

- [ ] Tests: `a_snapshot_copies_and_checks_the_database` (fixture; counts 40 tracks, 3 playlists; folder has `snapshot.json`); `a_running_rekordbox_refuses_the_snapshot` (`running = true` gives an error naming rekordbox and creates no folder); `a_corrupt_copy_is_refused_and_removed` (fixture whose `master.db` is truncated: error, no folder left); `wal_rows_reach_the_copy` (open the fixture read-write with WAL, insert a playlist without checkpointing, keep the connection open, snapshot; the copy shows 4 playlists).
- [ ] Implement: copy the three files, open the copy with the passphrase read-only, `PRAGMA quick_check` must return `ok`, `SchemaProbe::probe` must be `Full`; on any failure remove the folder.

### Task 3: Read a snapshot into a Collection

**Files:** Create `crates/oc-rekordbox/src/db/read.rs`

**Interfaces (produces):** `pub fn read_snapshot(snapshot: &Path, passphrase: &str) -> Result<xml::Collection, DbError>`

Mapping (so the existing mirror and views need no change): track `id` from `djmdContent.ID` parsed as `u64`; `location` and `location_bytes` from `FolderPath` (plain path, bytes as stored); attributes `Name`, `Artist`, `Album`, `Genre`, `Label`, `Year` (from `ReleaseYear`), `Comments`, `AverageBpm` (`BPM / 100` with two decimals), `Tonality` (`djmdKey.ScaleName`), `Rating` (stars times 51, the XML scale), `Colour` (`djmdColor` name when present), `MyTags` (names joined with `"; "`), `TotalTime` (`Length`). Cues from `djmdCue` (`InMsec / 1000`, `OutMsec` when positive, `Kind` hot cue number as XML's `Num`: hot cues `Kind` 1..8 become `Num` 0..7 and `Type` 0, memory cues `Kind` 0 become `Num` -1). Playlists from `djmdPlaylist` (`Attribute` 1 folder, 0 playlist, 4 smart playlist read as a playlist with no items) ordered by `Seq`, items from `djmdSongPlaylist` by `TrackNo`. Rows with `rb_local_deleted = 1` are skipped.

- [ ] Tests: `the_fixture_reads_as_a_collection` (40 tracks, 3 playlists of 5 items in order); `deleted_rows_are_left_out` (mark one track and one playlist deleted); `my_tags_and_rating_are_read` (insert a My Tag and set `Rating = 3`: attributes `MyTags = "Warm"`, `Rating = "153"`); `cues_are_read_in_seconds`.

### Task 4: Backups and restore

**Files:** Create `crates/oc-rekordbox/src/db/backups.rs`

**Interfaces (produces):**
- `pub struct BackupInfo { pub dir: PathBuf, pub taken_ms: i64, pub reason: String, pub pinned: bool, pub app_version: Option<String>, pub tracks: u32, pub playlists: u32 }`
- `pub fn list_backups(backups: &Path) -> Vec<BackupInfo>` (newest first, unreadable folders skipped)
- `pub fn prune(backups: &Path, keep: usize) -> std::io::Result<usize>` (removes the oldest unpinned beyond `keep`; returns how many)
- `pub fn set_pinned(dir: &Path, pinned: bool) -> std::io::Result<()>`
- `pub fn restore(backup: &Path, location: &LibraryLocation, backups: &Path, running: bool, now_ms: i64) -> Result<Snapshot, DbError>`: refuses while running or when the backup's `master.db` fails `quick_check`; first takes a pinned snapshot of the current library with reason `before-restore`; then copies the backup's `master.db` to a temporary name beside the live one, renames it over, and replaces or removes the live `-wal` and `-shm` to match the backup.

- [ ] Tests: `list_is_newest_first`, `prune_keeps_twenty_and_every_pinned_backup` (25 unpinned plus 2 old pinned: 20 newest plus 2 pinned remain), `restore_brings_back_the_old_library` (fixture, snapshot, delete a playlist live, restore: 3 playlists again, and a pinned `before-restore` backup exists), `restore_refuses_while_running` (live file unchanged), `restore_refuses_a_broken_backup`.

### Task 5: Host commands and setting

**Files:** Modify `crates/oc-core-db/src/mirror.rs` (`MirrorSource` gains `kind: SourceKind { Xml, MasterDb }`), `crates/oc-core-db/src/service.rs`, `src-tauri/src/main.rs`; create `src-tauri/src/rekordbox_db.rs`

**Interfaces (produces), Tauri commands:**
- `rekordbox_db_status() -> RekordboxDbDto { installed: bool, app_version: Option<String>, running: bool, enabled: bool, confirmed_version: Option<String>, needs_confirmation: bool }`
- `rekordbox_db_enable(enabled: bool) -> RekordboxDbDto` (stores `{"confirmed_version": app_version}` under the `rekordbox_database` setting, or clears it)
- `rekordbox_db_read() -> ImportResult` (refuses when disabled or the version differs from the confirmed one; snapshot with reason `read`, `read_snapshot`, `replace_mirror` with `SourceKind::MasterDb`, prune to 20)
- `rekordbox_backups() -> Vec<BackupDto>`, `rekordbox_backup_pin(dir, pinned)`, `rekordbox_restore(dir) -> RekordboxDbDto`

Backups live in the app data directory under `rekordbox-backups/`. `rbl_db::locate()` finds the library; `rbl_db::is_rekordbox_running()` gives `running`.

- [ ] Rust tests for the status logic: `a_new_version_needs_confirmation_again` (confirmed 7.2.19, installed 7.2.20: `enabled` true, `needs_confirmation` true, read refused).
- [ ] Mirror test: a `master_db` source and an `xml` source coexist (`UNIQUE (kind, path_raw)`).

### Task 6: Interface

**Files:** Modify `src/library/types.ts`, `src/library/tauri-api.ts`, `src/components/Sync.tsx`; create `src/components/RekordboxDatabase.tsx`; stub and tests in `tests/desktop-bridge.spec.ts`

- [ ] Settings, rekordbox: "Let openCrate read rekordbox's library directly", off. Turning it on opens a warning that names the risks (an unsupported rekordbox version, a format change) and the protections (a backup before every read or write, rekordbox must be closed, version check, restore), with "I understand, turn it on" and Cancel.
- [ ] Sync screen: when enabled, "Read from rekordbox" beside the XML route; the result line matches the XML read. A notice when rekordbox is running or the version needs confirmation.
- [ ] Backups list (date, reason, version, counts, pin, Restore with a confirmation naming the date).
- [ ] Browser tests: `the database setting warns before turning on`, `reading from rekordbox fills the playlists`, `a running rekordbox is reported`, `restore asks first`.

### Task 7: Spike on the laptop's rekordbox

- [ ] With rekordbox closed, copy the laptop's `~/Library/Pioneer/rekordbox` folder to the session scratchpad as an outside backup.
- [ ] An ignored test `live_laptop_read` takes a snapshot of the detected library into a temporary folder and reads it; compare track and playlist counts with `Library::live_track_count` and `live_playlist_count`. Aggregate output only.
- [ ] Research note `docs/research/010-rekordbox-database-read.md`: command, rekordbox version, DBVersion, counts matched, time taken; no names.
- [ ] LOG, ASSUMPTIONS (backup location and retention, attribute mapping), HANDOFF.
