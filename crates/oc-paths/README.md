# oc-paths

How openCrate names files without losing anything: the stored path form, lookup keys that respect each volume's
rules, mapping rekordbox's Locations to library folders, read-only probes and walks of a folder, and the rules for
when a file counts as missing. The stored bytes decide: a lookup key may fail to find a match, but it must never
produce a false one. The crate has no database and never writes to the file system.

## Public API

| Item | Purpose |
| --- | --- |
| `RelPath`, stored form | A path relative to its music folder as byte-string components, stored as one blob joined by NUL, the same on every system |
| `wtf8::encode`, `wtf8::decode` | Windows file names as bytes, unpaired UTF-16 surrogates included, so every name round-trips |
| `NameProfile` (`Exact`, `FoldCase`, `FoldCaseNormalized`) | The rule that turns a path into a lookup key: exact bytes, letters folded to lower case, or Unicode NFC and then folded, to match how the volume compares names |
| `first_collision(profile, paths)` | Two paths the volume would treat as one name |
| `LocationPath`, `resolve(location, roots)` | A decoded rekordbox Location (such as `/G:/Folder/File.aif`) mapped to a music folder and a relative path, through aliases the owner confirmed |
| `prefix_histogram(locations, depth)` | The commonest leading folders among Locations, used to suggest which folder an export's paths belong to |
| `walk_root(root, options) -> WalkReport` | A read-only walk of a music folder: names, sizes, modified times and file tokens; it never follows a link |
| `probe_root`, `canonical(path)` | Questions about a volume, and a path with links and `..` resolved in its plain form (`C:\Music`, never `\\?\C:\Music`) |
| `may_mark_missing`, `next_presence`, `hold_mass_disappearance`, `match_status` | The rules for when a file is missing, offline or matched |

## Behaviour and limits

- A file is recognised by a token that survives a rename, a move within the volume and a restart: the inode on
  macOS, the NTFS file index on Windows. Windows opens each audio file with no access rights, only to ask for the
  index; nothing is read. Where a volume gives no token, a moved file is matched by name only.
- Offline belongs to the music folder, never to a file: a file on an unplugged drive is not marked missing.
- A scan that would mark at least 25 percent of a folder's files missing, and at least 20 files, is held for the
  owner to confirm instead of applied.
- Every stored path is made with `canonical`, so rekordbox's paths, XML links and stored folders compare in one form.

## Validation

From the workspace root:

```sh
cargo test -p oc-paths --locked
cargo clippy -p oc-paths --all-targets --locked -- -D warnings
```

The tests use temporary folders and synthetic names. Some cover Windows or Unix behaviour only and run on that
system.
