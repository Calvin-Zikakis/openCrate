# Plan review (Opus)

Date: 2026-10-22. Written by Claude (Opus 5.5) at the owner's request: check that the plan makes sense, and record the findings and issues.

## Scope

- **Reviewed:** every document in the repository at commit `4adfd1b`, plus the uncommitted edits in the working tree on the evening of 2026-10-22 (the download folders setting). While this review was being written, another process edited HANDOFF, docs/README and LOG. The plan files did not change: the Game Plan, BUILD-PLAN, ASSUMPTIONS, the design docs, the decisions and the research notes.
- **Also checked:** the rbxport code the plan means to reuse (`~/repositories/rbxport-beat-analysis`), and a few outside facts on the web. Sources are at the end.
- **Not opened:** the owner's data. No rekordbox export, playlist folder or library file was read, and no private name appears here.
- **Independence:** a second review, `plan_review_gpt6.md`, appeared while this one was being written. It was not read.

Findings are grouped by when they bite. Each one says what the plan says, what is wrong, and what to change. Numbers in parentheses are milestones.

## Verdict

The core of the plan is sound (see "What holds up"). It is not ready for milestone 0 as written, for two reasons: some decisions contradict each other, and some features the plan depends on were never designed. Nothing here needs a rewrite. Most items need one decision from the owner and edits to two or three documents.

The issues that matter most, in order:

| Priority | Finding | Bites in |
| --- | --- | --- |
| 1 | Finding 1: "database mode" is required for upgrades but never designed, and it contradicts the copy-only rule for `master.db` | 8 |
| 2 | Finding 2: dedupe and upgrades conflict with "the existing library is never changed" | 8 |
| 3 | Finding 3: the rekordbox round trip is manual at both ends, and "synced" is never defined | 5 |
| 4 | Finding 4: "other DJs from day one" is in no milestone: no configurable rules, no signing or notarization, no macOS data | 0, 9 |
| 5 | Finding 7: "two independent signals" is undefined, and milestone 3's local signals are not independent | 3 |
| 6 | Finding 8: the hand-checked set that gates milestones 3 and 7 has no owner, format, home or time budget | 3, 7 |
| 7 | Finding 9: Bandcamp album downloads arrive as ZIP files, and the pipeline has no archive step | 3 |
| 8 | Finding 11: the ffmpeg sidecar is probably unnecessary, and the lossy-to-lossy and hi-res rules are missing | 4 |
| 9 | Finding 12: MP3 cue offsets between decoders can put rekordbox's cues tens of milliseconds off | 6, 8 |
| 10 | Finding 5: case and Unicode form are ignored in path matching and uniqueness, and `Location` must not go through a URL parser | 1, 2 |

## What holds up

Keep these. They are the strongest parts of the plan.

- **SQLite, local, WAL, one writer.** The reasoning is right, including that WAL does not work over a network file system, so the database stays out of the synced folder. The size arithmetic checks out: a six-minute 16-bit 44.1 kHz stereo AIFF is about 63.5 MB and a 320 kbps MP3 about 14.4 MB, so 500 GB holds about 7,900 or 34,700 tracks, as the Game Plan says.
- **XML-first read-back.** It keeps SQLCipher out of the build and keeps rekordbox's database out of reach until the experimental option is wanted.
- **Index in place; nothing moves in the existing library.** With a two-way sync this is the only safe default. Staging and archive outside the synced folder, copy-verify-rename across volumes, and archive instead of delete are all right.
- **Lookups per release, cached including misses, throttled per provider.** The published limits match the sources (MusicBrainz about 1 per second, AcoustID 3 per second, Discogs 60 per minute with a token). "An hour or two" for 3,000 releases holds at one or two requests per release.
- **Speed as a test.** Budgets checked by a benchmark on a 100,000-track synthetic library, about 11 times the owner's, and "measure before claiming".
- **Windows first,** with Windows-style fixtures on macOS, and checks on real data run by the owner with aggregate output only.
- **Research 002.** Measuring how rekordbox spells `Location` before writing a writer is the right order of work. This review relies on it twice.

## Must decide before milestone 0

### 1. "Database mode" is relied on but never designed

What the plan says:
- Game Plan, decisions table: "File moves after sync: frozen by default; opt-in database mode can fix paths" and "Upgrades: ... Synced tracks need database mode; otherwise they wait in review". The `rekordbox` crate row lists "opt-in path fixes".
- BUILD-PLAN milestone 8 and import-pipeline rule 4 repeat that a synced track "needs the opt-in database mode".

What is wrong:
- Database mode means writing new paths into rekordbox's live `master.db`. Decision 0008 and A45 say openCrate only ever opens a copy of `master.db`, read-only, and AGENTS.md rule 4 says never touch rekordbox. The plan cannot have both.
- No milestone builds it and no decision record describes it. Milestone 5 builds only a read-only reader of a copy. As written, every upgrade of a track rekordbox knows waits in review forever, and "keep cues" never happens.
- XML is not a fallback. Third-party reports say rekordbox's "Import To Collection" skips tracks that are already in the collection, so a second import does not update their cues or details (Rekord Cloud; similar reports from Mixed In Key and Engine DJ users; not verified here). A generated XML that points at a new file for an existing track is more likely to add a second entry, without the first entry's places in playlists.

What to change: decide, and record it as a decision.
- Option A, recommended for v1: no database mode. Upgrading a track rekordbox already holds is a guided manual step. openCrate moves the old file to the archive and files the better copy. rekordbox then lists the entry as missing, and the owner points it at the new file with rekordbox's Relocate, which keeps the same entry, so its cues and playlist places stay (but see finding 12). The next read-back confirms the new path.
- Option B: a write mode for the live `master.db`, in its own decision record. rekordbox must be closed, a backup is taken first, every write is shown before it is made, and the owner turns it on. It would be the only place openCrate writes to rekordbox's database, so it needs its own risk entry and tests on a copy.
- Either way, update the Game Plan rows ("File moves after sync", "Upgrades", the `rekordbox` crate row), BUILD-PLAN milestone 8, import-pipeline rule 4, and A45.

### 2. Dedupe and upgrades conflict with "the existing library is never changed"

What the plan says: A40, import-pipeline rule 7 and the Game Plan's "Existing library: index in place; nothing moves". Milestone 8: "Duplicates found, better copies replace old ones, old ones are archived". The Game Plan's Library storage table: "Fingerprinting is for new imports".

What is wrong:
- Nearly every track the owner has today (about 9,000 in rekordbox) is in the existing library. The common upgrade, a lossless purchase of a track the owner holds as an MP3, replaces an existing-library file. That is changing it.
- Without fingerprints for the existing library, the "fingerprint index" cannot see that a new download is a track the owner already has. Only tag and title matching can.
- Replacing or retagging files in the existing library also makes QSync upload them again (an AIFF is about 60 MB), and rekordbox keeps showing the old tag values until "Reload Tag".

What to change: state the scope of milestone 8.
- Recommended: dedupe compares new downloads against the whole library by tags and version-aware titles, and reports matches in the review queue. Automatic replacement applies only to files openCrate filed. Upgrading an existing-library file is an explicit action the owner takes for one file at a time, archive first, and A40 names it as its one exception.
- If fingerprint matching against the existing library is wanted, add an opt-in background pass and say so in the Library storage table. It decodes the first two minutes of every file, about 21 MB each for AIFF.

### 3. The rekordbox round trip is manual at both ends, and "synced" is never defined

What the plan says: openCrate writes an XML that rekordbox imports (5), reads back the XML export (2), freezes paths "after sync", and keeps `sync_state.synced_ms`. ASSUMPTIONS question 1 asks whether new tracks should go to rekordbox "automatically, or only when you ask".

What is wrong:
- rekordbox cannot be driven from outside; A46 already says so for the export. Importing is manual too. The owner opens rekordbox's rekordbox xml view and runs "Import To Collection". Then rekordbox analyzes, the owner sets cues and exports the collection again, and only then does openCrate see BPM, key and cues for the new tracks. That is four manual steps per batch, and the plan never draws the loop.
- So question 1 offers a choice that does not exist. The only automatic part is writing or refreshing the XML file.
- openCrate cannot know when, or whether, a track was imported. `synced_ms` would record when the XML was written. "Frozen after sync" then either freezes files rekordbox never took, or misses files it did take.

What to change:
- Define two states: offered (written into an XML by openCrate) and confirmed (seen in a later read-back at the same decoded path). Freeze on offered, which is the safe side, and record confirmation with a state or `confirmed_ms` column in `sync_state`.
- Write to one stable file that the owner sets once as rekordbox's rekordbox xml "Imported Library", not a new file each time. Consider a dated playlist node inside that file so new tracks are easy to find in rekordbox. Decision 0008 rules out playlist files, not a node in the import file, so this is the owner's call. A dated name matters: rbxport's notes on rekordbox 7.2.19 say an imported playlist with an existing name prompts to replace the old one.
- Show the loop on the Sync screen: what was offered, what was confirmed, and a reminder to export after analyzing. Rephrase question 1 as "keep the rekordbox import file updated after every filing, or only when I press Send?".

### 4. "Other DJs from day one" is in no milestone

What the plan says: decisions table, "Audience: Other DJs from day one. Installers, onboarding, configurable rules and user docs are v1 work."

What is wrong:
- **Configurable rules are not scoped.** The path template is a code constant (A12: "Change `DEFAULT_TEMPLATE` in `oc-organize`"). The remixer-folder rule, AIFF for all lossless and MP3 for all other lossy are fixed. These are the owner's own preferences, and they are the first thing another DJ would change.
- **No signing, notarization or updates.** Milestone 9 says "installers". An unsigned Windows installer shows a SmartScreen warning, and macOS blocks an app that is not notarized unless the user overrides it in System Settings. Both fixes cost a yearly fee. There is no update path, and Tauri's updater needs its own signing key.
- **macOS has no real data behind it.** Research 002 covers Windows only. How rekordbox on macOS spells `Location`, including the Unicode form of names, is unmeasured, and the owner never runs the app on the Mac with real music.

What to change: ask the owner to choose.
- Either v1 is for the owner and other DJs come in v1.1, which takes configurable rules, signing and macOS checks off the critical path,
- or add configurable rules (template, remixer-folder rule, conversion targets) to milestones 3 and 4, add signing, notarization and updates to milestone 9 with their cost, and get one macOS rekordbox export from a volunteer for a research note (aggregate facts only).

## Before milestones 1 to 3

### 5. Path identity: case, Unicode form and `Location` decoding (1, 2)

What the plan says: `file.rel_path` is stored "exactly as the name is on disk", unique on `(root_id, rel_path)`. rekordbox entries are matched to files "by decoded path under a known library root". `rb_track.location_key` exists, but its rule is not written down.

What is wrong:
- NTFS and the default APFS ignore case. A unique `(root_id, rel_path)` in SQLite does not, and SQLite's `NOCASE` folds ASCII only. If the template writes `artist/EP/...` while an `Artist/EP/` folder exists, the file lands in `Artist/EP/` but the database records `artist/EP/...`, and the next scan adds a second row. A rekordbox entry whose stored case differs from the disk still plays in rekordbox, but will not match.
- The same goes for Unicode form: a name can be NFD on disk on macOS, and rekordbox on macOS may write either form.
- rekordbox leaves `#` (33 locations in the owner's export) and `+` (33) unencoded (research 002). A URL parser treats `#` as the start of a fragment and cuts the path there, and a form decoder turns `+` into a space. rbxport's decoder does plain percent-decoding, which is right, but then converts the bytes with a lossy UTF-8 step, which A41 forbids.

What to change: add `rel_path_key` (NFC, Unicode case folding, forward slashes) and make it the unique key on case-insensitive volumes. After every move, read the real name back from disk. Decode `Location` by raw percent-decoding of the bytes after the `file://localhost/` prefix, keep raw bytes that are not valid UTF-8, and build `location_key` with the same rule as `rel_path_key`. Put these cases in the milestone 2 fixtures.

### 6. Test the XML writer against rekordbox early, and safely (2)

- The top item in the risk register (a `Location` spelling that differs from rekordbox's makes duplicate entries) is only tested in milestone 5. The test does not need milestones 3 and 4: a few generated audio files with awkward names (space, `&`, `'`, `[`, `#`, `+`, `,`, accented letters, a long name) in a test folder on the desktop, and a generated XML for the owner to import. It belongs in milestone 2, beside the decoder it mirrors.
- rbxport's XML writer does not reproduce rekordbox's spelling. Its `location()` in `crates/rbl-index/src/xml_export.rs` writes uppercase hex and encodes the drive colon (`C%3A`) and `,` `#` `+` `$`. The owner's export uses lowercase hex and leaves the colon and those four characters raw. So that writer cannot be reused for `Location`. If its exports import into rekordbox without duplicates, rekordbox may decode before matching. The early test should try both spellings to learn how much the exact spelling matters, and import twice with a changed cue to check the skip behavior in finding 1.
- Importing test tracks changes the owner's real rekordbox collection. Write the procedure down: back up the library with rekordbox's own backup first, use only generated files, and remove the test entries afterwards. Without that, the acceptance check in milestone 5 conflicts with AGENTS.md rule 4.

### 7. Identification: "two independent signals" is undefined (3)

What the plan says: Game Plan, "Auto-file requires at least two independent signals to agree on artist, title and version". A14: "agreed by at least two different signal sources, with no source disagreeing". Milestone 3 uses local signals only: tags, folder and file names, and MusicBrainz IDs already in the tags.

What is wrong:
- Bandcamp names its files from the same data as the tags; research 002 shows the raw form `Artist - Release - NN Artist - Title.ext`. Soulseek names are often written by a tagger from the tags. When tag and file name agree, that shows the download is consistent, not that it is right.
- So milestone 3 lands in one of two places. If tag plus file name counts as two sources, most Bandcamp releases file themselves on what is really one source. If it does not, almost nothing files itself until milestone 7, and "no wrong auto-file" is met with near-zero coverage.
- Two rules contradict each other. The Game Plan says that if 9 of 10 tracks agree on a release, the tenth is probably a tagging error, so fix it and go on. A14 says no source may disagree, so the release goes to review. Pick one.
- "Numbers must match exactly ... 2020 is not 2021" is right for numbers inside names ("Vol. 2", "Part 3"). Applied to the year field, it rejects good matches: Bandcamp, MusicBrainz and Discogs often give different years for the same release (pre-orders, reissues, digital against vinyl). In the beets model the plan copies, year is already a low-weight term (1.0, against 3.0 for artist and title; research 001).

What to change: define signal classes in the Game Plan and A14. Tags and names from the same download are one class. An MBID in the tags, the Bandcamp page, a catalog lookup and a fingerprint are each their own class. State what milestone 3 may auto-file with one class; perhaps nothing, as a pure dry run. Choose the tenth-track rule. Limit the exact-number rule to numbers inside names and titles.

### 8. The hand-checked set has no owner, format or home (3, 7)

What the plan says: milestone 3 acceptance, "On a hand-checked set of about 200 downloads, no wrong auto-file, with coverage reported". The Game Plan: "tune it against a hand-checked set of a few hundred of your own downloads".

What is wrong:
- The set is private. The labels hold track names, so they cannot go in the repository (AGENTS.md rule 5), and the downloads are on the Windows desktop, where the agent cannot see them. Labeling and scoring have to run on the owner's machine with aggregate output. Nothing in the plan provides for that.
- "200 downloads" could mean 200 files or 200 releases. If releases, that is 1,000 to 2,000 tracks to check by hand, which is weeks of the owner's 10 to 15 hours.
- Zero wrong out of 200 still allows a true error rate of up to about 1.5% (95% confidence, by the rule of three). Using the same set to tune the threshold and to judge the result makes it look better than it is.

What to change:
- Make the review queue the labeling tool. In dry-run mode, the owner's accept and edit decisions become the labels, stored in the app data directory and never in the repository.
- Add an `evaluate` command that prints only counts and rates: precision per field, coverage, and the confidence bound.
- Define "wrong" per field. Artist, title, version, remixer and release decide the path and must be zero. Year and label can be softer.
- Keep a held-out part that is never used for tuning. Start collecting during milestones 1 and 2, so the set exists when milestone 3 needs it.

### 9. Bandcamp album downloads are ZIP files (3)

What the plan says: "New music sources: Bandcamp and Soulseek. Whole-release folders." The release unit is a folder: `import_item.source_rel_path` is "where the release was found".

What is wrong: Bandcamp delivers an album purchase as a ZIP archive and a single track as a loose file. The pipeline has no archive step and no rule for loose files at the top of a download folder.

What to change: ask the owner how purchases arrive today. Then either unpack archives into staging, never touching the original (A50), refusing entries whose paths escape the target folder and archives that expand to an unreasonable size; or ignore archives with a notice. Group loose files by album and album artist tags, else treat each as its own release. Besides waiting for a stable size, ignore partial downloads by name (`.part`, `.crdownload`, `.tmp`, and the clients' incomplete-download folders). Add fixtures for all of these to milestone 3.

### 10. Windows file system realities (3, 4)

- **The 260-character rule tests the wrong thing.** A15 and question 7 make the limit depend on whether long paths are available. openCrate can always write long paths itself (with `\\?\` paths). What matters is whether rekordbox, QSync and Explorer can open them, and an app has to opt in to long paths even when Windows allows them. That is unknown for rekordbox. Make the limit hard, count it in UTF-16 code units over the full path including the root (a character outside the Basic Multilingual Plane, such as an emoji in a Bandcamp title, counts twice), and keep a margin. Rephrase question 7 as "Does rekordbox open a file whose full path is over 260 characters?" and answer it with the test in finding 6.
- **Transient locks.** Windows Defender scanning a new file, QSync reading one, or rekordbox playing one makes a rename or delete fail for a moment with "access denied" or a sharing violation. Add retry with backoff to the file and archive steps and to the failure-handling table, and test it on the Windows runner.
- **Durability.** An atomic rename without a flush can leave an empty or cut-off file after a power loss. Flush the file before the rename and the folder after it. rbxport's `rbl-core` has a durable-publication module to learn from.

## Milestones 4 to 8

### 11. Conversion: the ffmpeg sidecar is probably unnecessary, and two rules are missing (4)

What the plan says: `convert` uses an ffmpeg sidecar with LAME. "Lossless to AIFF; all other lossy to MP3."

What is wrong:
- rbxport's `rbl-audio` already converts without ffmpeg. Its manifest (`crates/rbl-audio/Cargo.toml`) decodes with symphonia (MP3, AAC, ALAC, MP4, FLAC, WAV, AIFF, PCM, ADPCM), encodes MP3 with `mp3lame-encoder` (LAME), and resamples with `rubato`. The player already plans to use symphonia, and writing AIFF is a small container writer.
- Dropping the sidecar removes the "ffmpeg license and LAME" risk from the register, a separate binary per platform, signing and notarizing an extra executable on macOS, tens of megabytes of installer, and a process boundary whose errors must be parsed. The cost: symphonia 0.5 decodes neither Opus nor WMA. Such files can go to review as unsupported, or ffmpeg can stay as an optional tool the user points to.
- "All other lossy to MP3" re-encodes AAC, which rekordbox and CDJs play, and loses quality for nothing. It must also never re-encode an MP3. Transcode only what rekordbox cannot play (Ogg Vorbis, Opus, WMA), and flag the result, because its quality is capped by the source.
- Hi-res sources are not covered. Some older Pioneer players do not play files above 48 kHz. Decide whether to keep the sample rate and bit depth, or to resample anything above 48 kHz and keep the bit depth.
- Upgrades must compare `source_quality`, not the output file. A 320 kbps MP3 made from an Opus file is not better than a 256 kbps AAC.

What to change: write a decision record before milestone 4. Recommended: no sidecar in v1 (symphonia, an AIFF writer, `mp3lame-encoder`, `rubato`), MP3 and AAC kept as they are, transcoding only for formats rekordbox cannot play, and a sample-rate setting with a default the owner picks.

### 12. MP3 cue offsets between decoders (6, 8)

What the plan says: the player shows rekordbox's cues on its own waveform (6). Upgrades "keep cues" (8). Milestone 6 acceptance: "Cue markers match the read-back for a sample of tracks (owner check)."

What is wrong: rekordbox's cue times are positions in rekordbox's own decoding of the file. For MP3 files, DJ apps disagree on whether the first frame (the LAME or Xing header) is audio. A DJ-tools knowledge base reports a 26 ms shift for about 6% of MP3 files moved from Traktor to rekordbox (from the DJ Data Converter research), and Rekord Cloud reports a 50 ms offset for rekordbox cues when MP3s are replaced by WAVs. If symphonia's handling differs from rekordbox's, markers and seeks are off by that much, and a format-changing upgrade carries its cues off by the same amount. A visual check will not catch 26 ms.

What to change: add it to the risk register. Build synthetic MP3 fixtures with and without LAME and Xing headers, with valid and invalid CRCs, each with a click at a known sample. Choose symphonia's gapless setting to match rekordbox, and confirm once in rekordbox with the owner by setting a cue on the click. Without database mode (finding 1), openCrate cannot shift cues inside rekordbox, so an upgrade that changes format should warn that the cues will be off by the measured amount.

### 13. The experimental reader must copy the WAL too (5)

rekordbox keeps `master.db` in WAL mode: rbxport's `rbl-db` says so in `src/new_library.rs` and handles a non-empty `-wal` in `src/locate.rs`. A copy of `master.db` alone misses every change still in `master.db-wal`, and copying while rekordbox writes can tear. Copy `master.db` and `master.db-wal` together, run `PRAGMA quick_check` on the copy, warn when rekordbox is running, and show how old the copy is.

### 14. SQLCipher in every build: the CI cost (5)

A44 compiles the experimental option into every build, so from milestone 5 every build compiles OpenSSL and SQLCipher from source.
- rbxport's Windows CI (`.github/workflows/validate-windows.yml`) needs Strawberry Perl for the vendored OpenSSL (`OPENSSL_SRC_PERL`) and a short `CARGO_TARGET_DIR` (`C:/rbl-target`) to stay under path limits. It runs a compile check and one test on a self-hosted runner; the full test suite runs on Linux. So rbxport does not prove a full Windows test run and installer with SQLCipher on a GitHub-hosted runner.
- With resolver 2, `cargo test --workspace` builds `oc-core-db` against SQLCipher once `oc-rekordbox` depends on `rbl-db`, while `cargo test -p oc-core-db` builds it against plain SQLite. Two SQLite builds are then under test. Re-run the database benchmark on the build that ships.
- Use rustls for `reqwest` in milestone 7. rbxport's `src-tauri/Cargo.toml` records a collision on Linux between the OpenSSL that native-tls pulls in and SQLCipher's vendored OpenSSL.
- Keep A44's fallback (a Cargo feature) ready, and decide after one measured CI run with `rbl-db` linked.

## Architecture and data model

### 15. No component runs the pipeline

The architecture lists a crate per stage but no orchestrator: the code that runs the import state machine, resumes after a crash, bounds concurrency, cancels, and emits progress. If it lives in `src-tauri`, the failure-injection tests in milestone 4 need the Tauri runtime. Name it (an `oc-pipeline` crate, or a module in `oc-ingest`) and keep it testable without Tauri.

### 16. Data model gaps

- **How existing files get `track` and `release` rows.** `file.track_id` "is empty until the file is identified", and only new imports are identified. The Tracks, Releases and Artists views (1) and dedupe (8) need rows for the existing files. Say that indexing builds them from tags, without identification.
- **Offline is stored twice:** `library_root.online` and `file.state = offline`. Unplugging a drive would rewrite thousands of rows. Derive offline from the root.
- **Conversion and `file` rows.** It is not said whether the AIFF reuses the staged FLAC's row (keeping `source_quality` attached) or gets a new one.
- **Copied BPM and key.** `track.bpm` and `track.musical_key` are copied from the mirror, which is replaced on every read-back, so the copies go stale when an entry leaves rekordbox. Join at read time, or rebuild the copies with the mirror.
- **What only the database knows.** The Game Plan says only the review queue, audit log, sync state and aliases exist solely in the database. So does `source_quality` once the original is converted and the staging copy removed, and so do the chosen release IDs unless they are written into tags. Write source quality and the IDs into the files (a comment or custom frames), and add them to the list the backups protect.
- **Bulky, rebuildable data in the main file.** Snapshots copy the whole database "before every migration and on a schedule". Provider responses (tens of KB per release) and fingerprints (about 4 KB per track for two minutes of audio, so about 400 MB at 100,000 tracks) do not need backing up. Put them in a separate cache database.
- **Provider tokens.** "Credentials go in the OS keychain" lived only in decision 0005, which is withdrawn. Restore the rule: the OS keychain, never the database or its snapshots.

### 17. Path template and version grammar

- `{folder_artist}/{release}/...` has no disambiguator. Two different releases with the same title by the same artist ("Remixes", self-titled EPs) merge into one folder, which breaks "every release gets its own folder"; "never overwrite" only catches identical file names. Add a rule, for example a catalog number or year suffix when the folder already holds another release, and record which release owns a folder.
- The version and remixer grammar decides folders, but it is not written down. Which of Remix, Edit, Rework, VIP, Dub, Bootleg, Flip and Re-Edit put a track in the credited artist's folder? What about two remixers ("X & Y Remix") or "feat."? Write it as a table of synthetic examples and use it as the test corpus for `oc-identify`.

## Answers to the questions for the review

These answer "Questions for the review" in BUILD-PLAN.

**1. Is the milestone order right?**
- **Slim milestone 0.** As written it holds the whole database layer and benchmark, the organize crate, the name parser and claim scorer, the stability tracker, the full interface shell, and CI with installers. That is more than 2 to 5 weeks at 10 to 15 hours, unless agents write nearly all the code, and then the owner's review and testing time is the limit, which no milestone budgets. The claim scorer also needs the labeled set (finding 8). Keep the workspace, CI, the database core and benchmark, the shell on mock data and the Windows installer. Move `oc-organize` and `oc-identify` to milestone 3, where the plan view first needs them.
- **Split milestone 2.** Do the `Location` decoder, the match report and the rekordbox import test (finding 6) early, because they de-risk milestone 5. Cues, grids and the playlist mirror can move next to the player.
- **Keep the export before the player.** The export closes the core loop: download, library, rekordbox. The player is the most optional v1 feature, since the owner plays in rekordbox.
- **Move the experimental `master.db` option** to the end of v1 (decision 0008 keeps it in v1). It is the only reason SQLCipher enters the build in milestone 5 (finding 14).

**2. Dry run first, or a detour?** Dry run first. It is the evaluation harness and the labeling tool (finding 8). Build the plan view as part of the review queue, not as a second screen.

**3. Is "no wrong auto-file" realistic, and is 200 enough?** Realistic for the fields that decide the path, once "independent" and "wrong" are defined (findings 7 and 8). Zero errors in 200 releases bounds the error rate at about 1.5%. Keep growing the set from review decisions, and keep a held-out part.

**4. Are the free CI minutes enough?** GitHub Free includes 2,000 minutes a month for private repositories. Windows and macOS minutes cost more: GitHub's billing page, read on 2026-10-22, gives $0.010 and $0.062 a minute against $0.006 for Linux. The page does not say whether the included minutes are consumed at a multiplier, so measure in the first week. The installer on every run is the expensive part, because it is a release build with fat LTO and one codegen unit.
- Run format, clippy and tests in the dev profile on every push (Windows). An optional Linux job for the pure crates is cheaper still.
- Run macOS on pull requests or nightly, and build release installers on demand or nightly. Cache the target folder.
- A Windows 11 on Arm virtual machine on the Mac would shorten the loop for path, lock and WebView2 problems. CI stays the gate.
- Public repositories on standard runners are free. Going public is the owner's call alone; agents must not change it (AGENTS.md rule 6).

**5. Is windowed reading by position enough?** For unfiltered lists with an index per sort order, yes. It does not cover filtered and sorted views: a playlist filter sorted by BPM, search results, format filters. Each window then re-runs the filtered sort. Recommended: when a view changes, compute its ordered list of track IDs once (100,000 IDs is 800 KB), slice windows from it, and fetch rows by primary key. Deep offsets stop mattering and totals stay exact. rbxport's `rbl-index` already serves windows of rows from an in-memory columnar snapshot that owns sorting, filtering and search; it is a working reference. Benchmark both approaches in milestone 0.

**6. Is the scoring policy strict enough, without starving auto-file?** It is strict about agreement and silent about independence (finding 7), so with local signals it is either trivially met or never met. Define the classes first; then the hand-checked set can answer this.

**7. Which unverified items would change the plan if wrong?**

| Item | If wrong | Cheapest check | When |
| --- | --- | --- | --- |
| rekordbox skips tracks already in the collection on XML import | Upgrades and any re-send through XML do not work (finding 1) | Import a generated XML twice, with a changed cue | Milestone 2 test (finding 6) |
| rekordbox matches `Location` exactly rather than after decoding | Duplicate entries (risk register, first row) | Import both spellings in the same test | Milestone 2 test |
| rekordbox opens paths over 260 characters | The limit could be relaxed; until then it stays hard | One long-path test file | Milestone 2 test |
| How rekordbox decodes the start of an MP3 | Cue markers and seeks off by 26 to 50 ms (finding 12) | Synthetic MP3s with a click | Before milestone 6 |
| Bandcamp purchases arrive as ZIP files | Milestone 3 needs an archive step (finding 9) | Ask the owner | Before milestone 3 |
| `rbl-db`'s interface stays stable | Rework on upstream changes. rbxport is a fork of chrisle/rbxport, a third party's active project | Pin a revision and read the upstream log before bumping | Milestone 5 |
| SQLCipher builds and tests on a hosted Windows runner in acceptable time | A44's fallback, a Cargo feature (finding 14) | One CI run with `rbl-db` linked | A spike in milestone 0, or early in 5 |
| The labeled set fits in the owner's time | Milestones 3 and 7 lose their gate (finding 8) | Time 10 releases by hand | Milestone 1 |

## Proposed milestone order

| # | Name | Change from BUILD-PLAN |
| --- | --- | --- |
| 0 | Foundation and Windows CI | Without `oc-organize` and `oc-identify`. Benchmark both windowing approaches |
| 1 | Index in place and library views | Adds the case- and Unicode-insensitive path keys (finding 5). Start the labeled set |
| 2 | rekordbox `Location` codec and import test | Decoder, match report, and the owner's rekordbox import test with generated files after a backup (finding 6) |
| 3 | Import pipeline, dry run | Adds `oc-organize` and `oc-identify`, archives and loose files (finding 9), signal classes (finding 7), and the `evaluate` command (finding 8) |
| 4 | Convert, tag and file | Conversion decision first (finding 11). Windows retries and flushes (finding 10) |
| 5 | rekordbox export | Offered and confirmed states (finding 3). No `master.db` yet |
| 6 | rekordbox mirror and player | Cues, grids and the playlist mirror from the old milestone 2, then the player with the MP3 offset settled (finding 12) |
| 7 | Online lookups | rustls (finding 14). Bandcamp only through URLs found in the files (see "Smaller issues") |
| 8 | Dedupe and upgrades | Rescoped per findings 1 and 2 |
| 9 | Experimental `master.db` reader | With the WAL copy rule (finding 13) |
| 10 | Installers, onboarding and user docs | Plus signing, notarization and updates if v1 is for other DJs (finding 4) |

## Additions to the risk register

Milestone numbers in this table follow the proposed order above.

| Risk | Effect | Mitigation | When |
| --- | --- | --- | --- |
| MP3 start handling differs from rekordbox's | Cues and seeks 26 to 50 ms off; upgrades carry wrong cues | Synthetic fixtures, match rekordbox, one owner check | 6, 8 |
| rekordbox does not update tracks already in the collection from XML | No path or cue change through XML | Test early; design upgrades without it | 2, 8 |
| Transient file locks on Windows | Failed moves and archives | Retry with backoff; test on the Windows runner | 4 |
| Unsigned or unnotarized builds | Other DJs cannot or will not install | Budget signing and notarization | 10 |
| rbxport crates change upstream | Breakage when updating | Pin a revision; a small adapter in `oc-rekordbox` | 5, 9 |
| The labeled set costs too much, or leaks names | No gate for 3 and 7, or private data in the repository | Labels from the review queue, kept in app data, aggregate reports only | 1, 3 |
| Bandcamp pages or terms change | The Bandcamp provider breaks or is not allowed | Fetch only release pages reached from links in the files; cache; best effort; recheck the terms | 7 |
| A copy of `master.db` misses WAL changes | The experimental reader shows stale data | Copy the WAL, `quick_check`, show the copy's age | 9 |

## Questions for the owner

1. Is v1 for you, or for other DJs at release? This decides configurable rules, signing, notarization and macOS checks (finding 4).
2. Upgrades of tracks rekordbox already has: drop from v1, a guided Relocate in rekordbox, or a write mode for the live database (finding 1)?
3. May dedupe and upgrades replace files in the existing library, one at a time with your approval (finding 2)?
4. How do Bandcamp purchases arrive in your download folders today: ZIP files or unpacked folders (finding 9)?
5. Lossy files that are not MP3: keep AAC as it is, and transcode only what rekordbox cannot play (finding 11)?
6. Hi-res files: keep them, or resample above 48 kHz? Which players do you and the DJs you have in mind play on (finding 11)?
7. Who builds the hand-checked set, how many releases, and how much time can you give it (finding 8)?
8. To replace question 1 in ASSUMPTIONS: keep the rekordbox import file updated after every filing, or only when you press Send (finding 3)?

## Smaller issues and doc fixes

| Where | Issue | Fix |
| --- | --- | --- |
| Game Plan, Library storage, "Staging, archive and database" | Puts the database "on the same volume as the library where possible"; the Database section, A19 and decision 0001 put it in the app data directory | Drop "and database" from that row |
| Decision 0004 | "Play to first audio under 150 ms for local or cached files". The Game Plan says "a local file"; "cached" is left over from NAS streaming | Use the Game Plan's wording |
| docs/README, "Decided" | Lists decisions 0003 and 0007 as decided; their status lines say proposed | Make the two agree |
| A22, "If wrong" column | Points to HANDOFF "Open decisions", a section that no longer exists | Point to HANDOFF section 8 |
| A7 | Calls rbxport's lints "the owner's own standard". rbxport is a fork of chrisle/rbxport, and its LICENSE names the upstream author as copyright holder | Reword. Treat `rbl-*` as third-party code: pin a revision, keep the notices |
| Workspace `Cargo.toml` | `rust-version = "1.89"` is never tested; the toolchain is pinned to 1.99.0 | Set it to the pinned version, or add an MSRV job |
| Interface tokens | The faint text color `#80808c` reaches 4.4:1 on raised `#1a1a20` and 4.1:1 on hover `#22222a`, under the 4.5:1 the doc asks for. It passes on the panel (4.8:1) and the base (5.1:1) | Lighten it a little, or keep it off raised and hover surfaces |
| Interface, keys | The mock shows Camelot keys ("8A"). How rekordbox writes `Tonality` in the export is not recorded | Store one canonical key and convert for display (Camelot, Open Key, classic); record the export's format in research 002 |
| Bandcamp provider (7) | Files carry the artist's URL, not the release's. bandcamp.com's robots.txt allows release pages and disallows `/search` (read 2026-10-22). The Terms of Use (effective 2026-05-07) allow personal, non-commercial use of content | Reach the release from the artist's page, not the search. Cache. Recheck the Acceptable Use policy before building |
| Waveform peaks | When they are computed is not said. Each needs a full decode; on a hard disk, a pass over 9,000 files is on the order of an hour of reads | Compute on first play, start the audio first, then draw the waveform; an optional background pass |
| Interface density | 40 px compact rows are roomy for DJs used to rekordbox | Consider a third, denser height |
| Import state diagram | No path back to review when filing finds a clash (rule 3), no rule for the staging files of rejected items, and a retry always restarts at Staged | Add the transitions and a clean-up rule |
| Tauri configuration | Capabilities and the asset-protocol scope are not mentioned | Limit commands and file access (the cache folder for artwork) in milestone 0 |
| Archive | Grows without limit | Show its size; manual clean-up later |
| Documentation load | The download-folder setting alone touched nine files (its LOG entry lists them). The same tables live in several places and already drift (decision 0004 above) | One home per fact: the Game Plan for decisions, BUILD-PLAN for scope, and the rest link to them. Move withdrawn records out of the reading path |

## Evidence

Local checks, 2026-10-22:
- This repository: `git log` (one commit, `4adfd1b`), `git diff --stat` on the working tree, and `wc -c` on every document before and after the review, to confirm the plan files did not change.
- `~/repositories/rbxport-beat-analysis`: `git remote -v` (origin `Calvin-Zikakis/rbxport-beat-analysis`, upstream `chrisle/rbxport`) and `git shortlog`; `LICENSE` (GPL-2.0-or-later); `crates/rbl-audio/Cargo.toml` (symphonia, `mp3lame-encoder`, `rubato`); `crates/rbl-db/Cargo.toml` (`rusqlite` 0.40 with `bundled-sqlcipher-vendored-openssl`); `crates/rbl-db/src/xml.rs` (XML reader, lossy UTF-8 decoding); `crates/rbl-index/src/xml_export.rs` (XML writer: uppercase hex, encoded colon); `crates/rbl-core/src/xml.rs` (a tag scanner with no XML crate); `crates/rbl-index/README.md` (in-memory columnar snapshot, windows of rows); `crates/rbl-db/src/new_library.rs` and `src/locate.rs` (WAL); `.github/workflows/validate-windows.yml` (self-hosted runner, Strawberry Perl, short target folder, compile check and one test); `src-tauri/Cargo.toml` (the rustls note).
- Arithmetic: 16-bit 44.1 kHz stereo AIFF is 176,400 bytes a second, so six minutes is 63.5 MB. 320 kbps is 40,000 bytes a second, so six minutes is 14.4 MB. Contrast ratios use the WCAG relative-luminance formula. The rule of three gives 3/200 = 1.5%.

Web sources, read or searched on 2026-10-22. Items marked "search summary" were seen only in search results, not read in full.
- rekordbox XML re-import skipping existing tracks: Rekord Cloud, "The rekordbox XML import bug", https://rekord.cloud/blog/the-rekordbox-xml-import-bug (returned 404 when fetched; search summary). Mixed In Key's rekordbox workflow, https://mixedinkey.com/workflows/use-mixed-in-key-with-rekordbox/ (search summary). Engine DJ forum, https://community.enginedj.com/t/engine-dj-v3-0-importing-rekordbox-xml-does-not-change-previously-imported-files/47365 (search summary).
- MP3 cue offsets: DJ collection converters, https://dj-kb.readthedocs.io/dj_collection_converters/ (read). Rekord Cloud track editor, https://rekord.cloud/wiki/track-editor (search summary). Algoriddim forum, https://community.algoriddim.com/t/26ms-shift-issue-when-converting-cues-loops-between-softwares/39259 (search summary).
- GitHub Actions billing: https://docs.github.com/billing/managing-billing-for-your-products/managing-billing-for-github-actions (read), and the pricing changelog, https://github.blog/changelog/2025-12-16-coming-soon-simpler-pricing-and-a-better-experience-for-github-actions/ (search summary).
- Bandcamp: Terms of Use, https://bandcamp.com/terms_of_use (read, effective 2026-05-07), and https://bandcamp.com/robots.txt (read).
- Facts stated from general knowledge rather than a source read today: Bandcamp album downloads arrive as ZIP files; symphonia 0.5 has no Opus or WMA decoder; Windows apps must opt in to long paths; some older Pioneer players stop at 48 kHz. Check any of them before relying on it.
