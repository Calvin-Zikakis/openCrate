# rekordbox sync

Status: design, agreed with the owner on 2026-10-23 in conversation; not built. It changes decision
[0008](../decisions/0008-rekordbox-readback-xml-first.md) (rekordbox owned the playlists; read-back only) once approved,
and the Game Plan is to be updated then.

## Intent

rekordbox is the owner's source of truth today almost by accident: it holds where 7,000 files are, their tags, My Tags,
ratings, analysis and a playlist tree of folders by style with subgenre playlists inside. It is slow and fragile in that
role. openCrate should become the place the library is managed, with rekordbox kept for what it is good at (analysis,
beat grids, cues, USB export to the XDJs, playing out). Changes flow both ways, like a Git merge: what changed on one
side flows to the other, and where both sides changed the same thing, the owner chooses.

The owner's words on the stakes: "If OpenCrate was able to mess up anything I've done in Recordbox to this point, I would
be livid." Every part of this design serves that before it serves convenience.

## Goals

1. Playlists managed in openCrate in the owner's shape (folders by style, subgenre playlists inside), synced both ways.
2. Track information synced both ways with a three-way merge: tags, ratings and My Tags.
3. New imports sent to rekordbox without duplicates, into one rolling "Imported" playlist.
4. A safe cleanup of folder names with stray symbols, where rekordbox follows the moved files and keeps everything.
5. Duplicates resolved in favour of the copy rekordbox has analysed, with nothing deleted.
6. Backups before every write to rekordbox, and a restore.

## Non-goals

- Analysing tracks (beat grids, keys, waveforms for cue editing). rekordbox keeps that job. A far-off stretch goal at most.
- USB export to the XDJs. rekordbox keeps that job.
- Redesigning the owner's folder structure. The owner is happy with it; only stray symbols are cleaned.
- Replacing openCrate's crates. Crates stay openCrate's own scratchpad for prototyping playlists, separate from the
  rekordbox playlists.

## Anti-goals (a failure even if everything else works)

- Any track in rekordbox losing its file location, analysis, beat grid, cues, My Tags, rating or playlist membership.
- An analysed track replaced by an unanalysed copy, so it plays unanalysed in a set.
- A change written to rekordbox that the owner did not see and tick first.
- Writing to rekordbox while it is running, or to a rekordbox version the writer has not been tested against.
- A file deleted. Losing copies are archived, never deleted.

## Machines

- The owner's main library and main rekordbox 7 are on a Windows desktop, in a shared folder that Qsync mirrors to a QNAP
  NAS with the same structure.
- Development happens on the owner's Mac laptop, which has its own rekordbox 7 kept apart on purpose. The owner allows
  real rekordbox testing there ("I don't care if we break stuff on this computer"), still backed up first. The Windows
  library and the NAS stay off limits during development.

## Constraints

- Never touch the owner's real library, NAS or rekordbox during development: synthetic data and temporary copies only.
- XML wherever it can do the job; rekordbox's database (`master.db`) only where XML cannot.
- The database route is off by default, switched on in Settings behind a warning the owner must read.
- Every write to rekordbox is preceded by a backup, and every write can be restored from.
- rekordbox must be closed for any database read or write.

## How openCrate talks to rekordbox

| Need | XML (default) | Database (opt-in) |
|---|---|---|
| Read playlists, tracks, ratings, cues, grids | Owner exports XML in rekordbox; openCrate reads it (built today) | openCrate reads a copy of `master.db` directly |
| Read My Tags | Not in the XML (only if rekordbox copies them into Comments) | Yes |
| Add new tracks and append to "Imported" | openCrate writes an XML; the owner imports it in rekordbox | Written directly |
| Playlist changes (add, remove, reorder, move, rename, new) | Written as XML playlists for the owner to import; to be verified which changes rekordbox applies to existing playlists | Written directly |
| Tag, rating, My Tag changes to existing tracks | To be verified whether rekordbox updates existing entries on XML import | Written directly |
| Tell rekordbox a file moved, keeping its analysis | Not possible | Written directly |
| Merge duplicate entries, moving playlist membership | Not possible | Written directly |

With the database switched off, openCrate uses XML for everything XML can do: it writes the file and shows the owner the
few steps to import it in rekordbox. Changes XML cannot carry are shown as "needs the database (Settings)" and not
offered. With the database switched on, every change is written directly, with no import steps in rekordbox (owner,
2026-10-23: "avoid having to click through XML pushes").

### The database setting

Settings, rekordbox: "Let openCrate write to rekordbox's library", off by default. Turning it on shows a warning that
names what can go wrong (an unsupported rekordbox version, a format change), what openCrate does about it (backup before
every write, rekordbox must be closed, version check, verification after writing, restore), and asks for an explicit
confirmation. The setting records the rekordbox version it was confirmed against; a different version turns database
writes off again until the owner confirms anew and the version is known to work.

## The base snapshot (how "newer" is decided)

rekordbox does not record when an individual field was edited. So, like Git, openCrate keeps a base: a snapshot of every
synced value (per track: the fields below; per playlist: its name, place in the tree and ordered tracks) as both sides
agreed at the last sync. At the next sync:

- changed only in openCrate: goes to rekordbox;
- changed only in rekordbox: comes to openCrate (and into the file's tags, for tag fields);
- changed on both sides to different values: a conflict, shown with the base, openCrate's and rekordbox's values, and
  rekordbox's row update time where the database route provides one. The owner chooses; nothing is decided silently.

The first sync has no base. rekordbox's values are taken as the starting point (the owner's existing collection is
what must not be disturbed), and only openCrate values for fields rekordbox leaves empty are offered as additions.

### Fields

| Field | Synced | Notes |
|---|---|---|
| Title, artist, album, album artist, genre, label, year, comments, track and disc number | Both ways | Accepted values are also written into the file's tags (ID3v2.3) |
| Rating | Both ways | XML carries it; also written into the file where the format has a place for it |
| My Tags | Both ways | Database route only (the owner does not use rekordbox's option that copies My Tags into Comments) |
| Colour | Both ways | Builder's call whether it is shown in openCrate in the first version |
| BPM, key, beat grid, cues, loops | rekordbox to openCrate only | Analysis results; openCrate shows them and never writes them |
| File location | openCrate to rekordbox only | Only through the folder cleanup below, database route |

## Playlists

- openCrate's sidebar gains a "rekordbox" section showing the playlist tree exactly as in rekordbox. Crates stay a
  separate section.
- Everything rekordbox lets the owner do to a playlist is possible here: add and remove tracks, reorder, rename, make new
  playlists and folders, move a playlist between folders, delete a playlist (the tracks stay in the library).
- Change dots: orange, changed in openCrate and not yet sent; blue, changed in rekordbox since the last sync. A bar counts
  both directions: "3 changes to send, 2 from rekordbox", with "Review and sync".
- Review and sync opens the full list: "openCrate to rekordbox", "rekordbox to openCrate" and "Both changed", each change
  with a tick box. "Both changed" items are unticked by default and offer keep openCrate's, keep rekordbox's, or keep
  both (a playlist conflict kept both becomes two playlists). Sync writes only the ticked changes.
- A crate can be sent to rekordbox as a new playlist: built on the XML route (A113); tracks rekordbox has are echoed
  exactly from its own export.

Mockup agreed with the owner: sidebar with dots and the bar, and the review panel over it (kept outside the repository in
the brainstorm session; to be redrawn in the interface design).

## New music

1. openCrate imports a release as today (clean, tag, convert, file).
2. "Send to rekordbox" adds the newly filed tracks to rekordbox and appends them to one rolling playlist named
   "Imported", created if missing. Only tracks rekordbox does not already have are sent (the read-back matching knows
   which files rekordbox has), so no duplicates enter rekordbox. XML route.
3. The owner verifies beat grids in rekordbox, as today; the Imported playlist also serves as the fresh-tracks list on
   the XDJs.
4. The owner sorts tracks out of Imported into style playlists, in openCrate or in rekordbox; the sync carries it.
   A track can go into several playlists, so it does not leave Imported on its own. In the Imported playlist each track
   shows a check once it is in at least one other playlist, and "Clear sorted" removes only the checked ones (owner,
   2026-10-23).

## Folder cleanup

The Library check (built, read only) lists folder names with stray symbols. Cleanup turns them into proposed renames:

1. Every before and after path is listed in Review and sync, with tick boxes.
2. On sync: backup; move the files (no-clobber, through the existing publication protocol where it applies); update
   each moved track's location in rekordbox's database; then read rekordbox's database back and confirm every moved
   track resolves to an existing file. Any mismatch restores the backup and moves the files back.
3. An undo record makes a whole cleanup reversible as one step.

Needs the database route. With it off, the cleanup is shown but not offered.

The NAS copy follows through Qsync, which mirrors the desktop folder. Before the first real cleanup, check on a small test
folder that Qsync carries a folder rename as a rename (not a full re-upload), and that the new paths stay within what the
NAS accepts: the owner reports the NAS cannot handle very long paths, so the 260-character budget (A89) stays hard.

## Duplicates

- The copy rekordbox has analysed wins: one with a beat grid first, then one with cues.
- The losing copy is moved to an archive folder outside the synced library (beside it, like staging), never deleted.
- Playlists that held the losing copy get the winner in the same position, so no playlist loses a track (database route).
- When both copies are analysed, the owner chooses in Review and sync.
- The owner's roughly 260 duplicates are offered in batches, each through backup, review and sync.

## Upgrades (roadmap, owner asked for it)

Replacing an analysed track with a better copy (an analysed MP3 by a lossless file) without losing anything: the new copy
is imported alongside, the owner analyses it in rekordbox, and openCrate then moves playlist membership, My Tags and
rating to it and archives the old copy. Moving cues between different encodings can shift them (MP3 encoder delay), so
cue transfer needs its own measurement before it is offered. Its own design later.

## Safety

- Backup before every write: rekordbox's database and its settings files, plus openCrate's own database, into a dated
  folder openCrate controls. Kept: the last 20, and every backup taken before a cleanup or duplicate merge until the
  owner deletes it.
- Restore: Settings lists backups with their date, what the sync changed, and the rekordbox version; restoring requires
  rekordbox to be closed, restores the database files and reverses any file moves recorded with that sync.
- rekordbox must be closed: openCrate checks for the running process and for the database's lock before any database
  read or write.
- Version gate: database writes only for rekordbox versions on a tested list; others fall back to XML.
- Verification after every database write: read back and compare what was meant with what is there.
- Development uses synthetic rekordbox databases and copies only, never the owner's.

## Left to the builder

- Bringing in `rbl-db` (with `rbl-core` and `rbl-anlz`) from the owner's rbxport fork (owner, 2026-10-23: bring it in
  if it is sound). It already opens read-only by default, refuses writes while rekordbox runs, guards tests against the
  real library and records observations against rekordbox 7.2.x on macOS and Windows; its write paths are to be read in
  full before use. A vendored copy pinned to a recorded commit keeps builds offline and reproducible.
- The spike that proves reads and writes on a synthetic database, then on the laptop's rekordbox 7; the version gate
  starts with the 7.2 versions it passes on.
- Before any database write on Windows: `rbl-db`'s `import_file`/`import_files` store `FolderPath` with the platform's
  own separators (`to_string_lossy` of a `PathBuf`), so on Windows they would write `C:\\...` where rekordbox appears to
  store `C:/...`. Check a real Windows `djmdContent.FolderPath` (aggregate only) and convert if needed; until then the
  write version gate lists no Windows version. Found 2026-10-23 while reading the write paths; the database read is
  unaffected.
- Verifying, on a test rekordbox, exactly which XML imports update existing tracks and playlists, and the step-by-step
  instructions shown to the owner for an XML import.
- The schema for the base snapshot, change records, backups and undo records in openCrate's database.
- Matching openCrate tracks to rekordbox entries beyond the existing location matching.
- Interface details beyond the agreed mockup.

## Calls made in the design, not by the owner

- Losing duplicate copies are archived, not deleted.
- BPM, key, grids and cues are read-only in openCrate.
- The first sync takes rekordbox's values as the base.
- A playlist conflict kept on both sides becomes two playlists.
- Backups kept: the last 20, plus every pre-cleanup and pre-merge backup.
- Sending a crate to rekordbox as a playlist is on the roadmap, not in the first version.
