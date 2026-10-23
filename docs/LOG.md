# Progress log

Newest first. Each entry says what was done, what was measured, and what comes next.

## 2026-10-23 (rekordbox database read, backups and restore; branch rekordbox-db-read)

- Plan: `docs/superpowers/plans/2026-10-23-rekordbox-database-read.md`, the first slice of the rekordbox sync design.
  Built on the branch `rekordbox-db-read` (not merged, not pushed).
- rbxport's `rbl-db`, `rbl-core`, `rbl-anlz` vendored unchanged at `1a0a8246` (decision 0015). SQLCipher's SQLite
  3.51.3 now serves the whole app; it returned wrong rows for a correlated `EXISTS` filter with `OFFSET`, so the paged
  library filters use `IN`, guarded by `every_page_is_a_slice_of_the_whole_list`. Cold first windows of Releases and
  Artists went from about 1 to 7 and 5 ms (research 008 addendum).
- `oc-rekordbox::db`: snapshot (copy `master.db` and its log while rekordbox and its agent are closed, fold, check),
  read a copy into the XML collection shape (tracks, ratings, colours, My Tags, cues, playlist tree), backups (keep 20
  plus pinned), restore (checks the backup, saves the current library pinned first, removes the old log before the
  restored file goes in). 20 tests on rbl-db fixtures, the log copy mutation-checked.
- Host and interface: Settings switch "Let openCrate read rekordbox's library directly" behind a warning, recorded with
  the rekordbox version (a new version asks again); "Read from rekordbox" on the rekordbox screen; backups list with
  pin and restore. One rekordbox source at a time (A106).
- Laptop check (research 010): rekordbox 7.2.19, 503 of 503 tracks and 96 of 96 playlist nodes read through a backup
  copy in 0.2 s; the live `master.db` unchanged afterwards.
- Checks: fmt, clippy, 403 Rust tests (7 ignored), 54 unit, 57 browser runs.
- Next: compare a database read with an XML export of the same library field by field; then the next plan (send new
  imports to rekordbox's "Imported" playlist through XML).

## 2026-10-23 (edits filed under the editor; rekordbox sync design)

- Owner answers: remixes go under the remixer, and credited edits now do too ("Song (X Edit)" files under X); a VIP
  stays with the original artist (A88). Generic cuts such as "Short Edit" or "Extended Club Dub" no longer read the
  descriptive words as an artist. Decision 0013 approved. The agreed rekordbox sync design is in
  `docs/design/rekordbox-sync.md` (not built).
- The two plan review files are removed; what is still open from them is in `docs/review-findings.md` (full text in git
  history at `3f1395b`).
- Checks: fmt, clippy and 374 Rust tests pass. Before committing, the tree was scanned for audio, databases, archives,
  keys, tokens, personal paths and names from the owner's sample folder: none found.

## 2026-10-23 (waveform with cue markers; player fixes)

- Waveform: `oc_player::peaks` decodes a file once into 800 one-byte peaks (largest sample per slice); schema v13
  `waveform` stores them per file with its size and modified time, so they are made again only when the file changes
  (Game Plan: peaks are stored, never recomputed by the player). `library_waveform` serves them. The player bar draws
  160 bars, the played part in the accent colour, with rekordbox's cues as markers (hot cues green, memory cues white,
  name on hover). The native slider sits invisibly on top, so dragging, clicking and the keyboard all seek.
- A screenshot showed two player bugs, both fixed and tested: pressing Next onto a track whose file is missing showed its
  title while the previous track kept playing (a failed load now stops the player); and a status from before a load
  that said "ended" could skip the new track (statuses for another track are ignored; a stale "ended" is simulated in the
  test, which fails without the guard). At the end of a track, unplayable tracks after it are skipped on purpose (up to
  10); pressing Next does not skip.
- Tests: 3 peaks (folding, a silent file, a loud middle found in the middle with real ffmpeg audio), 1 database (stored
  once, made again after the file changes), 2 unit, 2 more browser flows; player tests run in Chromium and WebKit.

Workspace: 374 Rust tests (6 ignored: benchmarks and live checks), 52 unit, 53 browser runs; strict clippy clean.

## 2026-10-23 (playback)

- `oc-player` (new crate; symphonia 0.5.5 with AIFF, WAV, FLAC, MP3, AAC, ALAC and MP4 only, and cpal 0.18, both built
  offline from the local cache): one engine thread decodes half a second ahead into a buffer, the sound device drains it,
  and a linear resampler covers a device that runs at another rate. Play, pause, seek, stop and volume; a track that ends
  says so; a damaged packet is skipped; an unreadable file is an error in the status, never a panic. The device opens when
  the first track loads, in 32-bit float. Files are only read.
- Host: `LibraryService::track_file` (a present, active file in a reachable folder, lossless first), `player_load` and
  `player_control`. Player bar: double-click or Enter plays a track; the play button plays the selected track or
  pauses; next and previous follow the list the track was played from (previous restarts after 3 seconds); the next
  track starts on its own at the end; seek bar with times; volume remembered in this browser; Space plays or pauses
  unless typing. A missing file is explained in the bar.
- WebKit (the macOS app's engine) does not focus a button on click, so Enter and the arrow keys did nothing after
  clicking a track there. Rows now take focus on click. Found by running the player tests in WebKit (6 of 6 failed
  before, 3 of 3 pass after); the player tests now always run in WebKit too.
- Tests: 9 `oc-player` (stereo mixing, resampling across blocks, WAV and AIFF length and seek, other output rate, bad
  files, play/pause/seek/end/restart, start partway, volume and mono device), 1 database, 1 host, 2 browser flows in
  Chromium and WebKit. The auto-advance check fails with auto-advance removed (checked). One ignored live test played a
  silent file on the real default output.
- Not yet: a real waveform with cue markers (Game Plan player v1), gapless playback between tracks.

Workspace: 370 Rust tests (6 ignored: benchmarks and live checks), 50 unit, 49 browser runs; strict clippy clean.

## 2026-10-23 (the fake-lossless check)

- `oc_convert::spectrum` (rustfft, built offline from the local cache): decodes 30 s of a lossless file with ffmpeg and
  finds the highest band within 40 dB of the 1 to 8 kHz level. Below 19 kHz: "probably made from a lossy file"; 19 to
  20.5 kHz: "may have been made from a high-bitrate MP3 or AAC". Shown as a warning in the preview and therefore in the
  Import all review; it never blocks. Edges are cached by path, size and modified time, so the import does not decode
  again after the preview.
- Measured on synthetic noise through real encoders (research 009): MP3 at 128, 192, 256 and 320 kbps all caught (16.8,
  18.8, 19.5, 20.2 kHz); lossless at 22.0 kHz; ffmpeg's AAC at 256 kbps not caught. About 47 ms a file.
- Tests: 3 in `oc-convert` (a synthetic band edge, verdict wording, real MP3s dressed as FLAC), 1 host (a fake FLAC in a
  release is flagged and its real neighbour is not; both still import). The host test fails with the threshold broken
  (checked).

Workspace: 359 Rust tests (1 more ignored, live), 50 unit, 45 browser runs; strict clippy clean.

## 2026-10-23 (track numbers from more file names; a read-only library check)

- File-name guesses: "03 Title" now gives track 3 when the number is two digits, followed by a space, with no " - "
  after it (so "50 Cent - Title" is an artist) and, above 40, only when zero-padded ("99 Problems" stays a title);
  Bandcamp's "Artist - Album - 03 Title" gives artist, album, track and title, and the album from the name wins over the
  folder's. Track 0 is never guessed.
- Settings, "Library check" (`oc_core_db::health`, `library_check`): reads the catalog only and lists folder names that
  "Tidy names" would change, and recordings filed more than once (same artist and title, lengths within 2 s; an extended
  mix of the same title is not a duplicate). Run once on a backup copy of the app's database at the owner's request: it
  found the two ";" artist folders and the duplicated track the owner's sample has. The copy was deleted.
- Fixing what the check finds is not built: moving files would break the paths rekordbox and the NAS copy hold, so it
  waits for the owner's decision (relocate with an updated rekordbox XML, or leave as is).
- Tests: 1 guesser (bare, Bandcamp, and names with numbers left alone), 1 database (untidy folders, a duplicate, an
  extended mix not counted, nothing on disk changed), 1 unit, 1 browser.

Workspace: 355 Rust tests (1 more ignored, live), 50 unit, 45 browser runs; strict clippy clean.

## 2026-10-23 (looking tags up in MusicBrainz from the Import all review)

- The owner asked for Import all to offer filling in uncertain tags. Decision 0014: on request only. The review now has
  "Look up N releases on MusicBrainz" for releases with doubtful tags; each answer shows the release found and, per
  file, the values that would change (old struck through, new beside it) with a tick box. Ticked values travel with the
  import (`ImportOptions.accepted`), decide the folder and file name, and are written into the staged copy.
- `oc-lookup` (new crate, no network code): search and release URLs, reading answers, choosing a candidate, matching
  tracks to files. Host client in `src-tauri/src/lookup.rs` (ureq with rustls, built offline from the local cache):
  1.1 s between requests, User-Agent, two retries on "busy", 4 MB cap.
- Live check against MusicBrainz (two releases' worth of requests, a well-known public album, synthetic files): the
  shape of real answers matches; the first run picked a reissue because ties kept the last candidate, now the earliest
  year wins (tested). One request during development got "busy", hence the retries.
- Tests: 6 `oc-lookup`, 3 lookup host, 1 import with accepted values, 2 unit, 1 browser flow (look up, see a busy
  release explained, untick a file, import sends only the ticked values). One ignored live test, run by hand.

Workspace: 353 Rust tests (1 more ignored, live), 49 unit, 44 browser runs; strict clippy clean.

## 2026-10-23 (import all, converting, tag writing, tidy names, folder layout)

- Owner asks: an "Import all", converting by kind, tags checked before a batch, ID3 version, stripping stray ";" from
  artist names, and a choice of folder layout.
- ID3 version: the owner's sample folder (read-only count, at the owner's request) holds 6 MP3s in ID3v2.3, 2 in v2.4 and 2
  untagged AIFFs; v2.3 is also what rekordbox reads most reliably. Tags are written as v2.3 (owner question 2 answered).
- Bad names in the sample folder (owner's request): two artist folders carry ";" from separator-joined artist values
  written by an earlier tool (one trailing, one joining two artists); the second holds a duplicate of a track filed under
  the plain artist folder. The files' tags are clean now. This entry records only the pattern.
- `oc-convert` (new crate): one target for lossless files (Keep, AIFF, WAV, FLAC, MP3 320) and one for lossy (Keep, MP3
  320); a file already in its target format is never converted; ffmpeg found on `PATH` or the usual Homebrew and system
  places (a sidecar later, per the Game Plan); audio only, 16 or 24 bits by source depth; an existing output is refused
  (ffmpeg's `-n` reports success, so the check is explicit). Tested with real ffmpeg on all four formats.
- `oc-tag`: `tidy` (artist lists split on ";", trailing `; , / \ | -` and padding removed; "+44", "C++", "AC/DC" kept) and
  `write_tags` for staged copies only (chosen fields replaced, others kept, cover added only if none, ID3v2.3). Two lofty
  0.22 problems found and worked around, each with a test: a plain year is silently dropped when writing v2.3 (written as a
  timestamp frame instead, which becomes TYER), and a picture with no description gets a malformed UTF-16 terminator that
  readers reject (described as "Cover"). Checked with ffprobe as an outside reader.
- Import (`plan.rs`, `import.rs`): the plan takes the choices and the saved layout and decides per file the format, the
  place and the tag fields to write (everything for a converted file; for a copied one only values it lacks or that were
  tidied, so a clean file still goes in byte for byte). Converted files get the source's picture or the release image.
- Schema v12 `app_setting`; Settings has "Folder layout": five presets (default Artist / Album / Track) and a custom
  pattern, checked with a live example; a bad pattern is refused with the parser's reason.
- Import screen: "When importing" card (choices remembered in this browser; disabled with a reason without ffmpeg);
  preview "Becomes" column and a "*" on tidied values (hover shows the original); "Import all N finished releases"
  reads every plan first and shows a review (files going in, files staying, files with warnings or values taken from the
  file name), then imports one by one with progress and scans once.
- Not yet: looking tags up online (MusicBrainz) from that review; fixing folders already in the library; the fake-lossless
  check before converting; guessing a track number from "03 Title" without a dash.

Workspace: 342 Rust tests, 47 unit, 43 browser runs; strict clippy clean.

## 2026-10-23 (the first real import; a line under each page title)

- Import (`src-tauri/src/import.rs`, `library_import_release`): the preview now has "Import N files". Each ready file is
  copied into `.opencrate-staging` beside the music folder (same drive, checked; never inside the library), verified by
  size and SHA-256, then published with `oc-publish` through `LibraryService::publish_file` (recorded first, hard-linked
  without ever replacing a file, staged name removed). The plan is made again at import time; files with a problem stay
  in the download folder and are listed with the reason. The originals are only read. Tags are not written yet: files go
  in as they are. A scan of the music folder follows, and the lists refresh when it ends.
- Startup runs `recover_publications`, so an import a crash cut short is finished or set aside. Leftover partial copies
  in staging are swept before each import.
- Schema v11 `imported_release`: imported releases leave the "new" list for an "Imported" one (cleared with their folder).
- Owner asked for an orange line under Tracks, Releases and Artists saying what the page holds; added (not shown inside a
  crate, playlist, release or artist).
- Tests: 4 host (byte-for-byte filing with originals and staging checked, an existing file never replaced, a file that
  arrives between plan and publish left alone, partial copies swept), 1 database, 1 unit, 1 browser flow.

Workspace: 329 Rust tests, 44 unit, 39 browser runs; strict clippy clean. Owner checked the drag animation and multiple
artists in the app: good.

## 2026-10-23 (every artist shown; drag that makes room)

- Multiple artists: the tag reader keeps every artist and album artist (separate items, or ID3v2.4's NUL-separated values),
  drops case-only repeats and joins them with ", "; names are never split on "/" or "&" ("AC/DC"). Schema v10 clears the
  recorded tag-read times so the next scan reads every file's tags again (files are only read). Test in ID3 and Vorbis;
  fails under the old single-artist read (checked).
- Rearranging: rows slide apart to open a gap where the track will land (160 ms, none with reduced motion), the dragged
  row's slot closes, the drag shows a small title and artist label, and the new order is shown at once on drop. The landing
  spot is computed from the pointer's position in the list, not from the moving rows, so it does not flicker. A drag that
  ends without a drop leaves nothing slid (tested, and the slide check fails with the slide turned off).

Workspace: 324 Rust tests, 43 unit, 38 browser runs; strict clippy clean.

## 2026-10-23 (drag fixed in the real app; Custom order; plain copy; resizable sidebar)

- Dragging onto a crate did nothing in the app: Tauri's native drag-drop handler swallows in-page drags. `dragDropEnabled:
  false` (openCrate takes folders from the picker, never from Finder drops). Playwright now also runs the drag tests in
  WebKit, the engine of the macOS app. That exposed a real bug: the drop side was read from state set by the last
  dragover, which may not have rendered yet, so a track could land on the wrong side of the row; it is now taken from the
  drop event (three clean full runs after).
- Custom order for the whole track list (schema v9 `library_order`, `Sort::Custom`, `library_move_tracks`): the first drag
  starts from the order on screen, so nothing else moves; tracks added later follow the arranged ones; 15-27 ms to arrange
  20,000 tracks (release build). The list keeps its scroll position across the switch to Custom order.
- Copy: slogans removed ("GOOD MUSIC. LESS FRICTION.", "Find your people" and the like); titles say what the screen is
  (Tracks, Releases, Artists, Import, rekordbox); "synthetic" no longer appears for real libraries; window title "openCrate".
- Sidebar: crate rows are as wide as the navigation; the width can be dragged (180-420 px, arrow keys, double-click resets,
  remembered in this browser); the lists region scrolls on its own.

Workspace: 323 Rust tests, 43 unit, 38 browser runs; strict clippy clean.

## 2026-10-23 (crates with drag to reorder; seeing the real window)

- Crates (owner's choice over editing rekordbox playlists or a play queue): schema v8 `crate` and `crate_item` (a track at most
  once per crate; entries cascade away when a track leaves the library), `oc_core_db::crates` with a pure `moved` rule (moved
  tracks keep their crate order and land before the anchor, or at the end), `LibraryQuery.crate_id` and `Sort::Crate`, seven
  commands (at most 10,000 ids per call). Sidebar section: "+" names a new crate, right-click (or the menu key) renames or
  deletes (tracks stay), dropping tracks on a crate adds them. Inside a crate in crate order: drag rows to reorder (drop line,
  edge scrolling), Alt+Up/Down moves the focused track, Delete takes it out; reorders refetch in place without losing the
  scroll position. Tests: 1 pure, 4 database, 1 host command, 1 browser flow (make, drag in, reorder by drag and keys,
  remove, rename, delete). The sidebar lists now scroll as one region so the navigation never shrinks (seen at 640 px).
- The real window can now be captured (window only, never the rest of the screen), and was used to check the crates section
  and the play button. The play triangle was drawn 2.5 units right of centre with a padding hack that did not compensate; the
  path is now centred with a one-unit optical nudge.

Workspace: 321 Rust tests, 43 unit, 34 browser; strict clippy clean.

## 2026-10-23 (preview table like a spreadsheet; "already there" without dates)

- The preview table (`src/components/PlanTable.tsx`): drag a heading's edge to resize (double-click or Home resets, arrow
  keys step), right-click a heading (or the menu key, or Shift+F10) for a checklist of columns plus "Reset column widths".
  Title cannot be hidden. Widths and hidden columns are remembered in this browser. Browser test drags, uses the keyboard,
  toggles from the menu and reloads.
- The owner's folder still showed copies as already there: it was added before snapshots existed (read-only check of the
  test data folder: schema 7, `baseline_taken = 0`). The modified-time rule is gone: a folder without a snapshot assumes
  nothing is old, and Settings has "Count what's here as already there", which takes the snapshot on request.

Workspace: 315 Rust tests, 43 unit, 33 browser; strict clippy clean.

## 2026-10-23 (owner feedback: baseline, artist covers, tag columns)

- A file copied into a download folder showed as "already there": a copy keeps its old modified time, the A94 risk. Schema v7
  `watch_baseline`: adding a download folder records its top-level names, and only those count as already there; folders
  added earlier keep the modified-time rule until removed and added again. Test copies a file dated 2001 in after adding
  (fails under the old rule, checked).
- Artists show a collage of up to four of their releases' covers (one; two side by side; one over two; two by two), chosen
  from releases that have a cover (`library_artist_covers`, cached per artist); geometry checked in the browser.
- The preview shows tags as columns, decluttered: title over artist, album artist only when different, short format
  (`AIFF 16/44.1`, `MP3 320 kbps`) and length, faint dashes for missing values, a "?" for values from the file name, notes on
  a line under their file. A Columns menu chooses which show (remembered in this browser; "#" off by default).
- The wide preview table scrolls inside its card instead of pushing the screen sideways (test fails without, checked).

Workspace: 314 Rust tests, 43 unit, 33 browser; strict clippy clean.

## 2026-10-23 (owner feedback: dialogs, Import timing and layout, tags, album art)

- Settings and the cues dialog: the header and close button stay put; only the body scrolls (browser test at 420 px high).
- Import opened with "Looking at your download folders..." for about 3 seconds: a shared in-flight flag let the request of
  React's discarded first effect run (development double-run) block the live one until the next 3-second tick. The browser
  suite runs the production build, where effects do not double-run, so it could not see this; the polling is now
  `src/library/poller.ts` with unit tests for start-stop-start, overlap and late answers (the double-start test fails with a
  shared flag, checked).
- In a short window the Import cards were drawn over the screen's header: the grid squeezed its rows into the fixed height.
  `grid-auto-rows: max-content`; a browser test checks sections never overlap (fails without the fix, checked).
- Tags before import: each previewed file shows its tags (artist, title, album, album artist, track, disc, year, label, genre,
  key, BPM), which values came from the file name rather than the tags, format, rate, depth, length and bitrate, and
  warnings for common download mistakes ("Artist - Title" in the title, a file extension in the title, artist equal to title,
  web addresses, track 0, an implausible year, unreadable tags). Warnings do not block; problems do.
- Album art: `oc_tag::{embedded_cover, folder_cover}` (front cover first, else first picture; else cover/folder/front/artwork/
  album image beside the file; type from the bytes, only JPEG, PNG, WebP and GIF, at most 12 MB, links not followed) and an
  `artwork://` handler in the host that takes only a release number, reads only indexed files, caches covers (misses for
  30 s) and answers 404 so the window keeps its drawn cover. CSP `img-src` allows it. Rows, release cards, the header and the
  player show the real cover; artists keep the drawn one.
- "App crashed" report: no crash report and nothing in the system log; the current build starts cleanly on a new folder and
  on a schema-5 database upgraded to 6 (scratch copies, not the owner's folder). The development app restarts whenever the
  Rust code is rebuilt, which closes the window; a real crash still needs the terminal output to diagnose.

Workspace: 312 Rust tests, 42 unit, 32 browser; strict clippy clean.

## 2026-10-23 (Import screen: scrolling, speed, and a dry-run preview)

Owner feedback after a first look: the Import screen did not scroll in a small window (the container now scrolls; the browser
test fails without the fix), it felt slow to open, and there was no way to proceed. Speed: measured first. The lister takes
17 ms for 5,200 files and a whole poll 30 ms for 4,800 files in a debug build, so the cost was not the listing; the screen
was blank until the first answer and every file waited out the 10-second check. Now the last answer is remembered while the app
runs, a loading line shows, and a file whose own modified time is at least 30 seconds old and which has not changed since
first seen counts as finished at once (`trust_old_mtime_ms`, off when `None`; 2 tests; A92 updated). Proceeding: a
**Preview** button per release calls `library_plan_release`, which reads the tags (falling back to the path layout), applies the
default template and the Windows path limit, and lists each file's proposed library path with plain-word problems: no
artist, title or track number, still downloading, an archive, a file already at the destination, two files for one place.
It reads only, takes ids and a plain release name (checked against `..`, separators and links), and writes nothing (tested).
Tests: 4 host, 2 tracker, 1 unit, 1 browser. Workspace: 299 Rust tests, 37 unit, 30 browser; strict clippy clean. Not built:
the import itself (copy, tag, publish).

## 2026-10-23 (R1: the SQLite store for publication)

Schema v6 `publish_operation` and `oc_core_db::publish_store::PublishStore`. `begin` fixes a staged file's identity and writes
the `Planned` row; `Executor` saves the `Planned` record before anything else so a crash right after planning is
recoverable; `commit_record` runs the caller's catalog hook and the phase change in one immediate transaction, so a hook
that fails leaves neither (tested: the hook wrote a row first and it is gone). `Store::unfinished` now returns a `Result`,
because an empty list on a read error would be read as "nothing to recover". Tests (6) restart for real: the database is
dropped without a clean close, reopened (quick check after an unclean shutdown), and recovery from the table finishes
the job once for a crash at each of the 8 points, with the catalog change called exactly once; rejections are stored
with their reason and not retried; paths round-trip as raw bytes. Workspace: 293 Rust tests, strict clippy clean.

## 2026-10-23 (oc-publish: the R1 publish and recovery protocol)

New crate `oc-publish`. Per file: plan (size and SHA-256 fixed from the staged file), record intent, hard-link the staged
file to the destination (a link fails if the name exists, so publication is no-clobber in one atomic step), commit "record"
(the catalog change and the phase in one transaction), then remove the staged name. Running and recovering are one loop
that decides from what is on disk, so every crash point converges: it never overwrites an occupied destination (rejected
with the staged file kept), never deletes the staged name unless the destination still holds the planned content, and
publishes exactly once. `stage_copy` brings a file to the destination volume under a `.part` name, verifies it, and
renames it without replacing anything; `sweep_partials` clears leftovers. Tests (13): crash at each of 8 points, a second
crash during recovery for every pair of points, a file arriving between check and link, source edited after planning,
destination removed or replaced after recording, 8 operations recovered together, crash while copying. Two mutations of
the code were run and failed the tests as they should. Workspace: 287 Rust tests; strict clippy clean on macOS and the
Windows target (type check only; the hard-link behaviour on NTFS is unobserved). Not built: the SQLite `Store`, release-level
behaviour (several tracks and remixer folders as one unit), the owner's choice of fallback on volumes without hard links.

## 2026-10-23 (cues and beat grid on screen)

`mirror::read_track_analysis` (cues in time order with fractional seconds kept, tempo points in file order, from the
newest-read export whose entry matched one of the track's files; nothing when none did), `LibraryService::track_analysis`,
command `library_track_analysis` (refused to a remote origin) and a "Cues and grid" dialog opened from the player bar for a
selected track. Hot cues are lettered A-H, memory cues and loops are labelled as rekordbox numbers them (Type 0 to 4,
Num -1 for memory), positions show as m:ss.mmm and the stored value is not rounded. It is a read-only view: openCrate
never edits a cue or grid (rekordbox owns them). Tests: 2 database, 1 host (extended), 2 unit, 1 helper unit file, 1 browser.
Workspace: 274 Rust tests, 36 unit, 29 browser; strict clippy clean. Not built: waveform, cue editing, master.db cues.

## 2026-10-23 (Import screen: a read-only view of the download folders)

`oc-ingest::list_folder` (read-only; does not follow links; depth 6; 100,000-entry cap; unreadable subfolders skipped and
counted), `ReleaseStatus.newest_modified_ms`, host module `downloads` (in-memory trackers per folder, so a restart makes
every file wait out the quiet period again) and command `library_download_releases`. The Import screen polls it every 3 s
while open and shows new releases with Ready / Still downloading / Checking it has finished, and, apart, what was
already there when the folder was added (modified time at or before the day it was added, A94). Nothing is copied or
changed. Tests: 6 oc-ingest, 3 host module, 1 host command (also refused to a remote origin), 1 unit, 1 browser.
Workspace: 272 Rust tests, 33 unit, 28 browser; strict clippy clean. Not built: file-system events, staging, import.

## 2026-10-23 (download folders in Settings)

Schema v5 `watch_folder` and `LibraryService::{add,remove,set_enabled,check}_watch_folder(s)`. A download folder must exist
and be readable, and may not be, contain or lie inside the music library, another download folder or the data folder;
the library is likewise refused over a download folder (new `Error` variants with plain messages). An unreachable folder
stays listed as offline and returns when the drive does; removal touches nothing on disk. Five Tauri commands (the picker
is opened from Rust as for music folders), a `Downloads` section in Settings, and tests: 4 database, 1 host, 1 unit,
1 browser. Workspace: 263 Rust tests, 32 unit, 27 browser; strict clippy clean. Not built: anything that watches or lists
these folders (the `oc-ingest` rules are ready for it). The stored columns are fewer than data-model.md lists (A93).

## 2026-10-23 (oc-ingest: partial files, stability, releases)

New pure crate `oc-ingest`: `classify` (temporary names win over the audio extension inside them; litter ignored;
archives and cover art recognised; works on raw bytes), `StabilityTracker` (the caller reports what it saw and when; a
file is stable after 2 identical sightings spanning the quiet period; an empty file is never stable; a clock that goes
backwards restarts the wait) and `evaluate` (groups a listing into releases and reports ready, or what it waits for).
23 tests; strict clippy clean on macOS and the Windows target. Not built: the watcher and the listing itself.

## 2026-10-23 (offline folders, missing files, startup errors, reserved database folder)

Milestone 1 acceptance, "unplugging the drive shows a banner, keeps the library browsable and marks nothing missing":
`LibraryService::check_roots` (read-only, writes only when the answer changed) and command `library_check_roots`; the UI
checks on load, on window focus and after scans and shows a banner with "Check again". A folder that exists but is not
the one that was added (recorded directory identity differs, as with a recreated folder or another drive) is
`unverified`; it is never scanned until the owner presses "Use it anyway" (`confirm_root` re-records the identity).
Tracks whose files are all gone show a dimmed row and a "Missing" tag (`LibraryItem.missing`); an offline folder is not
a missing file (tested). Also: a failed `OPENCRATE_DATA` or `OPENCRATE_DEMO` now shows a banner in the window instead of
only printing to stderr; the service reserves its own database folder, so no library root can contain it, be it or lie
inside it (a database inside a synced library was the case decision 0003 warns about); refusal messages are plain
sentences. A code audit listed every write in non-test Rust: only the database folder, the app's own temporary demo
folders and snapshots next to the database; no network code. Tests: 3 database, 2 host, 3 unit, 3 browser (26 total).
Workspace: 235 Rust tests, 31 unit, 26 browser; strict clippy clean on macOS and, for the four pure crates, the Windows
target. 0 of 100 database-suite runs failed. Not built: automatic periodic checks while the window is idle.
Native check of the rebuilt debug bundle (macOS arm64): the app stayed alive for 8 s in four modes (default, persistent
library, an invalid relative data folder that printed the error and stayed up, and a 2,000-track demo database); no demo
folder was left behind. This shows process liveness only, not what the window rendered.

## 2026-10-23 (oc-organize: pure naming rules, slice 0f)

New pure crate `oc-organize` (no file access): `sanitize` (forbidden characters become `_`, never deleted; NFC; 100
characters and 240 bytes per component; Windows device names defused including superscript digits; no trailing dots or
spaces; idempotent, with a 5,000-case random property test for validity and idempotence), `template` (`{field}` and
`{track:02}` with a typed error for a missing value; a `/` inside a value never adds a level), `fit` (Windows limit counted in
UTF-16 units over the whole path including the root, a reserve for the temporary suffix, only the file stem gives
way, an over-long root or folder is an error), `title` (version and remixer parsing over a 30-row synthetic corpus,
"no version is Original Mix", `folder_artist` per A12) and `destination` (render then fit). 27 tests; clean under strict
clippy on macOS and the Windows target. Found and fixed: a bare `(Remix)` lost its kind. Policy calls in A88-A90 below
need the owner. Nothing here moves or renames a file; that stays behind the R1 recovery protocol.

## 2026-10-23 (playlists in the sidebar and as a track filter; Windows type-check)

`LibraryQuery.playlist_id` and `Sort::Playlist`: tracks that the rekordbox export lists in a playlist and that match a
file, in the playlist's own order (first occurrence; the query builder now binds an ORDER BY parameter between the
filter and the window parameters), still combinable with search, format and the other sorts. `read_playlists` and
`LibraryService::playlists` give the tree with entry and in-library counts; command `library_playlists`. UI: a nested
sidebar tree (rekordbox's invisible `ROOT` hidden, folders collapsible, counts, active item marked), a playlist header
("38 of 40 entries are in your library"), "Back to tracks" and a "Playlist order" sort; the library label no longer
says "Synthetic" for a database. A playlist shows a track once even if the export lists it twice. Tests: 1 database, 2
unit, 1 browser (22 total); 203 Rust tests + 1 more, strict clippy clean.

Windows evidence (new): `oc-paths`, `oc-tag` and `oc-rekordbox` pass `cargo check` and strict `cargo clippy
--all-targets` for `x86_64-pc-windows-msvc` (type-check only, nothing run), so the WTF-8 glue and path-based file
identity compile. `oc-core-db` and the Tauri host could not be checked (their C build needs a Windows toolchain);
hosted Windows CI has never run.

## 2026-10-23 (remove a folder, confirm a held disappearance, scan timing)

`Database::remove_root` / `LibraryService::remove_root` / command `library_remove_root`: one transaction removes the
folder's index entries, aliases and the tracks only those files made (and their search rows); rekordbox entries that
pointed into it become "not in your folders"; empty artists and releases stay but are not shown; nothing on disk is
touched; refused while a scan runs. `library_scan` takes `confirmMissing` so the owner can accept a held mass
disappearance. UI: Remove with an inline confirmation, and "Mark them missing" after a held scan. Tests: 1 database, 2
host, 2 browser (now 21), unit tests extended. Workspace: 203 Rust tests, 27 unit, 21 browser, strict clippy clean.

Measured (`cargo test -p oc-core-db -- --ignored --nocapture benchmark_scan_9000_files`, dev profile, Apple M3 Pro,
9,000 synthetic 4 KiB tagged WAV files in a temp folder, warm OS cache): first scan 0.57 s (walk, index, tags, catalog),
unchanged rescan 0.11 s, database 5.7 MiB. This says nothing about a real library: real files are tens of MB, read
from a cold disk or a NAS, carry embedded art and long tag blocks, so I/O will dominate; tag reads touch a file's
header and tags but a format without a length header may need more of the file. Measure on a copy before trusting it.

## 2026-10-23 (rekordbox Sync screen: import, explain, match)

`oc-core-db`: `LibraryService::import_export` (parse away from the writer, one-transaction replace, a failed read is
recorded and the old mirror stays), `reread_mirror` (from the stored path), `mirrors` with match counts, `add_alias`
(validated Location prefix) and `alias_suggestions`: for each unmapped entry every prefix depth is tried against the
files actually indexed under each folder, so a suggestion such as "8,977 entries start with /G:/Library" is evidence,
not a guess. `LocationPath::to_decoded` is the inverse of `parse`. Host: `sync.rs` and commands `library_mirrors`,
`library_import_xml` (native file dialog from Rust; the page never supplies the path), `library_reread_mirror`,
`library_alias_suggestions`, `library_add_alias`. Frontend: validated admin methods and a Sync screen (export card, four
counts, error banner that keeps earlier data, suggestions with "Use this"); the library reloads and shows rekordbox's
tempo and key. Found and fixed in the UI: a successful reload cleared the error a failed import had just set. Tests: 2
new end-to-end service tests on real synthetic folders (export from another drive prefix, suggestion, accept, tempo and
key appear and sort, unchanged re-read, broken export keeps the old mirror), 1 host test through the commands, 3 unit
and 3 browser tests (screens viewed in Chromium). Workspace: 200 Rust tests, 27 unit, 19 browser; strict clippy clean.
The dialog and a real export are not exercised; no real export was read. Not built: removing a folder, cue and playlist
screens, writing anything to rekordbox (not planned).

## 2026-10-23 (folders in the app: add, scan with progress, browse)

Hardening first: the tag reader catches a panic per file (one bad file no longer drops a whole worker's batch) and the
tag loop ends when no result moved a track, not only when all were stale. `LibraryService::scan_root` reports
progress (checking, walking, applying, reading tags with counts, finished); `roots()` lists folders with file and
missing counts. Host: `folders.rs` (root DTOs, one scan at a time on a background thread with panic protection and a
polled status) and commands `library_roots`, `library_add_root` (the NATIVE folder dialog, opened from Rust, so the
page never supplies a path and no dialog permission is granted), `library_scan`, `library_scan_status`,
`library_scan_cancel`; `library_mode` now answers `{mode, canManageFolders, persistent}`. `OPENCRATE_DATA=<absolute
folder>` opens a persistent library (wins over `OPENCRATE_DEMO`); with neither, nothing changes. Frontend: a validated
admin interface on `TauriLibraryApi` and a Folders panel in Settings (list with availability badge and counts, Add
folder, Scan with progress and Cancel, plain-language summary; the library reloads after a scan). Tests: 16 host tests
(the whole add-scan-read flow and a restart through Tauri's command dispatch; the dialog needs a real window and is not
tested), 24 unit, 16 browser (three new folder flows, rendered and viewed in Chromium). Native (macOS debug
bundle): launched with no env and with `OPENCRATE_DATA=<scratch folder>`, alive after 8 s each, no stderr; the
persistent run created its database (schema applied, WAL, owner lock). The native dialog and what the window
renders are NOT verified (no screen capture, no interaction here). Not built: removing a folder, confirming a held mass-disappearance, the app-data default location (an owner
decision), file watching, a Sync screen.

## 2026-10-23 (tags and catalog: a folder of audio becomes the library)

New `oc-tag` crate (read-only, `lofty` 0.22, MIT/Apache): audio properties and normalized tags (text trimmed
and capped at 512 characters, blank is none, tempo sanity-checked, cover art never loaded), format detected from
content not extension, plus `guess_from_path` for the `Artist/Release/NN - Artist - Title` layout. A `fixtures`
feature builds tiny valid WAV, AIFF, FLAC and MP3 files for tests. In `oc-core-db`: migration 004 (tag
bookkeeping, unique artist and release keys), `catalog` (parallel tag reads away from the writer, one transaction per
batch creating or updating artist, release and track and the search index; an unreadable or untagged file is
catalogued from its path; tempo and key are NOT taken from tags, A21), `LibraryService::add_root` (read-only
validation, volume probe) and `scan_root` (check, walk, apply, read tags, catalog; cancellable), releases and
artists with no tracks hidden. A detected file change now invalidates its tag read. 11 `oc-tag` tests and 6 new
end-to-end tests on a real synthetic folder (7 files: four formats, untagged, corrupt, compilation): catalog,
rescan reads nothing again, retag updates the same track and search, deleted file stays browsable, vanished root
changes nothing, cancel applies nothing. Workspace: 187 Rust tests, strict clippy clean, 0 of 100 suite runs failed.
Benchmark unchanged for tracks; release views +1 to 4 ms from the new filter. Not built: IPC or UI for adding a root
and scanning (needs a folder picker and a capability decision), file watching, progress events, duplicate detection.

## 2026-10-22 (IPC: the UI can read the Rust library)

`LibraryService` (writer thread plus reader pool) in `oc-core-db`; Tauri commands `library_mode` and `library_read` with
strict DTOs in `src-tauri/src/ipc.rs`; the frontend `TauriLibraryApi` (wire mapping, abort, runtime validation) is chosen
in `main.tsx` only when the backend reports `database`, which needs `OPENCRATE_DEMO=<tracks>` (A80). Default behavior is
unchanged. Tested: 8 host tests (4 through Tauri's mock runtime, including refusal of remote origins), 8 new unit tests
(20 total), 4 new Playwright tests with a stubbed bridge (13 total); strict clippy clean. A mock-runtime failure showed
that Tauri refuses app commands to non-local origins, which is the behavior we want.

Native check (macOS debug bundle, `OPENCRATE_DEMO=5000`): the app launched, stayed alive for 9 seconds and wrote
nothing to stderr, twice (before and after the cleanup change). A screenshot was impossible (`could not create image
from display`: no screen-capture access in this session), so what rendered is NOT verified; this is liveness only,
like the earlier bundle checks. Found while checking: killed or normally-exiting apps left `opencrate-demo-*` folders
(79 MiB at 100,000 tracks), because Tauri exits without dropping managed state. Fixed: removal on the exit event, a
startup sweep of such folders older than 24 hours, and tests that use their own folder. The exit-event removal is not
exercised by any test (it needs a real window close); the sweep is. Not built: a real app-data library, root picker,
IPC for scans or the mirror.

## 2026-10-22 (rekordbox mirror)

Migration 003 and `Database::replace_mirror`: `rb_source`, `rb_track` (match status, kind, root, file), `rb_cue` and
`rb_tempo` (fractional seconds as REAL), `playlist`/`playlist_item` (order, duplicates and unresolved IDs kept) and
`track.derived_source_id/derived_generation`. One IMMEDIATE transaction per source; generation advances only when the
export or any match result changed (a state digest covers both), so re-reading an unchanged file writes nothing; tempo and
key go to linked tracks with source and generation and are cleared when a successful read drops the match; a failure
mid-replace rolls everything back; `record_mirror_failure` keeps the old mirror and stores the error. `read_export` parses
a file away from the writer. 11 tests (found and fixed two test-setup bugs, e.g. `LIKE` on a NUL-joined blob). Workspace:
161 tests (78 database, 60 paths, 23 rekordbox), strict clippy clean. Not built: master.db read, UI Sync screen.

## 2026-10-22 (roots, scan reconciliation and Location matching)

The persistence half of the path identity contract, in `oc-core-db`: `Database::add_root`
(raw path, profile, first alias), `add_alias`, `set_availability`, `set_name_profile` (two-phase
rekey, refuses on a collision); `Database::apply_scan` applies one pass: inserts, updates,
respelling within a profile, revival of the same row, `missing` only under directories that
were listed (or whose listed parent shows them gone), skipped entirely for an unavailable or
unverified root, a 25%/20-file hold that the owner confirms, collision demotion to `exact`, and
lifecycle never touched; `Matcher` resolves a Location through aliases to a file by key
without touching the disk. 17 new tests with a deliberate mutation check of the "directory was
listed" rule.

Also `oc_paths::walk_root`, a read-only walker (never follows links, a listing error means "not
listed", skips AppleDouble `._` files, keeps non-text names as bytes, honours depth and
cancellation), `ScanPass::from_walk`, `walk_stored_root` (walk outside the writer) and
`check_root` (unavailable vs unverified by recorded directory identity). 7 end-to-end tests on
real temporary folders: delete, restore, edit, remove a folder, locked folder, link out of the
library, vanished root, an empty folder replacing the library, 30 files vanishing at once.
Workspace: 150 tests (67 database, 60 paths, 23 rekordbox), strict clippy clean, 0 of 100 runs
of either new suite failed.

## 2026-10-22 (library schema v2, windowed queries, benchmark)

`oc-core-db` schema v2 (`002_library.sql`): `library_root`, `root_alias`, `artist`,
`release`, `track`, `file` (raw path BLOB, per-root `path_key`, lifecycle and
presence, no offline state) and three FTS5 tables. `library::read_page` mirrors the
UI's `LibraryQuery` (views, four sorts, format/artist/release filters, word-prefix
search, windows of at most 200); `LibraryReader` caches the ordered id list per
query and follows `data_version`. `synthetic::seed` builds the 100,000-track
catalog. The migration runner no longer snapshots between migrations of a fresh
create (it keyed on `index > 0`). 36 database tests (16 new) plus an ignored
benchmark; strict clippy clean. Measured in [research 008](research/008-library-query-benchmark.md):
first window of a new query at most 19 ms p95, scrolling 0.2 ms, deep direct
windows up to 87 ms. Dev profile, one macOS machine, no IPC or UI. Search is word
prefix, not the demo's substring match (a deliberate call).

Also added the application `Writer` (one thread owns the `Database`; only
`shutdown()` closes cleanly, a plain drop leaves the dirty marker) and `ReaderPool`
(blocking leases of cached readers, returned after a panic). 43 tests; 0 of 150
full-suite runs failed.

## 2026-10-22 (oc-paths: path identity crate, provisional)

New pure crate `oc-paths` implements the proposed R3/R7 contract
([design](design/path-identity-and-availability.md), [0013](decisions/0013-filesystem-identity-and-availability.md),
A67-A78): NUL-joined raw path storage with a WTF-8 codec, three name profiles
(exact, ASCII fold, NFC plus fold), Location shape parsing and alias resolution
(whole-component longest prefix, ambiguity), read-only `lstat` volume probes,
root registration/overlap checks and the lifecycle/presence/match state rules.
49 tests, strict clippy clean, macOS only; Windows branches (WTF-8 glue, file
identity) are written but not compiled here. Research 007 measured APFS name
equivalence. Provisional: written under the owner's overnight instruction before
owner review. Next: library schema v2, windowed queries and benchmark (slice 0d).

## 2026-10-22 (database test instability root-caused and fixed)

Resumed from the stopping checkpoint with the instability first, as HANDOFF
required. The failure reproduced in two of five `cargo test -p oc-core-db --locked`
runs and in 24 of 100 runs of the test binary. It never occurred serially or with
the two child-process tests skipped (0 of 100 each); skipping either one alone
still failed (6 and 5 of 100). A standalone std-only program then isolated the
mechanism: a lock belongs to the open file description, and a child spawned by
another thread holds a copy of the descriptor until it execs, so closing the
owner's descriptor can leave the directory locked. A concurrent spawner caused
49 to 64 refused re-acquisitions per 20,000 cycles with plain close, and zero
with an explicit `unlock()` first. Details: [research 006](research/006-owner-lock-inheritance.md).

Fix, one change at the root ([decision 0012](decisions/0012-explicit-owner-lock-release.md),
A65-A66): a private `OwnerLock` wrapper in `oc-core-db` unlocks on drop, returned
by `own_directory` so `open`, its error paths and `restore` are all covered. No
serial CI, retries or test skips. Two deterministic regression tests copy the
descriptor with `try_clone`; both failed on the unfixed code (`AlreadyOpen`, three
of three runs) and pass now. One strict-clippy failure (`used_underscore_binding`
on the test reading `_owner`) was fixed with a single `#[expect]` after a rename
was tried and rejected because it created a dead-code error.

Results on final code: test binary 0 of 300 default-thread runs failed (was 24 of
100); five `cargo test --workspace --locked` runs passed, 43 tests (20 database,
23 rekordbox); fmt, strict clippy, `cargo check` and `git diff --check` passed.
Only macOS was run. The frontend, browser and Tauri bundle were not rerun; this
change touches only Rust database code. The `documenter` agent is not available
as a subagent in this session, so the docs were reconciled directly.

Next: follow HANDOFF. R3/R7 filesystem identity and availability contracts still
precede library migrations and matching. All changes remain uncommitted; no
commit or push, no private data accessed, nothing left running.

## 2026-10-22 (final repeat found database test instability; stopped)

Superseded by the entry above: the instability is resolved.

The final repeat after preserving COLLECTION attributes failed two existing
database tests (16 passed, two failed): clean/drop reopen returned `AlreadyOpen`,
and invalid lifecycle state did not produce the expected `InvalidState`.
The immediately preceding workspace run passed all 41 tests. This supersedes
the green stopping status below; the intermittent issue is unresolved.

Strict workspace clippy, format and diff checks passed. The final documentation
check found 290 local targets across 34 Markdown files, zero missing. No source
fix or serial-test workaround was attempted. HANDOFF starts with reproduction
steps and a clearly labelled lock/process-concurrency hypothesis.

The owner asked to stop for credit. Resume with this failure before new features.

## 2026-10-22 (owner-requested credit-limit stopping checkpoint)

Stopped new feature work at the owner's request. Standalone XML parsing is now
built: typed tracks/cues/tempo points, original attributes and lossless path
bytes, ordered nested playlists, duplicate references, explicit unresolved IDs
and reported skipped extension roots. The parser does no filesystem or database
access and is not connected to the demo UI.

Final local checks: 41 Rust workspace tests passed (18 database, 23 rekordbox),
strict workspace clippy and formatting passed. Research
[005](research/005-read-only-xml-validation.md) records fixtures, limits, failed
lint correction and commands. Existing shell evidence is 12 unit/nine browser
tests, 34.1 ms browser search p95; those tests were not rerun for XML-only edits.

The documenter wrote decision 0011/A58-A64 concurrently with implementation.
Its session was no longer available for the final update, so the parent
reconciled the handoff, crate README and indexes directly. CLAUDE.md remains the
Claude Code entry point. All changes remain uncommitted; preserve the dirty
worktree. No app/server is running, no private library was accessed, no push.

Next: follow HANDOFF. R3/R7 filesystem identity/state contracts still precede
matching and library migrations. XML UI integration, mirror transactions,
Windows CI/installers and native visual checks remain incomplete.

## 2026-10-22 (standalone XML parser contract; in progress)

Recorded [decision 0011](decisions/0011-read-only-xml-parser.md) for the next
bounded slice and assumptions A58-A64. The parent owns implementation; this
documentation checkpoint did not inspect code under construction. The crate
README separates the validated Location decoder from the planned XML reader.
No parser implementation or test result is claimed.

The contract is a public `BufRead` reader using `quick-xml`, returning a
complete typed document only after valid EOF. It preserves metadata attributes,
XML-unescaped Locations and lossless decoded path bytes, finite nonnegative
`f64` seconds, playlist order and duplicate references. Absent collection
references remain unresolved source IDs, not missing files. Invalid Locations,
duplicate collection IDs, count mismatches, malformed numerics/shape,
unsupported keys, DTDs and unsupported entity mechanisms fail the document.

Initial bounds are 128 MiB input, 1,000,000 collection tracks, 1,000,000 playlist
references and XML depth 128, including unknown subtrees. These are limits, not
measurements. Tests will use synthetic inputs only; research 002 counts inform
capacity without opening real XML. There is no matching, root mapping, mirror
database, transaction or UI connection. R3/R7 remain open.

The parent also supplied final validation for the preceding shell checkpoint:
the final diff check was clean; a Python local-link checker checked 280 targets
across 32 Markdown files with zero missing targets. Exact checker invocations
were not supplied, and these results predate the new parser documentation.
Native `/tmp` stdout/stderr logs were empty and cleaned up. No server or app
was left running. This pass did not rerun those checks.

Next: wait for the parent's ready signal, inspect public items, tests and
manifests, then reconcile documentation against the actual implementation.
Create research 005 only from supplied test or measurement evidence. No private
data, source/config edits, review edits, commits or pushes in this docs pass.

## 2026-10-22 (synthetic shell documentation reconciled)

Reconciled the shell checkpoint against `src/`, manifests and lockfile, unit
and browser tests, Tauri main/config/capabilities and both workflows. The visual
sub-slice of 0e is built locally under [0010](decisions/0010-synthetic-interface-shell.md).
This is not full milestone 0, a windowed SQLite API or real database IPC.
The workspace has two library crates plus the Tauri host; default members
remain the two libraries.

- Replaced the scratch handoff with one current narrative and exact continuation
  commands. Added the host README and
  [research 004](research/004-synthetic-interface-validation.md). Updated the
  root/index, build plan, interface, implementation assumptions and Game Plan
  status without changing prior product decisions or milestone 1-9 scope.
- Documented worker query/sort and replies of at most 200 rows. Abort rejects
  pending promises and stale replies are ignored; it does not interrupt an
  already-running synchronous worker computation. The UI does not use `oc-core-db`.
- Separated current `/`, arrow/Home/End navigation and Enter selection/card
  drill-down from planned Ctrl/Cmd+K and playback shortcuts. Recorded collapsed
  sidebar names, modal focus, explicit demo limits, appearance-only storage,
  and the implemented faint token `#9494a1`.
- Parent-supplied evidence, not rerun by this documentation pass:
  `npx -y -p node@24 -p npm@11 npm ci`, `npm run build`, `npm test` and
  `npm run test:browser` under that same wrapper passed on Node 24.22.1/npm
  11.21.1: 12 unit tests in three files and nine Chromium browser tests.
  The newly added Vitest 3 dependency graph had audit warnings; the deliberate
  update to Vitest 5 now reports zero. `npm ci` warned about unapproved
  esbuild/fsevents install scripts; builds still passed and the lockfile was
  reported reproducible.
- Latest `npm run test:browser` workload: 100,000 deterministic tracks,
  10,000 releases, 240 artists; headless Chromium 156.0.8078.4, 1440x960,
  arm64 Mac. Thirty warm searches had 34.1 ms p95 from input event through
  current-result completion plus two animation frames. Over 120 scroll frames,
  p95 was 17.5 ms, maximum 17.8 ms, none exceeded 33 ms, and at most 21 track
  rows were rendered. Observed cold readiness was 83.2 ms including polling,
  not precise startup-to-paint. Research 004 holds raw values and limitations.
  These supersede the earlier timing run and are not general speed guarantees.
- Parent-reported workspace fmt, strict clippy, tests and check passed:
  25 Rust tests (18 database, seven codec, zero Tauri tests). Metadata confirmed
  the host's workspace membership. The Node-24-wrapped
  `npm run tauri -- build --debug --bundles app` produced a 14.78 MiB arm64
  macOS app with a linker ad-hoc signature and no Developer Team. Its executable
  remained alive for three seconds, then stopped; no native UI readiness was
  established. Screenshots in ignored `test-results/` were viewed by the parent.

Rust CI now builds the frontend first. The frontend workflow has Linux web,
unit and browser jobs plus an on-demand unsigned Windows NSIS build. Neither
remote workflow has run. No private data, source/config changes in this
documentation pass, commits or pushes. No app or server remains running.

Next safe slice: standalone read-only rekordbox XML parsing against synthetic
fixtures. It has not started at this checkpoint. No matching, mirror database,
UI connection or private export access is authorized by that slice. Scheduling,
path/state and mutation gates remain open; see [HANDOFF](HANDOFF.md).

## 2026-10-22 (synthetic shell verified and measured)

Node 24 validation passed: production web build, 12 unit tests and nine Chromium
browser tests. Browser checks cover bounded/deep windows, search races, filters,
release/artist drill-down, keyboard focus, persisted settings, storage failure,
startup-worker failure, minimum width and reduced motion. All 12 combinations
of main/secondary/faint body tokens against the four surfaces meet 4.5:1.
Screenshots of the track and release views were inspected locally.

The named headless Chromium workload at 1440x960 used 100,000 deterministic
tracks, 10,000 releases and 240 artists. Latest run: 30 warm search samples,
34.1 ms p95 from input event through correct result plus two frames; 120 scroll
frames, 17.5 ms p95, 17.8 ms maximum, zero over 33 ms and at most 21 rendered
track rows. Observed startup readiness was 83.2 ms including test polling.
These are browser-demo measurements, not Windows, SQLite, IPC or audio results.

Rust workspace format, strict clippy, tests and compilation passed with the
Tauri package included. `npm run tauri -- build --debug --bundles app` produced
an arm64 macOS app (14.78 MiB, ad-hoc linker signature, no developer signature).
Its exact executable was started and remained alive after three seconds, then
was stopped. Native rendering/responsiveness was not inspected. `npm ci` was
also tested; npm reported zero audit findings and warnings about unapproved
optional/install scripts, but all builds passed.

Frontend/browser CI and an on-demand unsigned Windows NSIS build are configured,
not run. No server or app was left running. No private data, commits or pushes.
Next: reconcile documentation, then continue with a bounded read-only feature.

## 2026-10-22 (shell implemented, browser/native checks next)

The synthetic shell now has a worker-owned 100,000-track catalog, bounded windows,
virtualized tracks/releases/artists, search/sort/format filters, selection,
keyboard navigation, appearance persistence with visible errors, honest empty
import/sync screens and a disabled player preview. A minimal Tauri 2 host has no
filesystem, shell, network plugin or custom command permissions. Icons were
generated from an original SVG, not third-party artwork.

Under Node 24.22.1/npm 11.21.1, the TypeScript/Vite build and 12 unit tests passed.
The initially selected Vitest 3 pulled vulnerable development dependencies;
the manifest was deliberately updated to compatible Vitest 5.0.5 or newer and
the new install reports zero audit findings. No automatic audit fix was used.

Browser tests, actual UI inspection, performance measurements and native checks
are still pending. No real library, audio or private export was accessed.
Next: run Chromium tests, fix behavioral/layout defects, measure the named
synthetic workload and compile the desktop host. No commit or push.

## 2026-10-22 (synthetic interface slice started)

The owner asked to continue autonomously. [Decision 0010](decisions/0010-synthetic-interface-shell.md)
starts the mock-data visual shell before the filesystem-backed query slice:
React/Tauri, a worker-owned synthetic catalog, bounded windows and virtualized
views. R3/R7 remain gates for real library indexing. Node 24/npm and actual
Chromium DOM tests are the chosen frontend toolchain. A55-A57 record the calls.

The environment has Node 26 globally and no frontend yet. Next: add manifests,
install the declared dependencies using Node 24, implement and test the shell.
This is a work-in-progress checkpoint, not a feature or timing claim.

## 2026-10-22 (reduced foundation documentation reconciled)

Reconciled the docs against both crate manifests, public APIs, lifecycle SQL,
recovery code, tests, workspace manifest and Rust workflow. Added crate READMEs
and [research 003](research/003-reduced-foundation-validation.md). Replaced the
stale planning-only handoff and split milestone 0 into bounded slices.

- Current code is lifecycle schema v1 (`app_state` only) and lossless Location
  decoding to bytes. There is no UI, scanner, library search, matching, writer
  worker, reader pool or benchmark.
- Backup wording now follows decision 0009: online backups before existing-schema
  upgrades, none before initial creation, and no-clobber restore to an unused
  local app directory. Orphan journals block creation/restore. Local storage is
  a caller contract, not network-filesystem detection. Scheduling and retention
  remain later work.
- A6 now matches `rust-version = "1.99"`; A11 reflects the reduced first slice.
  A51-A54 and the owner's product choices are preserved. Future table design is
  labelled unimplemented. Fuzzy metadata keys do not define filesystem identity;
  the decoder does not close R3 or the other mutation review gates.
- Execution/parent-session evidence, not a new run by this documentation pass:
  `cargo test --workspace --locked` passed 25 tests (18 database, seven codec)
  and zero doctests on arm64 macOS with Rust 1.99.0.
  `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo build --workspace --locked` passed; `~/.cargo/bin/cargo fmt --all`
  completed. After adding the read-transaction consistency test, the parent
  reran format checking, workspace tests and strict clippy successfully.
  `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked` also
  passed. All fixtures were synthetic. Research 003 records the initial 15/16
  database result, corrected canonical-path assertion and added orphan-journal
  and reader tests. No speed numbers exist.
- Ruby/Psych parsed the workflow and checked its Windows/macOS matrix,
  `contents: read` and exact fmt/clippy/test/build steps. This is static
  validation, not a hosted run. The parent also updated database/snapshot ignore
  patterns and verified six synthetic paths with
  `git check-ignore --no-index --stdin`, without creating files.
- Final parent verification passed: `git diff --check` was clean. A Python
  inline local-link checker found 236 local targets in 29 tracked or nonignored
  untracked Markdown files, with zero missing. It skipped external, mail and
  anchor-only links, stripped fragments and URL-decoded angle-bracket paths.
  The emoji checker found zero emoji in 24 files across `docs/`, root README
  and CLAUDE, and crate READMEs. Current status counts consistently report
  25 total tests, 18 database and seven codec; historical LOG counts are unchanged.
  These are parent-reported checks, not new runs by this documentation pass.

Only documentation changed in this pass. Existing planning edits, both review
reports and the `CLAUDE.md` import of `AGENTS.md` were preserved. No private
library or export was accessed, and nothing was committed or pushed.

Documentation verification is complete. Remote Windows/macOS CI and the rest
of milestone 0 remain unverified or unimplemented. Follow [HANDOFF](HANDOFF.md)
for authorization requirements, unresolved contracts and the next bounded slice.

## 2026-10-22 (Rust foundation validated locally)

Validation on the arm64 development Mac with Rust 1.99.0:

- `cargo test --workspace --locked`: 24 passed, zero failed (17 database tests,
  seven Location codec tests; one database test is a child-process helper).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo build --workspace --locked`: passed.
- `cargo fmt --all`: completed; a final `--check` follows reconciliation.

All tests use synthetic data in temporary directories. Recovery tests cover a
committed WAL snapshot, a restored standalone database, a failed migration,
an abruptly exited child with an uncommitted migration-like transaction,
foreign/future/corrupt input, owner locking, and preserving orphan journals.
The first run found a test expectation that overlooked macOS's canonical
`/private/var` alias; the assertion now compares canonical paths.

No benchmark, UI, installer, Windows run or remote CI result exists yet. Next:
the documenter reconciles the crate READMEs, handoff, assumptions, plan and
research record; final checks verify links and the uncommitted change inventory.

## 2026-10-22 (database lifecycle written, validation pending)

Added `oc-core-db`: cooperative exclusive owner lock, application/schema checks,
WAL, read-only connections, lifecycle marker, transactional migrations, online
backup snapshots and restore into a new directory. Added synthetic tests for
refusal cases, WAL consistency, rollback, failed backups and abrupt child-process
exit. The schema currently contains only `app_state`, not library tables.

Added a Windows/macOS Rust workflow and a root `CLAUDE.md` entry point that shares
`AGENTS.md`; the owner explicitly requested a durable Claude Code handoff.
These changes have not yet been compiled or tested. No timing claims are made.
No real library was accessed; no commit or push was made.

Next: run formatting, tests and strict clippy, fix failures, then have the
documenter reconcile all current-state docs and record verified commands/results.

## 2026-10-22 (foundation resumed after both reviews)

Read both plan reviews and inspected the uncommitted work. The Location decoder
and seven synthetic tests exist; there is no database crate yet. Previous doc
edits and both review reports are preserved. No real library or exports were read.

[Decision 0009](decisions/0009-reduced-foundation-and-recovery.md) narrows the
first implementation slice to the lossless codec, database lifecycle/recovery
and Rust CI. Later product decisions remain gated. The claimed earlier codec
test result will be revalidated in this session; no new measurement exists yet.

Next: implement and fault-test database lifecycle, then reconcile the handoff
and build plan with the documenter agent. No commit or push is planned.

## 2026-10-22 (plan review complete, recommendations pending)

The root `plan_review_gpt6.md` (removed 2026-10-23; open items in [review-findings.md](review-findings.md)) reviews the current plan, including the download-folder changes. It records 13 findings (five High, eight Medium), evidence, recommendations and closure checks, a proposed sequence, and answers to all seven BUILD-PLAN review questions. The report is advisory. No product decisions were made or recommendations accepted.

- **Documentation.** HANDOFF now points to the review and its pending decisions; the docs index links it. Existing planning changes, including A48 to A50, are preserved. No source, configuration or lockfiles changed.
- **Review scope.** Active planning docs, recorded research, workspace manifests, toolchain and lints, plus public primary documentation. The only external repository reads were the local rbxport root and `rbl-db` Cargo manifests, both declaring compatible `rusqlite` 0.40 versions. That is not a build or interoperability result.
- **Reported checks.** The reviewer reports passing Node checks for 13 sequential findings, evidence/recommendation/closure text on each, all seven answers, 40 local links, 22 line references, and no control characters or placeholders. Report editor diagnostics were clean. These checks are structural, not product validation.
- **Reconciliation checks.** Editor diagnostics on the three updated docs are clean; their new relative links resolve to the report. Spot-checks against the current schema, pipeline, milestones, assumptions and workspace manifest found no material factual issue, and sampled local line anchors match. The earlier Node checks and upstream web claims were not independently rerun here; this session has no command runner or web tool.
- **Limits.** No application build or tests, runtime or performance measurement, Windows/rekordbox round trip, or authenticated service calls. No real exports, audio, libraries or NAS were accessed. The report's statistical bound is analytic, not a dataset measurement. No decision, research or assumption record was added, and nothing was committed or pushed.

**Next:** consider the high-priority design gaps before mutation work. A reduced, read-only foundation remains reasonable once path and state contracts are settled. Any accepted recommendations need explicit decisions and corresponding plan updates; the proposed sequence is not an adopted milestone change.

## 2026-10-22 (download folders are a setting)

The owner asked to set the download folder in Settings. The design docs were updated, and the index docs were reconciled. There is still no application code, nothing has been compiled, and no speed number exists.

- **Download folders are a setting.** One or more folders, each chosen with the folder picker, validated, and switched on or off in Settings. Changes take effect at once, with no restart. None is set by default, and the Import screen offers to choose one ([interface](design/interface.md), [import-pipeline](design/import-pipeline.md), A48).
- **Nothing is staged on add.** Adding a folder lists what is already in it and stages nothing. Only releases that arrive afterwards are staged, and "Import existing" brings in the rest (A49).
- **Originals are never changed.** openCrate only reads a download folder. It copies into staging and never moves, renames or deletes the originals in version 1 (A50).
- **Folders are validated.** A folder is refused if it does not exist or cannot be read, if it is the library root, staging or archive (or inside or containing one), or if it is nested with another download folder.
- **Assumptions A48 to A50 are new.** A24 was reworded: where the downloads live no longer matters to the plan.
- **Question 1 is answered.** "Where do Bandcamp and Soulseek downloads land?" is answered by the setting. The remaining owner questions are renumbered 1 to 7 in [ASSUMPTIONS](ASSUMPTIONS.md).
- **Data model.** A `watch_folder` table holds the download folders, and `import_item` gains `watch_folder_id` and `source_rel_path` ([data-model](design/data-model.md)). `watch_folder` stays the table name because the pipeline stage that reads it is called Watch.
- **Plan.** [BUILD-PLAN](BUILD-PLAN.md) milestone 3 starts with the setting and checks that nested or library-overlapping folders are refused and originals are unchanged (by hash). Milestone 9's first-run setup asks for the download folders. The Game Plan's `ingest` row reads "Watch the download folders set in Settings".
- **Consistency sweep.** HANDOFF, the docs index and the question list were updated to A1 to A50 and seven open questions. No other doc used "watch folders" for the setting, and no relative link or A-ID was found dead or missing.

**Next:** milestone 0, the foundation and Windows CI, as described in [HANDOFF.md](HANDOFF.md) section 9. The download folders setting is built in milestone 3.

## 2026-10-22 (owner decisions applied)

The owner answered the open questions. The docs were reconciled with the answers. There is still no application code, nothing has been compiled, and no speed number exists.

- **rekordbox owns the playlists, cues and grids.** openCrate shows playlists read-only, from the rekordbox XML export, and writes no playlist files. [Decision 0008](decisions/0008-rekordbox-readback-xml-first.md) is new and accepted. A34 now records this answer.
- **XML-first read-back, with an experimental `master.db` setting.** The XML export is the default read-back. A setting, off by default and labelled experimental, reads `master.db` through rbxport's `rbl-db`, on a copy only. The option is compiled into every build, so the SQLCipher build applies from milestone 5 (A44, A45). The XML export file is chosen in settings and exporting stays a manual step in rekordbox (A46).
- **Playlist export for Navidrome dropped.** [Decision 0006](decisions/0006-playlists-m3u8-and-rekordbox.md) is withdrawn, and with it A31, A32 and A43. [Decision 0005](decisions/0005-integrations-through-navidrome.md) stays withdrawn. Nothing in openCrate writes playlist files or calls Navidrome or AudioMuse-AI.
- **QNAP QSync.** The NAS copy is kept in step by QSync, two-way, with the desktop owning the files (A33, [decision 0003](decisions/0003-local-library-synced-copy.md)). The owner says the detail is not important. openCrate still never writes to the NAS.
- **One crate fewer.** `oc-playlist` is dropped, so the workspace has nine crates plus the Tauri app. The playlist export tables are out of the data model, and the rekordbox mirror tables (`rb_*`, `playlist`, `playlist_item`) are in ([data-model](design/data-model.md)).
- **Milestones.** Milestone 2 is now the rekordbox XML read-back, with playlists read-only. Milestone 5 is the rekordbox export plus the experimental `master.db` option. The player needs only milestone 2.
- **Private GitHub repository, commit and push.** The repository is private on GitHub (Calvin-Zikakis/openCrate), and CI is GitHub Actions on Windows and macOS (A38). The owner told the agent to commit and push (A47). `AGENTS.md` now says to ask before committing or pushing unless the owner has just said to.
- **Consistency sweep.** HANDOFF, the docs index and the root README were rewritten for the above. Stale playlist-editing wording was removed from the interface design, the data model's playlist ownership and index names were corrected, and the notes on research 002 and on decision 0005 now say which parts are historical. A9 and A42 were corrected for the private repository.

**Next:** milestone 0, the foundation and Windows CI, as described in [HANDOFF.md](HANDOFF.md) section 9. `cargo` fails until milestone 0 creates the workspace members. Planned research notes: a database benchmark note, and later a comparison of the experimental `master.db` reader with the XML read-back.

## 2026-10-22 (plan complete, ready for review)

The plan is complete enough for a reviewer. There is still no application code, nothing has been compiled, and no speed number exists.

- **Owner data inspected.** The owner's rekordbox XML export (9,008 collection entries) and a copy of the playlist folder (55 `.m3u8` files) were read in place on the dev Mac. Only aggregate facts are recorded, in [research note 002](research/002-owner-data-shapes.md): where files live by drive, how rekordbox spells `Location`, the playlist line format, and the library layout. The raw files stay outside the repository.
- **Windows first.** The music, rekordbox and the library (drive G:) are on the owner's Windows desktop. The Mac is for development and holds no music. Windows is the primary target, with CI on Windows and macOS from milestone 0 ([decision 0007](decisions/0007-windows-first-develop-on-mac.md)).
- **Playlists rewritten.** [Decision 0006](decisions/0006-playlists-m3u8-and-rekordbox.md) now describes playlists as app objects written as `.m3u8` export targets in a folder the owner chooses (relative or absolute paths), importable into rekordbox by an XML route and an absolute-path m3u8 route. There is a new crate, `oc-playlist`, so the workspace has ten crates plus the Tauri app. Who owns the playlists, openCrate or rekordbox, is an open question for the owner (A34).
- **Navidrome and AudioMuse-AI withdrawn.** The owner dropped the integration, so [decision 0005](decisions/0005-integrations-through-navidrome.md) is withdrawn and A28 to A30 are withdrawn with it. The only Navidrome-related feature left is the playlist files: the owner's Navidrome scans a `_Playlists` folder in the library root, and nothing in openCrate calls it.
- **New design docs and build plan.** [BUILD-PLAN](BUILD-PLAN.md) (milestones 0 to 9, acceptance checks, test strategy, risks, out-of-scope list and the questions for the review), and the design docs [interface](design/interface.md), [data-model](design/data-model.md) and [import-pipeline](design/import-pipeline.md). [docs/README.md](README.md) indexes them, and a root `README.md` was added.
- **New assumptions A31 to A43.** Playlist export defaults and format (A31, A32), the unknown sync tool (A33), playlist ownership (A34), `.aif` output (A35), the sanitizer rule (A36), Windows first (A37), the CI host (A38), the `Location` spelling (A39), the existing library left alone (A40), byte-safe importers (A41), real exports kept out of the repository (A42), and the unverified rekordbox m3u8 import (A43).
- **Consistency sweep.** Stale wording was fixed in decisions 0005 and 0006 and in assumption A10. HANDOFF was rewritten for the Windows setup. Decisions 0003 and 0007 stay proposed: the facts in them are the owner's, the design is not yet confirmed. The sweep also found that `source_quality` and `fingerprint` pointed at `file` rows that do not exist until filing, which is fixed by recording staged files as `file` rows ([data-model](design/data-model.md)).

**Next:** the owner and reviewer read the plan, starting from [docs/README.md](README.md). Then begin milestone 0 as described in [HANDOFF.md](HANDOFF.md) section 9. Planned research notes: 003 (the rekordbox playlist import check) and a database benchmark note.

## 2026-10-22 (paused for review)

Work paused at the owner's request so the plan can be reviewed. No application code has been written.

**Planning and research (done)**

- Updated the Game Plan with six new decisions: interface, speed, database, library location, tag lookup and integrations. Added sections on the database, interface and speed budgets, library storage and integrations, and looking up tags without hitting limits. Changed the identification rules to score by weighted distance and to require exact numbers.
- [Research note 001](research/001-tag-lookup-options.md): official limits for MusicBrainz, AcoustID and Discogs, and how Lexicon, MusicBee, beets and MiniMediaScanner get their data. None of them is free of limits; each moves the cost somewhere else.
- Decisions 0001 to 0005 recorded as proposed (0004 as accepted, since it restates the owner's requirements).
- Checked AudioMuse-AI and Navidrome. AudioMuse-AI's Navidrome plugin exposes similar tracks and song paths through Navidrome's OpenSubsonic `sonicSimilarity` extension, so one Navidrome client covers import, rescans, playlists and similarity ([decision 0005](decisions/0005-integrations-through-navidrome.md)).
- Caught an error before it reached the plan: a deep `OFFSET` is not slow at 100,000 rows, and a scrollbar needs position-based reads, so the plan says windowed reads and the benchmark will measure deep offsets instead of claiming "keyset pagination".

**Repository setup (done)**

- Added `AGENTS.md`, the `documenter` agent, `LICENSE` (the GPL-3.0 text from gnu.org), the root `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml` and `.gitignore`.
- Environment checked: cargo 1.99.1 and a 1.99.0 toolchain with clippy and rustfmt, Node 26 with npm, ffmpeg 9, and network access to crates.io, npm and gnu.org.

**Calls made without the owner** are listed in [ASSUMPTIONS.md](ASSUMPTIONS.md), A1 to A30, with ten questions at the end.

**Next:** the owner and reviewer read the plan. Then build the foundation described in [HANDOFF.md](HANDOFF.md) section 6. Nothing has been compiled, so no speed numbers exist yet.
