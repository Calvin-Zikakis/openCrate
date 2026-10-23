# 0006: Playlists as m3u8 files, in a folder you choose, importable into rekordbox

Status: withdrawn 2026-10-22. The owner said rekordbox owns the playlists and asked to drop the playlists-into-Navidrome idea. See [0008](0008-rekordbox-readback-xml-first.md). Kept as a record of what was found about the owner's m3u8 files and about rekordbox's m3u8 import.

## Context

The owner cares about two things for playlists: choosing where the files are stored on the file system, and being able to import them into rekordbox too.

Today there are 55 playlists as `.m3u8` files in a `_Playlists` folder at the root of the shared library. Navidrome scans hourly and imports them, and the phone syncs from Navidrome. The owner has not tried importing them into rekordbox. The exact format of the files is in [research 002](../research/002-owner-data-shapes.md).

## What Navidrome documents (read 2026-10-22)

- `AutoImportPlaylists` is on by default and imports `.m3u` playlist files during a library scan. `.nsp` smart-playlist files are imported the same way. Discussions of `.m3u8` files show they are picked up too.
- `PlaylistsPath` limits where playlists are searched for. It takes folders or globs, and its paths must be relative to the music folder. Empty (the default) means any playlist file in the library is imported. So playlists have to live inside the music folder.
- A regular playlist imported from an `.m3u` file is read-only while its Auto-import option is on: the file owns the track list, and each scan replaces the tracks with the contents of the file. Rename, comment and public or private can still be changed. Navidrome never writes changes back to the file.
- Smart playlists imported from files are owned by the first admin user. `DefaultPlaylistPublicVisibility` makes imported playlists public by default.
- By default, folders whose name starts with a dot are ignored by the scanner (`Scanner.IgnoreDotFolders`).
- `Scanner.Schedule` is disabled by default. `Scanner.WatcherWait` (default 5 s) delays a scan after a file change is detected. `Scanner.ScanOnStartup` is on by default.
- Navidrome does not write to the music folder or the files, by design.
- Sources: https://www.navidrome.org/docs/usage/configuration/options/, https://www.navidrome.org/docs/usage/features/smart-playlists/, https://www.navidrome.org/docs/faq/

## Decision

1. **Playlists are objects in the app database:** a name, an optional group, and an ordered list of tracks ([data-model](../design/data-model.md)). A playlist references tracks, so a replaced file does not break it.
2. **Export targets** are a list the owner controls. Each target is a folder picked in the operating system's dialog (anywhere, on any drive), a path style, and a file-name template. The default target is `_Playlists` in the library root, relative paths, and `[{group}] {name}.m3u8`. A second target can write absolute paths somewhere else, for rekordbox.
3. **Format** is exactly the owner's existing one: UTF-8, LF line endings, no byte-order mark, `#EXTM3U`, `#PLAYLIST:<name>`, then for each track `#EXTINF:<seconds>,<artist> - <title>` and the path line. Relative paths use forward slashes and are relative to the playlist file (`../Artist/Release/File`). Absolute paths use the host's native form (`G:\Music\...` on Windows).
4. **Names:** a path is written exactly as the name is on disk, with no normalization. The playlist file name is sanitized with the same rules as library files.
5. **rekordbox, two routes**, because the owner has not yet tried importing an m3u8 into rekordbox and its handling of relative paths is unverified:
   - the XML route: playlists are written into the rekordbox XML with absolute `Location` values in rekordbox's own spelling ([research 002](../research/002-owner-data-shapes.md)). This is part of milestone 5. rekordbox writes this form itself in the owner's export, but a generated file still has to be tried in rekordbox (BUILD-PLAN risk register);
   - an absolute-path m3u8 target for rekordbox's playlist import, available from milestone 2.

   A short test on the desktop decides which route is recommended, and the result goes in research note 003.
6. **Import:** existing m3u8 files can be imported. A relative path is resolved against the playlist's folder and matched to an indexed file. An absolute path is matched by its path under a known root, then by file name within the same release folder. A line that cannot be resolved is reported, never dropped silently. The group comes from the `[Group]` prefix. The files are only read.
7. **Never clobber:** the app records each file it writes and a hash of its content. It overwrites a file only when the file still matches that hash. Any other file in an export folder, including the owner's 55 existing playlists, is left alone and flagged.
8. **No calls to Navidrome.** The file is the interface. Navidrome's own hourly scan picks the files up once the sync tool has delivered them.

## Open: who owns the playlists

The existing files look like exports of rekordbox playlists ([research 002](../research/002-owner-data-shapes.md)), so today rekordbox may be the source of truth, with a script or tool turning folders into `[Folder]` prefixes. Two ways to go:

| Option | What happens | Fits |
| --- | --- | --- |
| A: openCrate owns playlists (assumed for now) | The 55 files are imported once. From then on playlists are edited in openCrate and written to the export targets and to rekordbox. Edits made in rekordbox do not flow back | The Game Plan, where the app database is the source of truth and sync is one way |
| B: rekordbox owns playlists | openCrate reads the playlist tree from a rekordbox XML export and writes the m3u8 files, reproducing the `[Folder] Name.m3u8` convention. Playlists are never edited in openCrate | The owner's current habit, if confirmed |

Two-way sync is not proposed: it needs conflict rules that are not worth their cost. The owner decides. The answer changes the scope of milestone 2, not its shape.

## Not verified

These are checked once, with small test playlists, before relying on them:

- Whether rekordbox's playlist import accepts an m3u8 with relative paths. Third-party guides say rekordbox has a playlist import that takes m3u8, and a 2014 Pioneer forum thread shows an import with absolute paths matching tracks already in the collection. AlphaTheta's own documentation was not found in this research.
- Whether an absolute-path m3u8 imports into rekordbox with the right tracks, with non-ASCII names, and with a leading drive letter.
- That Navidrome resolves the owner's relative paths as expected. The owner's existing playlists use this form, so it is presumably working already; confirm.
- What Navidrome does when a playlist file is deleted, and whether tracks that have not reached the NAS yet appear after the next hourly scan.

## Not doing

- Navidrome smart playlists (`.nsp` files), and any use of Navidrome's API.
- Playlists edited on the phone flowing back to the desktop.
- Two-way sync with rekordbox.

## Consequences

- A small `oc-playlist` crate writes and reads the files, and `oc-core-db` gains the playlist tables ([data-model](../design/data-model.md)).
- Milestone 2 in [BUILD-PLAN](../BUILD-PLAN.md) delivers import, the editor, export targets and the rekordbox check. The XML route follows in milestone 5.
- The importer tolerates bytes that are not valid UTF-8 (two lines in the owner's files could not be read as text) and reports them.
- The path rules need Windows-style tests on every OS ([decision 0007](0007-windows-first-develop-on-mac.md)).
