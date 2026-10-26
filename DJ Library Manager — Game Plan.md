# DJ Library Manager — Game Plan

Oct 22, 2026 · @Calvin

## Decisions so far

v1 is a free, GPL-3.0 desktop app in Rust + Tauri for macOS and Windows. It turns downloads from any source into a clean Artist/Release library, syncs it to rekordbox through XML, and plays it back with rekordbox's cues on a waveform.

| Decision | Choice | What it forces |
| --- | --- | --- |
| Audience | Other DJs from day one | Installers, onboarding, configurable rules and user docs are v1 work |
| Platforms | macOS and Windows. The owner develops on a Mac and runs the app on a Windows desktop | Paths, filenames and Unicode must behave identically on both. Windows builds and tests run in CI from the start, because nothing can be run on Windows locally |
| Stack | Rust + Tauri 2 | Same stack as rbxport, so its rekordbox crates are reusable |
| License | GPL-3.0 | Compatible with rbxport's GPL-2.0-or-later code |
| Import automation | Confident matches file themselves; the rest go to a review queue | Every match needs a confidence score |
| Analysis (BPM, grid, key, cues) | rekordbox analyzes; the app reads it back | The player shows rekordbox's data rather than its own |
| rekordbox read-back | The rekordbox XML export is the default. A setting turns on an experimental read of `master.db`, on a copy | An XML reader comes first. The `rbl-*` crates and the SQLCipher build are needed only for the experimental option |
| File moves after sync | Frozen by default; opt-in database mode can fix paths | XML-only users never get broken rekordbox links |
| Player v1 | Waveform with cue overlay | Waveform rendering and cue read-back are v1 work |
| Sync targets v1 | rekordbox only, via XML | Serato, Traktor and Engine wait for later |
| Playlists | rekordbox owns them. openCrate shows them read-only, from the rekordbox XML export | A read-back of the playlist tree. No playlist editing and no playlist files in v1 |
| New music sources | Any download folder: web shops, peer-to-peer clients, rips | Whole-release folders, inconsistent tags, frequent fake lossless |
| Singles | Artist/Release/Track | Every release gets its own folder |
| Artist folder | Remixer for remixes | A remix EP splits across several remixer folders |
| Upgrades | Replace with the better copy, keep cues, archive the old file | Synced tracks need database mode; otherwise they wait in review |
| Conversion | Lossless to AIFF (`.aif`, as most of the owner's AIFF files use); all other lossy to MP3 | Source format and bitrate must be recorded before converting |
| Existing library | Index in place; nothing moves | Only new imports run the full pipeline. The owner's library mixes naming styles, so it is never renamed |
| Interface | Spotify-style: dark, artwork first, left sidebar, search on top, player bar at the bottom. No rekordbox-style dense panels | A design system, an artwork cache and virtualized lists are v1 work |
| Speed | The UI never waits on disk, the network or the database writer. Budgets are enforced by a benchmark | Background workers, windowed IPC, local caches, a 100,000-track benchmark |
| Database | SQLite stays. It holds no audio; current schema v1 is lifecycle only | WAL FULL, online snapshots before existing-schema upgrades, none before initial creation; library tables, search and workers follow later |
| Library location | The library folder on the desktop is the source of truth. The NAS holds a synced copy, and nothing is streamed from it. Network folders stay supported but are not the main case | Library roots plus relative paths, an offline state, staging and archive outside the synced folder |
| Tag lookup | Local first. Online lookups are per release, cached, and throttled on each user's own connection | Per-provider queues and a response cache; no bundled catalog dump |
| Time | Nights and weekends, about 10 to 15 hours a week | Milestones sized at 2 to 5 weeks each |

Current implementation includes the reduced lifecycle/Location foundation in
[decision 0009](docs/decisions/0009-reduced-foundation-and-recovery.md) and the
synthetic visual sub-slice in [0010](docs/decisions/0010-synthetic-interface-shell.md).
The workspace has `oc-core-db`, `oc-rekordbox` and a minimal Tauri host for the
React shell. Decision 0010 allows that mock shell before filesystem indexing;
it does not complete milestone 0 or change milestones 1-9. Local evidence is
in [research 003](docs/research/003-reduced-foundation-validation.md) and
[004](docs/research/004-synthetic-interface-validation.md). Rust/frontend
workflows are configured, not remotely run; native UI readiness and Windows
installation remain unverified.

There is no library schema/windowed SQLite API, real database IPC, scanner,
XML parser, matching, reader pool or background database writer. Browser-demo
measurements are not an application/database benchmark. The rest of this
document describes the product plan, not shipped behavior. Live database writes, synced upgrades,
identity, publication and conversion policy remain review gates. No review is
closed wholesale and no new write mode or conversion policy is chosen here.

## Architecture and stack

Planned architecture: a Tauri 2 shell hosts a React + TypeScript UI over a Rust
workspace with one crate per pipeline stage. The app database will hold files,
tags and sync state. rekordbox stays the source of truth for playlists, cues and
grids. The jobs and dependencies below are future scope except for the lifecycle
and decoder pieces named above.

| Crate | Job | Key dependencies |
| --- | --- | --- |
| `core-db` | Tracks, releases, files, fingerprints, source quality, playlists (a read-only mirror of rekordbox's), sync state, review items, audit log | rusqlite (see Database) |
| `ingest` | Watch the download folders set in Settings, wait until a download stops growing, stage a copy | notify, plus polling for network shares |
| `identify` | Parse names, read tags, fingerprint, look up releases, score confidence | lofty, rusty-chromaprint, reqwest |
| `dedupe` | Fingerprint index plus version-aware title matching | rusqlite |
| `convert` | Lossless to AIFF, lossy to MP3, fake-lossless check | ffmpeg sidecar, rustfft |
| `tag` | Write final tags and artwork after conversion | lofty |
| `organize` | Path templates, filename sanitizing, safe moves, archive | std, plus unicode-normalization so names are NFC on both platforms |
| `rekordbox` | Write XML; read the XML export (default) or master.db (experimental) for cues, grids, playlists and existing tracks; opt-in path fixes | quick-xml, rbxport's `rbl-*` crates (for master.db) |
| `player` | Decode, play, generate waveform peaks | symphonia, cpal |

In the workspace the crates carry an `oc-` prefix (`oc-core-db`, `oc-organize` and so on).

ffmpeg ships as a Tauri sidecar binary for conversion only; playback stays in pure Rust. MP3 encoding needs an ffmpeg build that includes LAME, and the build's license must stay GPL-3.0 compatible.

## Database

SQLite stays. The audio lives in files and the database holds metadata only, so the size of the music does not matter; the number of tracks does. At about six minutes a track, 500 GB is roughly 8,000 tracks as AIFF or 35,000 as 320 kbps MP3 (an estimate), which is tens of megabytes of rows. SQLite handles millions of rows. rekordbox keeps its own library in SQLite (an encrypted build), and the beets project looked at Postgres and MySQL and decided against them: the workload is read-heavy with rare concurrent writes, and an embedded file needs no server.

The alternatives do not help at this size. A client-server database is a service to install and run on every DJ's laptop. Key-value stores such as LMDB or RocksDB mean rebuilding sorting, filtering and search by hand. DuckDB is built for analytics, not for steady small writes.

Planned performance architecture and current lifecycle safeguards:

| Concern | Approach |
| --- | --- |
| The UI never waits on an import | WAL is implemented; a background writer thread and reader pool are planned. The current synchronous owner must stay off the future UI thread |
| Long lists | Windowed reads: the list asks for rows start to start+n of a sorted view and the total is known up front, so the scrollbar is exact. The benchmark measures deep offsets; a position index replaces them if they ever exceed budget |
| Search as you type | An FTS5 index over artist, title, release and label |
| Hot tables stay small | Waveform peaks and artwork thumbnails live in cache files, not in track rows. Fingerprints get their own table |
| Recovery | Checked, flushed online backup snapshots before existing-schema upgrades, not initial version-0 creation. Migrations and `user_version` commit together. Dirty reopen checks integrity. Restore writes a new database in an unused local app directory, never overwrites and refuses orphan journals; callers select it explicitly. Scheduling and retention follow later |
| Rebuildable | Tags allow re-indexing, but backups must also protect review decisions, audit and sync state, aliases, original source quality, selected identifiers and pending operations that tags cannot reconstruct |
| Where it lives | Callers must choose a private local app-data directory, never the synced library or a network share. The crate does not detect network filesystems. SQLite WAL and concurrent sync copying make those locations unsuitable |

The lifecycle owner uses a cooperative OS file lock, application ID/schema checks
before opening an existing database for writes, and an explicit clean/dirty
marker. Dropping without orderly close leaves dirty state. Restore and new
database creation preserve and refuse orphan journals. These safeguards do not
implement a library mutation protocol or consistent copying of rekordbox's
external database.

One constraint from the stack: rbxport's `rbl-db` builds `rusqlite` with `bundled-sqlcipher-vendored-openssl` so it can open rekordbox's `master.db`. Cargo builds one SQLite per binary, so once `rekordbox` links `rbl-db` the app's own database runs on that build too, unencrypted. Until then `core-db` uses plain `bundled`, which avoids building OpenSSL from source.

Revisit only if the app gains multi-user or networked features, or real libraries pass about a million tracks.

## Interface and speed

The interface follows Spotify's layout and feel, not its brand: a dark theme, artwork first, a left sidebar for the library, imports and sync, a search field on top, and a player bar fixed at the bottom with the waveform and cue markers. rekordbox's dense panels are the thing to avoid. A compact row height is a setting for people who want more tracks on screen.

Starting budgets, enforced by a benchmark on a synthetic 100,000-track library and tuned after measuring:

| Interaction | Budget |
| --- | --- |
| Cold start to a usable library list | Under 1 s. The shell paints at once and rows stream in |
| Scrolling a long list | 60 fps. Only the visible rows are rendered |
| Search results after a keystroke | Under 50 ms at the 95th percentile |
| Opening a release or artist | Under 100 ms |
| Play to first audio | Under 150 ms for a local file |
| Any click | Visible feedback within 100 ms, even when the work continues in the background |

Rules that get there:

1. Nothing on the UI path touches a network drive or the internet. Rows, thumbnails and waveform peaks come from local storage.
2. Only what is on screen crosses the IPC boundary: a window of rows plus a little overscan, never the whole table.
3. Heavy work (scan, fingerprint, convert, waveform) runs in background workers with bounded concurrency, reports progress as events, and can always be cancelled.
4. Waveform peaks are computed once and stored in a compact binary form. The player never recomputes them.
5. Every list is virtualized, and animations touch only transform and opacity.
6. Release builds use fat LTO and a single codegen unit, as rbxport does.
7. Performance is a test: a run over budget fails the benchmark.

## Library storage

The reference setup is the owner's. The library folder on the desktop is the source of truth. It is mirrored to a NAS by the owner's QNAP QSync, which syncs both ways, with the desktop owning the files. openCrate works on the desktop folder only: nothing is streamed from the NAS, and openCrate never writes to the NAS itself. Most other DJs will have a local or external drive. A network folder is supported but is not the main case.

| Topic | Design |
| --- | --- |
| Where files are | A library root plus a path relative to it, both stored. Moving the folder, or remapping a drive letter, is one setting, not a rewrite of every track |
| Root offline | A root that is not mounted (an unplugged drive, a network folder away from home) is offline, not deleted. Nothing is marked missing and no row is removed while it is unreachable, and the library stays browsable from the database |
| Scanning | Incremental, keyed by path, size and modified time, with bounded parallel reads, and resumable. Only headers are read. Fingerprinting is for new imports |
| Watching | Local folders use file-system events. A root on a network drive is polled |
| Staging, archive and database | All three live outside the synced folder, on the same volume as the library where possible so the final move is an atomic rename. The sync tool then only ever sees finished files, and the NAS never receives archived copies or files waiting in the review queue |
| Imports | Stage, identify, convert and tag in staging, then move into the library. When staging and library are on different volumes, copy, verify size and checksum, and only then delete the staging copy |
| Sync with the NAS | QSync is the owner's tool, and openCrate does not talk to it. Because it syncs both ways, a move or delete here is a move or delete there. So replaced files are archived and never deleted, and nothing moves in the existing library |
| File names | Forbidden characters are replaced, never deleted. Names follow the strictest common rules (Windows reserved names, no trailing dots or spaces, NFC), and the whole path stays under Windows' 260 characters unless long paths are available. The folder is read on macOS, Windows and Linux |

## rekordbox

openCrate and rekordbox are kept in step both ways, like a Git merge ([design](docs/design/rekordbox-sync.md), agreed 2026-10-23; built 2026-10-24 on the owner's go-ahead). rekordbox keeps the analysis: cues, beat grids, keys and tempo.

| Topic | Design |
| --- | --- |
| Read-back, default | The rekordbox XML export. The owner picks the export file in settings. openCrate remembers it, re-reads it on request, and notices when it changes. It carries the collection, cues, beat grids and the playlist tree |
| Reading rekordbox's library | A setting, off by default, behind a warning: reads rekordbox's `master.db` through the vendored `rbl-db`, always from a backup copy taken while rekordbox is closed |
| Writing rekordbox's library | A second setting, off by default, behind its own warning, needing reading on: only for rekordbox versions the writer is tested with (7.2.x), only while rekordbox and its agent are closed, after a backup listed in Settings (restorable), and checked by reading back afterwards. It never deletes a playlist; a track entry is removed only when the owner merges duplicates, after its playlist places, My Tags and rating move to the copy kept (the losing file is archived beside the music folder, never deleted). On Windows, files are added only to a library that stores paths with forward slashes |
| Playlists | The sidebar shows rekordbox's tree. With writing on, tracks dropped on a rekordbox playlist go into it |
| New tracks | With writing on, newly imported tracks go straight into rekordbox, into one rolling top-level playlist "Imported", made if missing; tracks rekordbox already has are not added again. With writing off, they go into openCrate's XML file for the owner to import (`Location` spelled exactly as rekordbox spells it, [research 002](docs/research/002-owner-data-shapes.md)) |
| Track details | Title, artist, album, year and label are merged both ways against a base of what both sides last agreed: a one-sided change flows across, a change on both sides is the owner's choice, and nothing is applied that the owner did not tick. A value kept from rekordbox is written into the file's tags (through a copy, never in place) |
| Folder names | Library check can tidy a folder named from untidy tags: the files move (never over another file) and rekordbox is told where each one it uses went, so it keeps its analysis and playlists |
| Duplicates | Library check resolves a recording filed twice: the owner keeps one copy (the rule suggests the copy rekordbox analysed, then the one with most cues, then lossless, then bitrate; with cues on more than one, the owner chooses). The others move to an archive folder beside the music folder; in rekordbox their playlist places, My Tags and rating go to the kept copy before their entries are removed. Cues only the dropped copy has stop it |
| Missing files | An entry whose file is gone is listed and flagged, not hidden. rekordbox keeps such entries, for example files it still points at in the recycle bin |

## Identification and metadata

No single source is reliable for downloaded files, so the app combines several weak signals into one scored match per release. Anything below the auto-file threshold goes to the review queue.

Signals, in the order they are tried:

1. **Existing tags.** Files from web shops usually carry artist, title and album. Read everything, trust nothing yet. Files already tagged by beets, Picard or MiniMediaScanner carry MusicBrainz IDs, which count as a strong signal and can skip the lookup.
2. **Folder and file names.** Folders from peer-to-peer clients often look like `Artist - Album (2024) [FLAC]`. Strip format and quality tokens, then pull out track numbers and version strings (Extended Mix, X Remix, VIP, Edit).
3. **Shop page data.** Some web shops put the release page's address in the comment tag; the page's embedded structured data gives the canonical names.
4. **Release databases.** MusicBrainz is free but limited to about one request per second per IP address. Discogs needs a personal token, which each user enters in settings. See Looking up tags without hitting limits.
5. **Audio fingerprint.** Chromaprint plus AcoustID confirms a recording when it is known. Coverage of underground releases is thin, so a miss counts as neutral, not negative.

Rules that matter for DJs:

- Version is part of identity. "Original Mix" and "Extended Mix" are different tracks.
- A remix credit in the title, like "(X Remix)", is parsed into the Remixer field. That field drives the folder rule.
- A release is matched as a unit. If 9 of 10 tracks agree on one release, the tenth is probably a tagging error.
- Auto-file requires at least two independent signals to agree on artist, title and version.
- Score by weighted distance, as beets does: artist, title and release name count most, then track length, track count, year and label. Only a strong score files itself, and the best candidate must also be clearly ahead of the runner-up.
- Numbers must match exactly, whatever the text similarity. Vol. 2 is not Vol. 3, and 2020 is not 2021.

The review queue groups items by release. It shows the current and proposed value for each field and which source each came from, with accept, edit, and "always apply this mapping" (for example, an artist alias). Start the threshold strict, then tune it against a hand-checked set of a few hundred of your own downloads.

## Looking up tags without hitting limits

Reading the tags already in a file (ID3, Vorbis comments, MP4 atoms) is local and instant. The hard part is filling in what is missing, which needs a catalog. The tools below get around rate limits in different ways, and none of them is free of limits in general.

| Option | How it works | Verdict |
| --- | --- | --- |
| MusicBrainz, AcoustID and Discogs, live | Public APIs. MusicBrainz allows about 1 request per second per IP, AcoustID 3 per second, Discogs 60 per minute with a token (25 without) | Chosen for v1. The limits are per IP, so every DJ has their own budget. Query per release, cache everything, throttle locally |
| beets | Python CLI, MIT licensed. Its autotagger matches whole albums against MusicBrainz, and plugins add Discogs, Beatport, Deezer, Spotify, Tidal, AcoustID and shop sources. SQLite library | Not a dependency: it needs Python and has no installer story. Its matching model is the one to copy: weighted distance, strong, medium and low tiers, and a quiet mode that skips what it is unsure of |
| MiniMediaScanner and its Datasets | .NET CLI, GPL-3.0, requires PostgreSQL. The datasets are bulk copies of Deezer, Tidal, Spotify and MusicBrainz, gathered over months of API calls | Not for v1. The February 2026 dataset is 21.7 GB packed and 149 GB unpacked as SQL, it needs Postgres, it goes stale for new releases (which are what DJs import), the Datasets repository lists no license, and the data was harvested from commercial APIs whose terms need checking. Two ideas are worth taking: numbers must match exactly, and its 80% text and 98% fingerprint thresholds as a starting point |
| Lexicon, Find Tags & Album Art | Paid tier. Queries SonoVault, Lexicon's own database, by the existing artist and title text. Fills genre, year, label, album, artwork and energy-style fields | The limits are Lexicon's problem, on its own server. Not available to us without building a service |
| MusicBee auto-tag | User-triggered, one album at a time, against a configurable list of web sources | Never bulk, so never throttled |

What openCrate does:

1. **Local first.** Tags, names, MusicBrainz IDs already in the files, and a shop's release page need no catalog. Chromaprint fingerprints are computed locally; only the lookup goes online.
2. **Per release, not per track.** One MusicBrainz release lookup returns the whole tracklist. Fingerprint one or two tracks to find the release, then confirm the rest by track count and durations.
3. **Cache every answer, including misses,** by release ID, fingerprint and normalized query. Nothing is asked twice.
4. **Throttle each provider** with a token bucket and back off on 429 and 503. The queue runs in the background and resumes after a restart.
5. **Each user's own connection and tokens.** Discogs takes a personal token. AcoustID needs an application key and is free for non-commercial use only, which fits a free app; register the app and tell AcoustID before release.
6. **Bulk enrichment is opt-in and slow on purpose.** A 10,000-track library is on the order of 3,000 releases, which at one request per second is an hour or two in the background.
7. **Offline lookup is a later option.** MusicBrainz database dumps, the AcoustID database (daily JSON updates) and Discogs monthly dumps (CC0) allow lookups with no limit at all, at the cost of large downloads whose sizes must be measured first.
8. **The provider interface stays open,** so a hosted service or an offline dump can be added without touching the pipeline.

## What gets built

The order, scope and acceptance checks for each milestone are in [docs/BUILD-PLAN.md](docs/BUILD-PLAN.md). In short: a foundation with Windows CI, then the real library in the interface, then the rekordbox read-back, then the import pipeline (a dry run first), then conversion and filing, rekordbox export, the player, online lookups, dedupe and upgrades, and the installers.

Later, after version 1: plugins and scripting, so users can add their own steps (an import rule, a tag fix, an export target) without changing openCrate itself. Planned; not yet designed.

| Document | What it holds |
| --- | --- |
| [docs/BUILD-PLAN.md](docs/BUILD-PLAN.md) | Milestones, acceptance checks, test strategy, risks, what is out of version 1, and questions for the review |
| [docs/design/interface.md](docs/design/interface.md) | Screens, layout, visual tokens, behavior rules, and how speed is measured |
| [docs/design/data-model.md](docs/design/data-model.md) | Implemented lifecycle state and future table design; identity remains unresolved |
| [docs/design/import-pipeline.md](docs/design/import-pipeline.md) | Stages, states, where files live, and failure handling |
| [docs/decisions/](docs/decisions/) | One record per decision, 0001 to 0009 |
| [docs/research/](docs/research/) | Tag lookup evidence, historical aggregate data shapes and synthetic foundation validation |
| [docs/ASSUMPTIONS.md](docs/ASSUMPTIONS.md) | Every call made without the owner, and the questions waiting |
