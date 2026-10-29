# oc-rekordbox

Everything openCrate exchanges with rekordbox: its Location paths, its XML export (read) and openCrate's own XML
file (written), and rekordbox's library database, `master.db`, read from a backup copy and written behind the
app's write setting.

## Locations

`location::decode_location(&str) -> Result<Vec<u8>, LocationError>` accepts the exact, case-sensitive
`file://localhost/` prefix. It percent-decodes the rest once and puts `/` in front, so
`file://localhost/G:/Synthetic%20Track.aif` becomes the bytes `/G:/Synthetic Track.aif`.

- Both hex cases are accepted. Literal `#`, `+`, punctuation, case and Unicode form are kept; this is not a URL
  parser or a form decoder.
- Percent-encoded bytes that are not UTF-8 are kept, so the output need not be valid UTF-8.
- `LocationError` is `UnsupportedPrefix`, `InvalidEscape { offset }` or `NullByte`. Messages never contain the path.

`location::encode_location` writes the Location rekordbox would write for an absolute path: the prefix, then the
path with forward slashes, percent-encoded with lowercase hex. Backslashes are separators only in a drive path.

Neither does any file system access, normalization, case folding or root mapping (that is `oc-paths`). A decoded
Location is bytes, not proof that a file exists or matches.

## XML

`xml::read(impl BufRead)` and `xml::read_with_limits(impl BufRead, Limits)` read a whole export with `quick-xml` and
return a typed `Collection`, or an error, only after the full input has been checked. The collection keeps track
attributes, the original and decoded Locations, cue and beat grid times in seconds, the playlist tree in order,
duplicate references and unresolved IDs. Unknown parts are skipped after checking, not imported.

The default limits are 128 MiB of input, 1,000,000 tracks, 1,000,000 playlist entries and 128 levels of nesting.
Malformed input, bad Locations, count mismatches, duplicate track IDs and unsupported entity tricks fail. Custom
limits can only be lower.

`xml::write_offer` and `xml::write_document` write openCrate's own small XML file: the tracks it offers to
rekordbox and the playlists for crates. They hold only what openCrate knows for sure; rekordbox analyses the files
itself.

## rekordbox's library (`db`)

`master.db` is an encrypted SQLite database, opened through the vendored `rbl-db` crate (`vendor/rbxport`).

- Reading: `take_snapshot` copies the database and its write-ahead log into a new backup folder, folds the log in
  and checks the copy, while rekordbox is closed. `read_snapshot` reads the copy into the same `Collection` an XML
  export gives, so the rest of the app treats both alike.
- Backups: `list_backups`, `set_pinned` and `prune` keep every pinned backup, the newest few, and the first one
  taken before writing on each of the last 14 days. `restore` checks a backup, saves the library it replaces as a
  pinned backup, and puts the chosen one back.
- Writing (`db::write`): `create_folder`, `create_playlist`, `add_to_playlist` (adding files rekordbox does not have
  yet), `set_fields` (the merge's title, artist, album, year and label), `relocate` (files that moved; analysis,
  cues, My Tags and playlists stay with the track) and `merge_duplicates` (two entries for one recording become one:
  the kept entry gains the other's playlist places, My Tags and rating, then the other is removed with rekordbox's
  own soft delete; refused when that would lose cues). Each write needs rekordbox closed, takes a backup first,
  reads back what it wrote, and puts the backup back if anything fails after it.
- Paths are written with forward slashes (`C:/...`), the form rekordbox uses for its own paths on Windows.
  `check_path_form` refuses to add files to a library that stores backslashes, a form not yet seen.

## Validation

From the workspace root:

```sh
cargo test -p oc-rekordbox --locked
cargo clippy -p oc-rekordbox --all-targets --locked -- -D warnings
```

The tests use synthetic XML and databases built in the test, including 10,000 generated tracks. No private export
or real rekordbox library is used. The writes were also checked by hand against a test rekordbox library.
