# Documentation index

Everything about openCrate is in this folder, except the [Game Plan](<../DJ Library Manager — Game Plan.md>) and the agent rules in [AGENTS.md](../AGENTS.md). There is no application code yet. The repository is private on GitHub.

## Index

| File | What it holds |
| --- | --- |
| [../DJ Library Manager — Game Plan.md](<../DJ Library Manager — Game Plan.md>) | The source of truth for decisions, architecture, speed budgets and identification rules |
| [BUILD-PLAN.md](BUILD-PLAN.md) | Milestones 0 to 9, acceptance checks, test strategy, risks, what is out of version 1, and the questions for the review |
| [ASSUMPTIONS.md](ASSUMPTIONS.md) | Every call made without the owner (A1 to A47, with A28 to A32 and A43 withdrawn), and the questions waiting for an answer |
| [HANDOFF.md](HANDOFF.md) | Current state, environment, open decisions, and where a coding agent starts |
| [LOG.md](LOG.md) | Dated progress log, newest first |
| [design/interface.md](design/interface.md) | Screens, layout, visual tokens, behavior rules, how speed is measured |
| [design/data-model.md](design/data-model.md) | The database schema, conventions and invariants |
| [design/import-pipeline.md](design/import-pipeline.md) | Stages, import states, where files live, failure handling |
| [decisions/0001-sqlite-local-database.md](decisions/0001-sqlite-local-database.md) | SQLite stays, on the local disk |
| [decisions/0002-metadata-lookup-strategy.md](decisions/0002-metadata-lookup-strategy.md) | Local first, per-release lookups, cached and throttled |
| [decisions/0003-local-library-synced-copy.md](decisions/0003-local-library-synced-copy.md) | The desktop folder is the source of truth; the NAS holds a synced copy |
| [decisions/0004-interface-and-speed-budgets.md](decisions/0004-interface-and-speed-budgets.md) | A Spotify-style interface and measurable speed budgets |
| [decisions/0005-integrations-through-navidrome.md](decisions/0005-integrations-through-navidrome.md) | Navidrome and AudioMuse-AI integration (withdrawn, kept as a record) |
| [decisions/0006-playlists-m3u8-and-rekordbox.md](decisions/0006-playlists-m3u8-and-rekordbox.md) | Playlists as m3u8 files in a chosen folder (withdrawn, kept as a record) |
| [decisions/0007-windows-first-develop-on-mac.md](decisions/0007-windows-first-develop-on-mac.md) | Windows is the primary target; development is on a Mac |
| [decisions/0008-rekordbox-readback-xml-first.md](decisions/0008-rekordbox-readback-xml-first.md) | rekordbox owns the playlists; the read-back starts from the XML export, with an experimental `master.db` option |
| [research/001-tag-lookup-options.md](research/001-tag-lookup-options.md) | Rate limits and how beets, MiniMediaScanner, Lexicon and MusicBee get tags |
| [research/002-owner-data-shapes.md](research/002-owner-data-shapes.md) | Aggregate facts about the owner's rekordbox export, and about the old playlist files (historical) |

Planned research notes: a note on the database benchmark, and later a comparison of the experimental `master.db` reader with the XML read-back. The numbering continues from 003.

## For a reviewer

Read in this order:

1. The Game Plan, for the decisions.
2. [BUILD-PLAN.md](BUILD-PLAN.md), for the order of work and how each step is judged.
3. [ASSUMPTIONS.md](ASSUMPTIONS.md), especially the questions at the end.
4. [Decision 0008](decisions/0008-rekordbox-readback-xml-first.md) and [decision 0007](decisions/0007-windows-first-develop-on-mac.md), which changed the most.

Status of the pieces:

- **Decided** (stated by the owner): the Game Plan's decisions table, the local library with a synced NAS copy ([0003](decisions/0003-local-library-synced-copy.md)), the interface and speed budgets ([0004](decisions/0004-interface-and-speed-budgets.md), accepted), Windows first ([0007](decisions/0007-windows-first-develop-on-mac.md)), and rekordbox owning the playlists with an XML-first read-back ([0008](decisions/0008-rekordbox-readback-xml-first.md), accepted).
- **Withdrawn** (kept as records): the Navidrome and AudioMuse-AI integration ([0005](decisions/0005-integrations-through-navidrome.md)) and playlists as m3u8 files ([0006](decisions/0006-playlists-m3u8-and-rekordbox.md)).
- **Proposed** (made by the agent, not yet confirmed): the milestone order and cut lines, the design docs, decisions [0001](decisions/0001-sqlite-local-database.md) and [0002](decisions/0002-metadata-lookup-strategy.md), and every assumption in ASSUMPTIONS that is not withdrawn.
- **Open:** the list in [BUILD-PLAN, "Questions for the review"](BUILD-PLAN.md#questions-for-the-review), plus the questions at the end of ASSUMPTIONS. The ones that change the plan most are where downloads land, how rbxport's crates are reused, and whether the free CI minutes are enough.
