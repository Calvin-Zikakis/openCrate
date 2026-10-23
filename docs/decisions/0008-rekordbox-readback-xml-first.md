# 0008: rekordbox owns the playlists, and the read-back starts from the XML export

Status: accepted 2026-10-22. These are the owner's own answers.

## Context

The owner said rekordbox owns the playlists, asked to drop the idea of writing playlists for Navidrome, and said the rekordbox read-back should default to the XML export, with an experimental setting for reading `master.db`. The owner's XML export already carries what the read-back needs: 9,008 collection entries, cues and loops, beat-grid points and a tree of 135 playlists in folders ([research 002](../research/002-owner-data-shapes.md)).

## Decision

1. **rekordbox is the source of truth for playlists, cues and grids.** openCrate does not edit playlists and writes no playlist files in version 1.
2. **Default read-back: the rekordbox XML export.** The owner picks the export file in settings. openCrate remembers it, re-reads it on request, and notices when its modified time changes. The export itself stays a manual step in rekordbox. What is read: collection entries, tempo and key, beat grids, cues and loops, the playlist tree, and entries whose files are gone.
3. **Experimental read-back: `master.db`.** A setting, off by default and labelled experimental. It reads rekordbox's database through rbxport's `rbl-db`. It only ever opens a copy of the database and never the live file.
4. **The option is compiled into every build.** A setting needs the code to be present. That means the SQLCipher build of `rusqlite` applies to the whole workspace from milestone 5 ([decision 0001](0001-sqlite-local-database.md)). If that costs too much (build time, installer size, what a Windows build needs), the option moves behind a Cargo feature and a separate build.
5. **Playlists are a read-only mirror.** They are rebuilt on each read-back and shown in the sidebar and as a library filter. Entries are matched to indexed files by decoded path.
6. **Missing files are listed, not hidden.** The owner's export has entries that point into a drive's recycle bin.
7. **Sending tracks to rekordbox stays.** Newly filed tracks are written to an XML that rekordbox imports, with `Location` spelled as rekordbox spells it. This is milestone 5.

## Not doing

- Playlist editing, playlist export files, and any m3u8 import or export.
- Anything for Navidrome. The earlier playlist decision ([0006](0006-playlists-m3u8-and-rekordbox.md)) is withdrawn.
- Two-way playlist sync.

## Reasoning

- The XML export is a documented, official route that needs no database access, no encryption library and no risk to the owner's rekordbox library. The owner already has one.
- `master.db` is live and richer, but it needs the SQLCipher build and a schema that can change with rekordbox updates. As an experimental option it can be tried without the plan depending on it.
- Making rekordbox the owner of playlists removes a whole class of conflicts, and it is how the owner works today.

## Consequences

- The `oc-playlist` crate is dropped. The workspace has nine crates plus the Tauri app.
- Milestone 2 becomes the XML read-back, and the player now needs only that milestone ([BUILD-PLAN](../BUILD-PLAN.md)).
- The playlist export tables are dropped from the data model, and the rekordbox mirror tables are added ([data-model](../design/data-model.md)).
- The reuse of `rbl-*` (vendor, submodule or git dependency, assumption A22) has to be settled by milestone 5, not milestone 2.
- Risk: a rekordbox update can change the XML or the database layout. Fixtures per format version and the experimental label contain it.
