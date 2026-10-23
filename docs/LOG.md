# Progress log

Newest first. Each entry says what was done, what was measured, and what comes next.

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
