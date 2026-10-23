# Research 002: shapes of the owner's real data

Date: 2026-10-22. Aggregate counts only. No track, artist or playlist name, and no part of these files, was copied into this repository. The raw files stay in the dev Mac's Downloads folder, outside the repo.

## What was read

| Dataset | What it is |
| --- | --- |
| rekordbox XML | `full_collection_export.xml`: a collection export from rekordbox 7.2.19 on the owner's Windows desktop, 10.4 MB, 141,422 lines |
| `_Playlists` | A copy of the owner's playlist folder: 55 files |
| One playlist | A single `.m3u8` the owner attached, used to read the line format |

Commands, run in place from the Downloads folder: `wc`, `head`, `grep -c`, `grep -o ... | sort | uniq -c`, `sed`, and `find ... | xargs grep`. Output was read in a terminal and summarized here.

## rekordbox XML

| Fact | Value |
| --- | --- |
| Collection entries | 9,008 (`<COLLECTION Entries="9008">`) |
| `<TRACK ` elements in total | 11,536, so about 2,528 are playlist references |
| `POSITION_MARK` (cues and loops) | 25,322 |
| `TEMPO` (beat-grid points) | 20,093 |
| `NODE` elements | 155. 135 are playlists (all `KeyType="0"`, so tracks are referenced by TrackID). The rest are the root and about 19 folders, by subtraction |
| Entries whose `Location` is not a `file://localhost/` URL | 0 |

Where the 9,008 entries live:

| Location | Entries |
| --- | --- |
| One library folder on drive G: | 8,977 |
| The G: drive's recycle bin (deleted files rekordbox still lists) | 11 |
| A rekordbox folder on G: | 8 |
| A samples folder on D: | 6 |
| A user profile folder on C: | 5 |
| A folder on E: | 1 |

### How rekordbox writes `Location`

The form is `file://localhost/<drive>:/<folders>/<file>`, with forward slashes. Special characters are percent-encoded like this:

| Character | Written as |
| --- | --- |
| space | `%20` (98,626 times) |
| `'` | `%27` |
| `&` | `%26` |
| `[` and `]` | `%5b` and `%5d` |
| `%` | `%25` |
| non-ASCII | each UTF-8 byte as `%xx` (for example `%c3`) |

Hex digits are lowercase in every escape (4,782 escapes with letters, none uppercase). Left unencoded in this export: `,` (540 locations), `#` (33), `+` (33) and `$`. Characters that do not occur in the export, such as `{`, `^`, `~`, `;`, `=`, `@` and `!`, are unknown.

Why it matters: a generated XML whose `Location` differs from rekordbox's own spelling may not match existing collection entries, which would create duplicates. The writer must reproduce this set exactly and be tested against the real rekordbox ([BUILD-PLAN](../BUILD-PLAN.md), milestone 5).

## The `_Playlists` folder (historical)

The plan no longer uses these files: rekordbox owns the playlists and openCrate writes no playlist files ([decision 0008](../decisions/0008-rekordbox-readback-xml-first.md)). The facts are kept as they were measured.

| Fact | Value |
| --- | --- |
| Files | 55, all `.m3u8`, one flat folder, no subfolders |
| `#PLAYLIST:` header | present in all 55 |
| Line endings | LF in every file (0 with CR) |
| Byte-order mark | none. Every file starts with `#EXTM3U` |
| Track lines starting `../` | 2,547 |
| Track lines the check could not read as text | 2 (possibly not UTF-8) |
| Absolute paths among the classified lines | 0 |
| Track extensions | `.aif` 1,845, `.mp3` 638, `.aiff` 50, `.wav` 5 |

Line format, read from the attached playlist: `#EXTM3U`, then `#PLAYLIST:<name>`, then for each track an `#EXTINF:<seconds>,<artist> - <title>` line followed by the path. The path is relative to the playlist file, which sits one folder below the library root, so every path starts `../` and then `Artist/Release/File`.

Names: `[Group] Name.m3u8`, where the bracketed group looks like a rekordbox folder. One was checked: the XML has a folder node of that name holding 3 playlists, and no playlist in the XML carries the bracket prefix. So these files look like exports of rekordbox playlists, with folders turned into prefixes. That is an inference, not something the owner confirmed. Playlist names contain brackets, apostrophes (including the typographic one) and other non-ASCII characters.

## Library layout (from the playlist paths)

- `Artist/Release/File` throughout.
- File names are either `NN - Artist - Title.ext` or the raw Bandcamp form `Artist - Release - NN Artist - Title.ext`. Release folders can carry catalog tags in brackets.
- Earlier tools sanitized inconsistently: a `?` became `_`, while a `/` inside an artist credit was dropped, fusing two names into one folder name.
- AIFF files are mostly `.aif`, with a few `.aiff`.

## What this changes in the plan

1. Windows is the owner's real platform. The library is on a secondary drive (G:), and the music is not on the dev Mac ([decision 0007](../decisions/0007-windows-first-develop-on-mac.md)).
2. `{track:02} - {artist} - {title}` already matches the owner's tidier files, which supports assumption A12.
3. The existing library is mixed, so "index in place, nothing moves" is right. Only new imports follow the new rules.
4. Historical: this note first concluded that playlist files had to be written in exactly the format above ([decision 0006](../decisions/0006-playlists-m3u8-and-rekordbox.md), withdrawn). The owner then decided that rekordbox owns the playlists and openCrate writes no playlist files, so the plan reads the playlist tree from the rekordbox XML instead ([decision 0008](../decisions/0008-rekordbox-readback-xml-first.md)).
5. Importers must tolerate bytes that are not valid UTF-8 and report them, not fail.
6. The sanitizer replaces forbidden characters and never deletes them, so words are not fused.
7. rekordbox itself lists files that were deleted, so "in rekordbox but missing on disk" is a normal state to handle in the read-back.
8. AIFF output uses `.aif`.
