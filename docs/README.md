# Documentation index

The reduced foundation has two library crates plus a Tauri host in the Cargo
workspace. The React synthetic visual sub-slice is built locally under decision
0010. Browser validation and a macOS development bundle passed; the native
launch check established process liveness only. There is no library schema,
windowed SQLite API, real database IPC or matching. A standalone XML parser is
now tested locally with synthetic fixtures (research 005). Milestone 0 is
not complete. Both remote workflows and Windows installation remain unverified.
Start at [HANDOFF](HANDOFF.md) for current state and exact continue commands.

This folder holds the planning and progress record. The
[Game Plan](<../DJ Library Manager — Game Plan.md>), two reviews, crate READMEs
and [AGENTS.md](../AGENTS.md) live outside it. Root [CLAUDE.md](../CLAUDE.md)
imports the shared agent instructions. The repository remains private.

## Index

| File | What it holds |
| --- | --- |
| [../DJ Library Manager — Game Plan.md](<../DJ Library Manager — Game Plan.md>) | The source of truth for decisions, architecture, speed budgets and identification rules |
| [BUILD-PLAN.md](BUILD-PLAN.md) | Milestones 0 to 9, acceptance checks, test strategy, risks, what is out of version 1, and the questions for the review |
| [review-findings.md](review-findings.md) | What is still open from the two 2026-10-22 plan reviews (the full reviews are in git history) |
| [design/rekordbox-sync.md](design/rekordbox-sync.md) | Agreed design for two-way rekordbox sync; not built |
| [../crates/oc-core-db/README.md](../crates/oc-core-db/README.md) | Implemented lifecycle/recovery API, caller contracts and tests |
| [../crates/oc-rekordbox/README.md](../crates/oc-rekordbox/README.md) | Implemented byte decoder API and limits; no matching |
| [../src-tauri/README.md](../src-tauri/README.md) | Minimal synthetic desktop host, capability/CSP limits, build and preview commands |
| [ASSUMPTIONS.md](ASSUMPTIONS.md) | A1 to A66, including owner choices, foundation, shell, XML-reader and owner-lock calls; A28 to A32 and A43 withdrawn |
| [HANDOFF.md](HANDOFF.md) | Current state, environment, open decisions, and where a coding agent starts |
| [LOG.md](LOG.md) | Dated progress log, newest first |
| [design/interface.md](design/interface.md) | Built demo behavior versus planned screens/shortcuts, tokens, worker boundary and measurement limits |
| [design/data-model.md](design/data-model.md) | Implemented lifecycle `app_state`; future table sketch and unresolved identity/state contracts |
| [design/import-pipeline.md](design/import-pipeline.md) | Stages, import states, download folders, where files live, failure handling |
| [decisions/0001-sqlite-local-database.md](decisions/0001-sqlite-local-database.md) | SQLite stays, on the local disk |
| [decisions/0002-metadata-lookup-strategy.md](decisions/0002-metadata-lookup-strategy.md) | Local first, per-release lookups, cached and throttled |
| [decisions/0003-local-library-synced-copy.md](decisions/0003-local-library-synced-copy.md) | The desktop folder is the source of truth; the NAS holds a synced copy |
| [decisions/0004-interface-and-speed-budgets.md](decisions/0004-interface-and-speed-budgets.md) | A Spotify-style interface and measurable speed budgets |
| [decisions/0005-integrations-through-navidrome.md](decisions/0005-integrations-through-navidrome.md) | Navidrome and AudioMuse-AI integration (withdrawn, kept as a record) |
| [decisions/0006-playlists-m3u8-and-rekordbox.md](decisions/0006-playlists-m3u8-and-rekordbox.md) | Playlists as m3u8 files in a chosen folder (withdrawn, kept as a record) |
| [decisions/0007-windows-first-develop-on-mac.md](decisions/0007-windows-first-develop-on-mac.md) | Windows is the primary target; development is on a Mac |
| [decisions/0008-rekordbox-readback-xml-first.md](decisions/0008-rekordbox-readback-xml-first.md) | rekordbox owns the playlists; the read-back starts from the XML export, with an experimental `master.db` option |
| [decisions/0009-reduced-foundation-and-recovery.md](decisions/0009-reduced-foundation-and-recovery.md) | Accepted reduced first slice, lifecycle-only schema, online backups and no-clobber restore |
| [decisions/0010-synthetic-interface-shell.md](decisions/0010-synthetic-interface-shell.md) | Accepted synthetic visual slice before filesystem indexing; Node 24/npm, worker windows and browser validation |
| [decisions/0011-read-only-xml-parser.md](decisions/0011-read-only-xml-parser.md) | Standalone read-only rekordbox XML reader contract; synthetic fixtures only |
| [decisions/0012-explicit-owner-lock-release.md](decisions/0012-explicit-owner-lock-release.md) | The database owner lock is released by explicit unlock, not descriptor close; serial tests and retries rejected |
| [decisions/0013-filesystem-identity-and-availability.md](decisions/0013-filesystem-identity-and-availability.md) | How folders and files keep their identity, and what "unavailable" means (provisional, awaiting owner approval) |
| [decisions/0014-musicbrainz-lookup-on-request.md](decisions/0014-musicbrainz-lookup-on-request.md) | MusicBrainz lookups only on request from the Import all review; candidate choice, track matching, polite client |
| [research/001-tag-lookup-options.md](research/001-tag-lookup-options.md) | Rate limits and how beets, MiniMediaScanner, Lexicon and MusicBee get tags |
| [research/002-owner-data-shapes.md](research/002-owner-data-shapes.md) | Aggregate facts about the owner's rekordbox export, and about the old playlist files (historical) |
| [research/003-reduced-foundation-validation.md](research/003-reduced-foundation-validation.md) | Local commands, synthetic fixtures, final 25-test result, failure correction and validation limits |
| [research/004-synthetic-interface-validation.md](research/004-synthetic-interface-validation.md) | Node-24 commands, 12 unit/nine browser tests, latest synthetic measurements, native liveness and CI limits |
| [research/005-read-only-xml-validation.md](research/005-read-only-xml-validation.md) | XML reader commands, synthetic fixtures, 23 tests and limits |
| [research/006-owner-lock-inheritance.md](research/006-owner-lock-inheritance.md) | Why two database tests failed intermittently: measurements, isolating experiment, regression tests and limits |
| [research/007-volume-name-equivalence-probe.md](research/007-volume-name-equivalence-probe.md) | How the volume compares names (case, Unicode form), probed read-only |
| [research/008-library-query-benchmark.md](research/008-library-query-benchmark.md) | Windowed library queries on 100,000 synthetic tracks |
| [research/009-fake-lossless-edges.md](research/009-fake-lossless-edges.md) | Where MP3 and AAC encoders cut the spectrum, and the thresholds the fake-lossless check uses |

The XML parser is implemented under [decision 0011](decisions/0011-read-only-xml-parser.md).
[Research 005](research/005-read-only-xml-validation.md) records its synthetic
validation. The lock fix is [decision 0012](decisions/0012-explicit-owner-lock-release.md). Next: filesystem identity/state contracts before matching or library
migrations. Database/IPC benchmarks and experimental database parity remain later.

## For a reviewer

Read in this order:

1. The Game Plan, for the decisions.
2. [BUILD-PLAN.md](BUILD-PLAN.md), for the order of work and how each step is judged.
3. [ASSUMPTIONS.md](ASSUMPTIONS.md), especially the questions at the end.
4. [Decision 0010](decisions/0010-synthetic-interface-shell.md),
   [0009](decisions/0009-reduced-foundation-and-recovery.md), then
   [0008](decisions/0008-rekordbox-readback-xml-first.md) and
   [0007](decisions/0007-windows-first-develop-on-mac.md).
5. Both reviews, for findings and recommendations still awaiting decisions.
6. The crate/host READMEs and research 003/004, to separate built code from future design.

Status of the pieces:

- **Decided** (stated by the owner): the Game Plan's decisions table, the local library with a synced NAS copy ([0003](decisions/0003-local-library-synced-copy.md)), the interface and speed budgets ([0004](decisions/0004-interface-and-speed-budgets.md), accepted), Windows first ([0007](decisions/0007-windows-first-develop-on-mac.md)), and rekordbox owning the playlists with an XML-first read-back ([0008](decisions/0008-rekordbox-readback-xml-first.md), accepted).
- **Withdrawn** (kept as records): the Navidrome and AudioMuse-AI integration ([0005](decisions/0005-integrations-through-navidrome.md)) and playlists as m3u8 files ([0006](decisions/0006-playlists-m3u8-and-rekordbox.md)).
- **Accepted implementation slices:** [0009](decisions/0009-reduced-foundation-and-recovery.md)
  and [0010](decisions/0010-synthetic-interface-shell.md). Lifecycle, codec and
  synthetic visual shell pass local checks. Rust and frontend CI are configured;
  hosted runs, native UI readiness, Windows installation and the remaining
  foundation acceptance are pending.
- **Proposed:** later milestone cut lines, future table design and unconfirmed
  assumptions. Decision [0001](decisions/0001-sqlite-local-database.md) remains a
  broader storage proposal with recovery wording superseded by 0009;
  [0002](decisions/0002-metadata-lookup-strategy.md) remains a lookup proposal.
  Owner-confirmed choices in ASSUMPTIONS are not reopened by this reconciliation.
- **Open:** the list in [BUILD-PLAN, "Questions for the review"](BUILD-PLAN.md#questions-for-the-review), plus the questions at the end of ASSUMPTIONS. The ones that change the plan most are how rbxport's crates are reused, whether newly filed tracks go into rekordbox automatically, and whether the free CI minutes are enough. Where downloads land is no longer open: the download folders are a setting.
