# Build plan

What will be built, in what order, and how each step is judged. The decisions are in the [Game Plan](<../DJ Library Manager — Game Plan.md>). This file turns them into milestones. It is a proposal for the owner and a reviewer to challenge; the order and the cut lines are open (see "Questions for the review").

[Decision 0009](decisions/0009-reduced-foundation-and-recovery.md) accepts a
reduced first slice of milestone 0. Its lifecycle and codec code pass locally.
[Decision 0010](decisions/0010-synthetic-interface-shell.md) brings the synthetic
visual part of 0e ahead of 0d; that sub-slice is built locally, not full milestone
0. Rust/frontend CI are configured, not remotely run. The remaining boundaries
below still apply.
Milestones 1-9 retain their scope and order. Neither review's full sequence has
been adopted, and the review gates below still apply.

## Principles

1. **Every milestone ends with something the owner can try on the Windows desktop.** Nothing is "done" because it compiles on the Mac.
2. **Risk and value come first.** Windows builds, the rekordbox file formats and the rekordbox read-back come before network lookups.
3. **Never touch the real library from code under test.** Tests use synthetic data and temp folders. Checks against the owner's library are run by the owner, print counts and timings only, and are written up in a research note.
4. **Measure before claiming.** Speed claims name the command and the dataset ([decision 0004](decisions/0004-interface-and-speed-budgets.md)).
5. **Document as you go.** Each milestone adds a LOG entry, updates ASSUMPTIONS, and records decisions and measurements.

## Milestones

| # | Name | What the owner can do afterwards | Needs |
| --- | --- | --- | --- |
| 0 | Foundation and Windows CI | Install a Windows build and see the interface shell, scrolling a large demo library smoothly | none |
| 1 | Index in place and library views | See the real library, searchable, with covers, without any file being changed | 0 |
| 2 | rekordbox read-back (XML) | See rekordbox's playlists, BPM, key and cues next to the library, and the entries whose files are gone | 1 |
| 3 | Import pipeline, dry run | Point it at a downloads folder and see exactly what it would do, with a review queue | 1 |
| 4 | Convert, tag and file | Import a real release end to end into the library | 3 |
| 5 | rekordbox export and the experimental `master.db` option | Send new tracks to rekordbox by XML, and optionally read `master.db` | 1, 2, 4 |
| 6 | Player | Play a track with the waveform and rekordbox's cues | 2 |
| 7 | Online lookups | Fewer releases go to review, because MusicBrainz, AcoustID, Discogs and Bandcamp are asked | 3 |
| 8 | Dedupe and upgrades | Duplicates found, better copies replace old ones, old ones are archived | 4, 5 |
| 9 | Installers, onboarding and user docs | Another DJ can install it, set it up and use it | all |

The two-to-five-week window is a planning target, not a measured estimate.
Estimate each remaining slice and give it a defer/split condition before
scheduling. Split work that cannot fit.

### 0. Foundation and Windows CI

Build only crates that a slice needs. The workspace currently has `oc-core-db`,
`oc-rekordbox` and the Tauri host `opencrate-desktop`. Default members are the
two library crates; `--workspace` includes the host. The planned boundaries
remain `oc-core-db`, `oc-ingest`, `oc-identify`, `oc-dedupe`, `oc-convert`,
`oc-tag`, `oc-organize`, `oc-rekordbox` and `oc-player`, plus Tauri.

| Slice | Status and boundary | Acceptance still needed |
| --- | --- | --- |
| 0a. Database lifecycle | Done locally: v1 `app_state` only, cooperative directory ownership, app-ID/version checks, WAL FULL, read-only connections, clean/dirty marker, online backups and no-clobber restore | Hosted Windows/macOS results; application shutdown/worker integration belongs to later slices |
| 0b. Location codec | Done locally: one-pass percent decoding to lossless bytes, with malformed escapes/NUL/prefix refusal | Hosted Windows/macOS results; matching/root mapping is not this slice |
| 0c. Rust validation | Configured Windows/macOS workflow builds the frontend first, then fmt, strict clippy, workspace tests and build; local checks include Tauri | Actual hosted runs, platform failures resolved and results recorded |
| 0d. Read-only library/query workload | Built locally, provisional (overnight run): library, mirror and catalog migrations (schema v4), FTS5 search, windowed API with a cached reader, writer thread, reader pool, synthetic generator and benchmark ([research 008](research/008-library-query-benchmark.md)); also the scanner walk, tag reading and rekordbox mirror | Defined identity/availability fixtures, versioned migration/restore tests, deterministic windows/totals, first/deep/filtered queries on 100,000 synthetic tracks; command, hardware and dataset recorded |
| 0e. Shell and Windows artifact | Visual sub-slice built locally under 0010, and since then connected to the Rust library through Tauri commands, opt-in (`OPENCRATE_DATA` / `OPENCRATE_DEMO`): folders, scans, rekordbox sync, playlists, offline banner; macOS development bundle built and launched (liveness only) | Native UI readiness, hosted CI, Windows NSIS build/install and owner smoke check; later database/IPC/render instrumentation. The shell does not use core-db or implement 0d |
| 0f. Pure pipeline helpers | Partly built: `oc-organize` (sanitizer, templates, Windows path fitting, title and version parsing, 27 tests). `oc-ingest` (partial-name filter, stability tracker, release grouping, read-only lister, 29 tests). Not built: name scorer (needs the identity/scoring contracts), the watcher; no production library mutation | Synthetic cross-platform fixtures and resolved identity/scoring contracts. Mutation helpers cannot be accepted before the R1 recovery/publication gate |

Local evidence is in [research 003](research/003-reduced-foundation-validation.md),
[004](research/004-synthetic-interface-validation.md) and later notes; current
counts and what each check could not observe are in [HANDOFF](HANDOFF.md) and
[LOG](LOG.md). Format, strict clippy and workspace check pass; the Tauri
development bundle compiles and launches.

Using `npx -y -p node@24 -p npm@11 npm <...>`, `npm ci`, `npm run build`,
`npm test` (12 tests in three files) and `npm run test:browser` (nine Chromium
tests) passed. Node 24/npm, React 19, Vite 7, TypeScript 5.9, TanStack Virtual 3
and plain CSS are implemented. Vitest 5 replaced the newly added v3 graph after
audit warnings; the current install reports zero findings.

Research 004 records the latest browser-demo workload and measurement limits.
It is not a SQLite/windowed-IPC benchmark or Windows evidence. Only appearance
persists in localStorage; filesystem pickers, importing, XML and audio are not
connected. The macOS app's three-second executable check proves process
liveness, not native rendering or responsiveness.

Lifecycle limits: local storage is a caller contract, not network detection.
Snapshots use SQLite's online backup API before upgrades of existing schemas;
initial creation from version 0 has no snapshot. Restore writes only into an
unused local app directory and refuses existing databases and orphan journals.
There is no retention or scheduling. The current synchronous owner is not a
background writer, and independent read-only connections are not a pool.

Remaining milestone-0 acceptance:

- Tests and strict clippy pass in hosted Windows/macOS CI, including portable
  Windows-style fixtures. Local checks alone do not finish the milestone.
- Before library migrations or matching, settle the path and state contracts.
  [Data-model](design/data-model.md) is a future sketch, not the shipped v1.
- Define backend, IPC and render allocations within decision 0004's
  user-visible budgets; it does not yet define separate database budgets.
  Record the synthetic 100,000-track benchmark and a real IPC list/search
  baseline. No unmeasured performance claim is acceptance.
- Browser synthetic scrolling/search and stale-result checks now have named
  evidence in research 004. Keep the 60 fps target and end-to-end application
  measurement open; this browser workload alone does not prove the budget.
- The owner installs the Windows shell and it opens. Linux frontend checks and
  an on-demand unsigned Windows NSIS build are configured in `frontend.yml`,
  not run. Both hosted workflows and owner installation still need evidence.
- Pure helpers pass synthetic fixtures. Atomic move, copy-verify and archive
  helpers remain gated on a durable operation/recovery contract and failure
  injection before they can be called complete.

Not in this milestone: scanning real files, provider/network features or audio.
The player bar is a shell. No real library or export is needed for this work.

#### Next safe checkpoint: standalone XML parser

Not started at the reconciled shell endpoint. Stage a read-only parser in
`oc-rekordbox` using synthetic XML fixtures only. Define its bounded API,
malformed-input checks, estimate and defer/split condition before coding.
This is parser groundwork for milestone 2, not a change to milestones 1-9 or
acceptance of that milestone. No path matching, root mapping, mirror database,
UI/native picker, live rekordbox access or private export reading belongs here.
Keep R3/R7 ahead of indexing/matching and all mutation gates unchanged.

#### Review gates before later work

These gates qualify the later scopes; they do not silently change product policy.
See the [open review findings](review-findings.md).

- **R3/R7, before schema, scanner or matching acceptance:** filesystem identity,
  raw paths, volume case/Unicode semantics, explicit root mapping, containment,
  and distinct lifecycle/availability/match states. The byte codec does not
  resolve R3.
- **R1, before any mutation helper or milestone 4 filing** (file side built in `oc-publish` with fault injection and a SQLite
  store in `oc-core-db`; release-level grouping and the no-hard-link fallback are not): a durable operation
  record and restart protocol spanning file and database changes; no-clobber
  publication; destination-volume staging outside the synced subtree; crash
  tests before and after each side effect.
- **R5/R6 and Opus 7-9, before automation:** signal provenance, unknown-version
  policy, conflict handling, held-out labels, durable download baselines and
  archive/loose-file completeness rules.
- **R4/R8, before milestone 5:** exported is not confirmed imported. Define
  freeze/confirmation/source identity and a consistent external database-copy
  contract. The app's own online backups do not implement that contract.
- **R2/R9 and Opus 11-12, before conversion/upgrades:** owner decisions on
  conversion compatibility and existing-library/synced upgrades; per-volume
  space/recovery tests and cue alignment. Copy-only read-back does not authorize
  live rekordbox writes. No write mode or replacement policy is selected here.
- **R10-R13:** dependency and packaging validation, supported platforms,
  end-to-end performance, provider privacy/retry contracts and realistic
  estimates remain open at their relevant milestones.

### 1. Index in place and library views

Scope
- Library roots (folder picker, several allowed, drive letters and removable drives).
- Scanner: reads headers only, bounded parallelism, incremental by path, size and modified time, resumable, cancellable. Handles names that are not valid UTF-8.
- Offline roots: never marked missing.
- Artwork: embedded art cut into two sizes once, cached locally.
- Tracks, Releases and Artists views on real data, search, density setting, Settings screen.

Not in this milestone: moving, renaming or tagging any file. The scanner opens files read-only.

Acceptance
- The owner indexes the real library (about 9,000 tracks in rekordbox): time, peak memory and row counts recorded.
- A rescan of an unchanged library is fast (target to set after the first run).
- Unplugging the library drive shows the offline banner, keeps the library browsable, and marks nothing missing.
- The interface budgets are met on the real index, with numbers recorded.
- A test proves the scanner never opens a file for writing.

### 2. rekordbox read-back (XML)

Scope
- Settings: the rekordbox XML export file, chosen with a file picker and remembered. A "Re-read" action, and a notice when the file's modified time changes.
- An XML reader: collection entries (location, tempo, key, rating), beat grids, cues and loops, and the tree of folders and playlists. `Location` is decoded exactly as rekordbox encodes it ([research 002](research/002-owner-data-shapes.md)). Element and attribute names are confirmed against the owner's export when the reader is written.
- Matching: each entry is matched to an indexed file by its decoded path under a known library root, so drive letters and remapped roots work. Entries that do not match are listed.
- The mirror tables are filled ([data-model](design/data-model.md)). The library shows BPM and key. Playlists appear read-only in the sidebar and as a library filter. Entries whose files are gone are listed on the Sync screen.

Not in this milestone: writing XML, reading `master.db`, any playlist editing.

Acceptance
- Synthetic XML fixtures cover every character in the encoding table, entries without cues, empty playlists, nested folders, and entries that point at missing files.
- On the owner's real export, run in place by the owner, the reader's counts match the file's own element counts (9,008 entries, 135 playlists, and the cue and tempo counts), and the share of entries matched to indexed files is reported. Aggregate numbers only.
- Re-reading an unchanged file changes nothing. A failed read leaves the previous mirror in place.
- The library list shows BPM and key at the same frame rate as before.

### 3. Import pipeline, dry run

Scope
- Download folders as a setting: one or more, chosen with the folder picker, validated, switched on and off in Settings with no restart ([import-pipeline](design/import-pipeline.md)). Then watching, stability tracking, staging, identification from local signals only (tags, folder and file names, MusicBrainz IDs already in tags), scoring, and the review queue UI grouped by release.
- A plan view: for every file, the proposed destination and tag changes.
- Nothing is moved into the library.

Acceptance
- A download folder added in Settings is watched at once. One that is the library root, staging or archive, or nested with another, is refused. The originals in a download folder are never changed (checked by hash).
- On a hand-checked set of about 200 downloads, no wrong auto-file, with coverage reported.
- Re-running is idempotent. Killing the app mid-run and restarting resumes.
- The proposed paths follow the template and the 260-character rule.

### 4. Convert, tag and file

Scope
- ffmpeg as a sidecar (a build with LAME whose license stays GPL-3.0 compatible). Lossless to AIFF (`.aif`), other lossy to MP3. The fake-lossless check. Tag and artwork writing. The atomic move into the library. The archive. `source_quality` recorded.
- Upgrades are not applied yet. They wait in review.

Acceptance
- Converted files have the right duration and tags, and play in rekordbox (owner check).
- Failure injection (a crash between any two steps) leaves no partial file in the library and no lost staging copy.
- Filed paths stay under 260 characters.

### 5. rekordbox export and the experimental `master.db` option

Scope
- A decision on reusing rbxport's `rbl-*` crates (vendor, submodule or git dependency), and then on the SQLite build ([decision 0001](decisions/0001-sqlite-local-database.md)).
- An XML writer for newly filed tracks, with `Location` written exactly as rekordbox does. `sync_state`. Files are frozen after sync by default.
- The experimental setting, off by default: read rekordbox's `master.db` through `rbl-db`. It only ever opens a copy and is marked experimental in the interface ([decision 0008](decisions/0008-rekordbox-readback-xml-first.md)).

Acceptance
- Round-trip tests on synthetic XML, including every character in the encoding table.
- The owner imports a generated XML and the new tracks appear, with no duplicates of existing entries.
- On a copy of the owner's database, the experimental reader's counts agree with the XML read-back (aggregate numbers only).
- A test proves the experimental reader never opens the original file.

### 6. Player

Scope
- Decode (symphonia) and output (cpal), waveform peaks computed once and cached in a compact binary form, the waveform with cue markers from the rekordbox read-back, seek by click, keyboard transport.

Acceptance
- Time from play to first audio under 150 ms for a local file, measured on the desktop.
- Cue markers match the read-back for a sample of tracks (owner check).

### 7. Online lookups

Scope
- The provider interface, the response cache and per-provider throttles; MusicBrainz, AcoustID (fingerprints with rusty-chromaprint), Discogs (a token the user enters) and the Bandcamp release page. Release-level matching.

Acceptance
- A fake server proves the throttles hold (1 request per second for MusicBrainz, 3 per second for AcoustID, 60 per minute for Discogs) and that backoff follows 429 and 503.
- On the hand-checked set, coverage rises and wrong auto-files stay at zero.

### 8. Dedupe and upgrades

Scope
- The fingerprint index and version-aware title matching. Upgrades: replace with the better copy, keep cues, archive the old file. A synced track needs the opt-in database mode to fix its rekordbox path; otherwise it waits in review.

Acceptance
- Remixes and extended mixes are never merged with the original (tests on synthetic cases).
- An upgrade keeps cues and leaves the old file in the archive, outside the synced folder.

### 9. Installers, onboarding and user docs

Scope
- Windows and macOS installers, first-run setup (library root, download folders, the rekordbox export file), user documentation, an accessibility pass, and the performance gate in CI.

Release gates
- Every budget in decision 0004 met on the 100,000-track benchmark and on the owner's real library.
- No known data-loss bug. A full dry run of the import pipeline on a copy of real downloads.
- User documentation for each screen.

## Test strategy

| Layer | How |
| --- | --- |
| Pure logic (parsing, scoring, sanitizing, templates, path rules, rekordbox XML reading and writing, `Location` encoding) | Unit tests, plus property tests for the sanitizer (output valid on Windows, macOS and Linux, within limits, idempotent) |
| Database | Tests on temp files. A migration test from every older version. The benchmark on a synthetic 100,000-track library |
| Filesystem steps | Temp folders. Failure injection between steps. A read-only check for the scanner |
| Audio | Tiny synthetic files generated at test time |
| Providers | A fake local server with scripted rate limits and failures |
| Interface | Unit tests for data logic. Scripts for frame-time and search-latency measurement on the mock API. Manual passes on Windows |
| Real data | Run by the owner on the desktop, aggregate output only, written up as research notes |

## Risk register

| Risk | Effect | Mitigation | When |
| --- | --- | --- | --- |
| rekordbox `Location` spelling differs from what the writer produces | Duplicate collection entries | Reproduce the observed encoding, test on the real rekordbox before relying on it | 5 |
| rekordbox updates change the XML or `master.db` layout | The read-back breaks or reads wrong data | Synthetic fixtures per format version. `master.db` stays experimental and copy-only | 2, 5 |
| Windows 260-character path limit | Files that cannot be opened or synced | Enforced in `organize`, shown in the plan, long-path detection | 0, 3 |
| The sync tool copies half-written files or a live database | Corruption on the NAS | Staging and database outside the synced folder, atomic renames | 3, 4 |
| A sync tool that mirrors deletes and moves | Loss on both sides | Archive instead of delete, nothing moves in the existing library | 4, 8 |
| CI minutes on a private repository are limited, and macOS runs count for more | CI cost or queueing | Current Rust workflow runs both OSes on push, pull request and manual dispatch. Measure usage and check the account's allowance before changing triggers | 0 |
| rbxport crates are GPL-2.0-or-later | License mismatch | GPL-3.0-or-later is compatible; confirm per crate when reusing | 5 |
| ffmpeg license and LAME | Cannot ship | Check the build's configuration and license before choosing it | 4 |
| Fake-lossless false positives | Good files flagged | Flag only, tune on samples before any blocking | 4 |
| AcoustID is free for non-commercial use only | Not usable if the app is ever sold | Fine for a free app. Register the app key and tell them before release | 7 |
| Very large lists in a webview | Lag | Windowing, virtualization, the benchmark in CI | 0, 9 |

## Out of version 1

Serato, Traktor and Engine sync. Streaming playback from the NAS. Any Navidrome or AudioMuse-AI integration. Playlist editing and playlist files (rekordbox owns playlists). BPM, grid and key analysis (rekordbox does that). Light theme. Linux builds. A mobile app. Cloud features.

## Questions for the review

1. Is the milestone order right? In particular: the rekordbox read-back (2) before the import pipeline (3), and the player (6), which now needs only the read-back, before the rekordbox export (5).
2. Should the import pipeline ship a dry run first (3) before it can move files (4), or is that a detour?
3. Is the "no wrong auto-file" bar realistic, and is the dry-run set of about 200 downloads enough?
4. CI on a private GitHub repository: are the free-plan minutes enough for a Windows and a macOS job?
5. Is windowed reading by position enough for 100,000 rows, or should the first release add a position index?
6. Is the scoring policy in [decision 0002](decisions/0002-metadata-lookup-strategy.md) strict enough to start and not so strict that nothing files itself?
7. Which of the unverified items in [ASSUMPTIONS](ASSUMPTIONS.md) would change the plan if they turned out wrong?
