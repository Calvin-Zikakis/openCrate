# Handoff for coding agents

## Latest change: rekordbox database read (branch `rekordbox-db-read`)

Built from the plan `docs/superpowers/plans/2026-10-23-rekordbox-database-read.md` on the branch `rekordbox-db-read`
(not merged or pushed; `main` holds everything before it). openCrate can read rekordbox's own library, opt-in from
Settings behind a warning, always through a backup copy taken while rekordbox and its agent are closed; backups can be
pinned and restored. Nothing writes to rekordbox except an explicit restore. Vendored `rbl-db` (decision 0015) brought
SQLCipher's SQLite into every build; read decision 0015 before adding a paged query with a correlated `EXISTS`.
Checked on the laptop's rekordbox 7.2.19 (research 010). Workspace: 404 Rust tests (7 ignored), 54 unit, 57 browser
runs. Next: a field-by-field comparison with an XML export, then the "Imported" playlist send (XML route).

## Earlier: overnight run, library core built (provisional)

The owner asked for code over documentation and for continuous work overnight. After the lock fix
(decision 0012, research 006) the following is built and tested locally on macOS, all provisional and
unreviewed by the owner except where noted (the R3/R7 contract in
[0013](decisions/0013-filesystem-identity-and-availability.md) was approved on 2026-10-23; assumptions A67-A84):

- `oc-paths` (pure): stored path form and WTF-8, name profiles and keys, Location parsing and alias
  resolution, read-only volume probes and root checks, state rules, read-only directory walker. 60 tests.
- `oc-tag` (read-only, `lofty`): audio properties, normalized tags, path-layout fallback, test fixtures. 11 tests.
- `oc-organize` (pure, slice 0f): sanitizer, path templates, Windows path fitting, title and version parsing,
  `destination`. 27 tests. Not used by anything yet; it moves no files.
- `oc-ingest` (slice 0f; `list` reads directory entries, the rest is pure): partial-download name filter (`.part`, `.crdownload`, `INCOMPLETE~`, Safari `.download`
  folders), a stability tracker driven by the caller's clock, and release grouping (`evaluate`: first folder below the
  download folder is the release; ready when nothing is partial and every audio, archive and cover file has been
  unchanged for the quiet period). 29 tests. The host polls it from the Import screen every 3 s (read only; no file-system events yet); `quiet_ms` (10 s) is
  a guess, see A92, and "already there" is decided by modified time against the day the folder was added, see A94.
- `oc-publish` (R1, file side; used by nothing yet, moves nothing in the real library): the journaled publish protocol
  (plan, record intent, hard-link no-clobber, commit record, remove staged name), `stage_copy` onto the destination volume
  with `.part` sweeping, an `Executor` whose run and recovery are one loop that decides from the disk, a `Store` trait
  (in-memory implementation only) and fault injection at every step. 13 tests, including a crash at each of 8 points, a
  second crash inside recovery for all 64 pairs, concurrent arrival, source changed, destination removed after recording;
  two deliberate mutations (clobbering publish, unguarded delete) were confirmed to fail them. See A95. Still open for R1:
  release-level behaviour (several tracks and remixer folders as one unit), the owner's choice for volumes without hard
  links, and a run on Windows NTFS.
- `oc-core-db` schema v6 and API (v6 adds `publish_operation` and `publish_store::PublishStore`, the SQLite `Store` for
  `oc-publish`: `commit_record` runs the catalog hook and the phase change in one transaction, `begin` fixes a staged
  file's identity, `unfinished` is fallible so a read failure is never taken for "nothing to recover"; 6 tests including
  real close-and-reopen restarts after a crash at each point; v5 adds `watch_folder`: download folders, added, switched on and off, removed and checked
  for reachability through `LibraryService`, refused when they overlap the library, each other or the data folder; the
  Settings screen has a Download folders section; nothing watches them yet): library tables and FTS5, `read_page` and the cached `LibraryReader`
  ([research 008](research/008-library-query-benchmark.md)), `Writer` and `ReaderPool`, roots, aliases,
  `apply_scan`, `Matcher`, the rekordbox mirror (`replace_mirror`, evidence-based alias suggestions), the tag
  catalog, and `LibraryService` (`add_root`, `scan_root`, `read_page`, `import_export`, `mirrors`). 88 tests plus an
  ignored benchmark.
- Tauri host: commands for reading, folders (native dialog), scans (background, polled status). The UI uses
  them only when started with `OPENCRATE_DEMO=<tracks>` (temporary synthetic database) or
  `OPENCRATE_DATA=<absolute folder>` (persistent library); otherwise the in-memory demo as before. Settings has
  a Folders panel (add, scan, progress, summary) and a real rekordbox Sync screen (import XML, read again,
  accept a suggested prefix). 25 host tests (most through Tauri's command dispatch), 52 unit, 53 browser runs (drag tests also in WebKit, the engine the macOS app uses).
- Workspace: 374 Rust tests, strict clippy and fmt clean. Nothing is committed or pushed.

Native check (debug macOS bundle with `OPENCRATE_DEMO=5000`): launched, alive 9 s, no stderr. What rendered is
NOT verified (no screen capture here); it is liveness only. Windows: `oc-paths`, `oc-tag` and `oc-rekordbox` type-check and pass strict clippy for the Windows target (nothing run); `oc-core-db` and the host were not checked there.

Cues and beat grids from the export are shown read only ("Cues and grid" on a selected track; `read_track_analysis`).
Not built yet: the default app-data location (owner chose the normal app-data folder, set up by a future installer; A86),
file-system watching, cue and beat-grid editing (read only by design). Decision 0013 was approved by the
owner on 2026-10-23. The agreed rekordbox sync design is docs/design/rekordbox-sync.md (not built).

State as of 2026-10-22: the reduced Rust foundation and synthetic React/Tauri
visual sub-slice are built and validated locally. A macOS development bundle
exists. Full milestone 0, native UI readiness, Windows installation and hosted
CI are not validated. (Older text: since then the UI gained database IPC and a windowed
SQLite API, opt-in; see the block above.)

The owner requested ongoing work with durable Claude Code checkpoints.
[Decision 0009](decisions/0009-reduced-foundation-and-recovery.md) bounds the
lifecycle foundation; [0010](decisions/0010-synthetic-interface-shell.md) allows
the visual shell before filesystem indexing. Neither adopts a review's full
sequence or closes R3/R7. Standalone read-only rekordbox XML parsing is now
implemented under [0011](decisions/0011-read-only-xml-parser.md), using synthetic
fixtures only. Work paused there for credit, resumed to fix the lock defect and
then continued through the overnight run above. Current validation is the
"Workspace" line above. Background: [research 005](research/005-read-only-xml-validation.md)
(parser) and [research 006](research/006-owner-lock-inheritance.md) (lock fix).

## 1. Start here in Claude Code

The root [CLAUDE.md](../CLAUDE.md) imports [AGENTS.md](../AGENTS.md). Preserve that
entry point. Read the newest [LOG](LOG.md) entry, [ASSUMPTIONS](ASSUMPTIONS.md),
decisions 0009/0010/0011 and the crate READMEs before changing code, including the
[Tauri host README](../src-tauri/README.md).

After each substantive step, record actual results in LOG, replace the current
handoff state rather than stacking contradictory scratch notes, and reconcile
the docs. Record commands, fixtures, failures, remaining work and whether an
app/server remains running. Preserve all earlier uncommitted code, manifests,
lockfiles, planning edits, original decisions and both review reports. Do not
edit reviews or discard unrelated changes. Nothing in this continuation has
been committed or pushed; historical permission is not fresh authorization.

Use synthetic fixtures and temporary directories only. Do not access private
music, download folders, exports, NAS or live rekordbox data. Keep names, audio
and credentials out of the repository. The repository remains private.

## 2. Built code and limits

| Item | Current state |
| --- | --- |
| [oc-core-db](../crates/oc-core-db/README.md) | Lifecycle owner (schema v2: lifecycle plus library tables), read-only connections, snapshots and restore, owner lock released on drop; library queries, cached reader, writer thread, reader pool, roots, scan reconciliation, matching; 67 tests |
| `oc-paths` | Provisional path identity, Location mapping, probes, walker and state rules; 60 tests |
| [oc-rekordbox](../crates/oc-rekordbox/README.md) | Location decoder and standalone typed XML reader, 23 passing tests; no file matching |
| [React frontend](design/interface.md) | React 19, Vite 7, TypeScript 5.9, TanStack Virtual 3, plain CSS; synthetic tracks/releases/artists, search/sort/filter and selection |
| [Tauri host](../src-tauri/README.md) | Minimal Tauri 2 binary; no custom commands, filesystem/network/shell plugins or command permissions |
| Root Cargo workspace | Members `crates/*` and `src-tauri`; two library crates plus Tauri. Default members are `crates/*`. Rust minimum 1.99, pinned toolchain 1.99.0 |
| SQLite | `rusqlite` 0.40 with `bundled` and `backup`; no SQLCipher or rbxport dependency |
| [.github/workflows/rust.yml](../.github/workflows/rust.yml) | Builds frontend first, then Windows/macOS fmt, strict clippy, workspace tests and build; no hosted run |
| [.github/workflows/frontend.yml](../.github/workflows/frontend.yml) | Linux web/unit/browser checks; manual-dispatch unsigned Windows NSIS build; neither run remotely |
| Library walk, tables, FTS5 search, writer worker, reader pool, matching, tag reading (`oc-tag`), rekordbox mirror and playlists | Built in Rust and connected to the UI through Tauri commands, opt-in (`OPENCRATE_DEMO` or `OPENCRATE_DATA`); the default preview is still the synthetic in-memory catalog |
| File watching, import/conversion, audio playback, cues and beat grids on screen, master.db read | Not implemented |

### XML parser checkpoint

Decision 0011 and A58-A64 define the `BufRead`/`quick-xml` contract: complete
typed output only after valid EOF; metadata attributes, Locations/raw bytes,
fractional-second cues/grids and ordered playlists retained. Duplicate playlist
references are preserved; missing collection references are unresolved IDs,
not missing files. Invalid Locations, duplicate collection IDs, count
mismatches, malformed input and unsupported key/entity mechanisms fail.
Initial limits are 128 MiB input, 1,000,000 collection tracks, 1,000,000 playlist
references and XML depth 128, including skipped subtrees.

No file I/O, matching, database transaction, UI connection or private export
access belongs to this slice. `xml::read` and `xml::read_with_limits` are built.
Limits may only lower the defaults. Unknown subtree roots are reported in
`ignored_elements`; unresolved source IDs are sorted and deduplicated.
The parser retains the complete typed result in memory, not constant memory.
Research 005 records 16 parser tests plus seven codec tests; 20 database tests
(18 plus two lock-release regressions) give 43 workspace tests. There is no XML performance measurement
or real-export compatibility result.

### Synthetic shell boundary

- A dedicated Web Worker owns the deterministic catalog and query/sort work.
  `LibraryApi.read(query, start, count, signal)` returns a total and at most
  200 rows. Request IDs, rejected cancelled promises and UI guards prevent
  outdated results from taking over. Abort does not interrupt synchronous
  computation already running inside the worker.
- Tracks and release/artist cards are virtualized. Cards open filtered tracks.
  `/` focuses search; arrows and Home/End move focus; Enter selects a track or
  opens a card. Ctrl/Cmd+K and actual playback shortcuts are not implemented.
  The collapsed sidebar retains accessible names; settings use a modal dialog
  with focus return.
- Only compact rows and reduced motion persist in `localStorage`. Storage
  errors are visible. System reduced motion is also respected. The faint text
  token is `#9494a1`, not the old design value.
- Import/sync/settings explicitly say that pickers, XML and importing are not
  connected. The player is a disabled visual preview. No real data or audio.
  The UI does not access `oc-core-db`.
- The configured CSP is local-only, including Tauri's local IPC transport and
  data images; inline styles support virtualization/artwork. Empty capability
  permissions and no plugins are not a general OS sandbox. Only generated
  desktop icons are retained.

### Lifecycle contract

- A canonicalized app-data directory has one cooperative owner lock. This is not
  protection against non-cooperating SQLite writers. The lock is released by an
  explicit unlock when the owning value drops, not by the descriptor closing,
  because a concurrently spawned child can hold a copy of it (decision 0012).
- The caller must choose a private local directory outside synced folders.
  Network-filesystem detection is not implemented.
- Existing database application ID, schema version and lifecycle state are
  checked before opening for writes. The owner uses WAL and `synchronous = FULL`.
  Explicit close marks clean; drop or crash leaves dirty. Dirty reopen checks
  database and foreign-key integrity.
- Online backups include committed WAL data. Checked, flushed snapshots precede
  upgrades of existing schemas, not initial version-0 creation. Only schema v1
  ships; tests inject later migrations.
- Restore takes a standalone snapshot and writes a new database in an unused
  local app-data directory. It never overwrites or switches directories for the
  caller. Creation and restore refuse orphan SQLite journals. Retention and
  scheduled backups remain later work.
- Location decoding preserves bytes, punctuation, case and Unicode form. It does
  not resolve a path, remap roots or establish filesystem identity.

## 3. Validation checkpoint

Evidence supplied by the parent on arm64 macOS, Node 24.22.1/npm 11.21.1 and
Rust 1.99.0. This documentation pass inspected code but did not rerun commands.
See [research 003](research/003-reduced-foundation-validation.md) for the
foundation and [research 004](research/004-synthetic-interface-validation.md)
for current commands, browser measurements and packaging limits.

All npm commands below used `npx -y -p node@24 -p npm@11 npm <...>`.
`.nvmrc` selects major 24 and `package.json` requires `>=24 <25`; do not use the
machine's global Node 26. The listed patch versions are the observed environment,
not an exact Node/npm pin in the repository.

| Command/check | Result |
| --- | --- |
| `npm ci` | Passed; lockfile reproducibility checked by parent; zero audit findings; unapproved esbuild/fsevents install-script warnings |
| `npm run build` | TypeScript and production Vite build passed |
| `npm test` | 12 unit tests passed in three files, Vitest 5 in the Node environment |
| `npm run test:browser` | Nine Playwright Chromium tests passed against the production preview |
| `cargo fmt --all --check` | Passed |
| `cargo test --workspace --locked` | 25 tests passed: 18 database, seven codec; zero Tauri tests and zero doctests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo check --workspace --locked` | Passed with Tauri included; Cargo metadata also confirmed membership |
| `npm run tauri -- build --debug --bundles app` | Produced `target/debug/bundle/macos/openCrate.app`, 14.78 MiB, arm64 only |
| Exact bundled executable launched for three seconds, then stopped | Process liveness only; no native UI readiness evidence |

The macOS bundle has a linker ad-hoc signature and no Developer Team. It is not
a signed release or a Windows installer. No app/server remains running. The
parent confirmed that native `/tmp` stdout/stderr logs were empty and cleaned
up, and reported a clean final shell diff check. Its Python local-link checker
found zero missing targets among 280 targets in 32 Markdown files. Exact
checker invocations were not supplied; those checks predate the parser docs.
The initially added Vitest 3 dependency graph had audit warnings; it was
deliberately replaced with Vitest 5, not repaired by automatic audit fixes.

The browser command used 100,000 deterministic tracks, 10,000 releases and
240 artists at 1440x960 in headless Chromium 156.0.8078.4. Latest results:
30 warm searches at 34.1 ms p95 (input event through current result plus two
animation frames); 120 scroll frames at 17.5 ms p95, 17.8 ms maximum, none over
33 ms and at most 21 track DOM rows. Cold readiness was observed at 83.2 ms,
including polling, not precise startup-to-paint. The parent viewed track and
release screenshots in ignored `test-results/`. These are browser-demo numbers,
not general speed guarantees or evidence for Windows, DB/IPC, import load or
audio. Earlier timings are superseded by research 004.

Browser coverage includes deep rows, virtualized cards, search races, filters,
drill-down, keyboard/modal focus, storage write failure, worker-construction
failure, 720 px minimum width, all twelve main/secondary/faint text versus
four-surface contrast combinations at least 4.5:1, and system reduced motion.
This is not a full accessibility audit. One Rust test is a child-process helper;
the symlink/alias test is Unix-only. Earlier documentation-check counts in
research 003 are historical, not fresh checks of this expanded worktree.

## 4. Product decisions unchanged

The [Game Plan](<../DJ Library Manager — Game Plan.md>) remains the product plan:
free GPL-3.0 desktop software in Rust, Tauri 2 and React, Windows first with Mac
development. Existing libraries are indexed in place, not renamed.

- rekordbox owns playlists, cues and grids. Read-back is XML-first; playlists
  are read-only. The experimental `master.db` reader remains off by default and
  copy-only ([0008](decisions/0008-rekordbox-readback-xml-first.md)).
- Planned download folders are configurable, validated and switchable without
  restart. Adding one stages nothing; originals stay unchanged (A48-A50).
  These settings are not connected in the shell.
- The desktop library is authoritative; QSync handles the NAS copy. No direct
  NAS, Navidrome or AudioMuse-AI integration, and no playlist files.
- Nine pipeline crate boundaries plus Tauri remain planned. Two library crates
  and the Tauri host exist; create the others when needed, not as placeholders.

Milestones 1-9 keep their existing scope and order in [BUILD-PLAN](BUILD-PLAN.md).
Milestone 0 is split into bounded slices there. Review recommendations are not
blanket approval to change product policy.

## 5. Unresolved review gates

Read the [open review findings](review-findings.md). This list is a restart aid, not a claim
that other findings are closed:

| Before | Gate |
| --- | --- |
| Library schema, scanner or matching | R3: volume-aware path identity, raw paths, Unicode/case, root mapping and containment; R7: lifecycle versus offline/unmatched/missing states. The codec does not close R3 |
| Filesystem mutation helpers or filing | R1: durable operation/recovery protocol, atomic no-clobber publication and staging outside synced folders; fault injection around file and database steps |
| Import automation | R5-R6: unknown versions, signal provenance, conflicts, held-out labels, persistent download baselines and completeness; Opus 8-9: label ownership and archive/loose-file input |
| XML export or experimental database read | R4: exported versus confirmed presence, freeze and source-scoped identity; R8: consistent copy acquisition for `master.db`. App-database backups do not settle that external-copy contract |
| Conversion, playback or upgrades | R9: per-volume space and preservation/passthrough policy; Opus 12: cue alignment; R2: synced upgrades versus copy-only access and existing-library protection |
| UI/performance/distribution | R10-R11: supported toolchains/platforms, dependency spikes, installer/signing/licensing and end-to-end measurement; R13: estimates and cut lines remain open |
| Online providers | R12: cache expiry, retries, credentials, consent and safe fetching |

Do not choose live rekordbox write mode, a manual upgrade workflow or a new
conversion policy without an owner decision. The existing owner questions
remain in [ASSUMPTIONS](ASSUMPTIONS.md): export trigger, ID3 version, name/bundle
ID, rbxport reuse, theme and Windows path limits. npm is selected under 0010,
not a blocker awaiting a package-manager choice. Review-driven
questions also include upgrade permission, input archive handling, conversion
compatibility and responsibility for private labels. CI minutes are unmeasured.

## 6. Exact next steps

1. Read the newest LOG and inspect the current worktree without discarding any
   changes. The visual shell, standalone XML parser and lock fix are locally
   built and tested. Keep the original decisions and dirty planning edits. No
   commit/push permission exists.
2. Reproduce the local baseline from the repository root under Node 24:

   ```sh
   npx -y -p node@24 -p npm@11 npm ci
   npx -y -p node@24 -p npm@11 npm run build
   npx -y -p node@24 -p npm@11 npm test
   npx -y -p node@24 -p npm@11 npm run test:browser
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo test --workspace --locked
   cargo check --workspace --locked
   cargo build --workspace --locked
   ```

   If Chromium is missing, first run
   `npx -y -p node@24 -p npm@11 npm exec -- playwright install chromium`.
   Use `~/.cargo/bin/cargo` if needed. Build the frontend before workspace
   Cargo checks; Tauri embeds `dist/`. Use `-p oc-core-db -p oc-rekordbox` for
   library-only validation, not as a substitute for a workspace check.
3. For an owner preview, run
   `npx -y -p node@24 -p npm@11 npm run tauri -- dev`.
   Warn that it is demo-only: no files, XML, imports or playback are connected.
   Stop the app/server afterwards. A native visual smoke check is still needed.
   To reproduce the local bundle, use
   `npx -y -p node@24 -p npm@11 npm run tauri -- build --debug --bundles app`.
4. To try it: `OPENCRATE_DATA=/absolute/path/to/an/empty/dev/folder npm run tauri -- dev`, then Settings, Add
   folder, Scan; then the rekordbox screen, Choose export. Use a COPY of a small music folder and a COPY of an
   export; everything only reads except Import, which copies files into the chosen music folder (never replacing one) and writes `.opencrate-staging` beside it. This build is unreviewed. Next, in order: import, Import all, converting (ffmpeg), tag writing (ID3v2.3), tidy names and the folder layout setting are done (A96-A100); MusicBrainz lookups from the Import all review are done (decision 0014, on request only); the read-only Library check and "03 Title" track numbers are done; fixing what the check finds waits for the owner (moving files breaks rekordbox and NAS paths); the fake-lossless check is done (research 009, A102; flags only); playback with a stored waveform and rekordbox cue markers is done (`oc-player`, A103, schema v13); next: Discogs and AcoustID lookups, file watching instead of polling, release-level publishing; release-level publishing for R1 (a `group` on each operation so a rejection holds the release's unstarted files, and creating destination folders before the link; not started, nothing half-edited); file watching (the Import screen polls today); then the Windows NTFS run of `oc-publish`
   before any filing or mutation. Use synthetic data only for tests; never the owner's library or export.
5. The R3/R7 contract (0013) is approved by the owner (2026-10-23). Do not reuse metadata normalization as filesystem identity. Scheduling and
   all mutation/recovery gates above remain unchanged. Remote workflows,
   Windows NSIS installation and owner checks require separate authorization
   and evidence; do not push to obtain them without permission.

Later agents must leave a durable checkpoint after each substantive step,
including failures and remaining gates. If a test is flaky, reproduce it,
find the mechanism and fix the cause; do not serialize or retry around it.
