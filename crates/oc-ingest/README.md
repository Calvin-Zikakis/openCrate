# oc-ingest

The rules for watching a download folder: which names are still downloading, whether a file has stopped changing,
and which files make up one release. There is no clock and no database: the caller passes the time in and decides
what to do with the answer. Only `list_folder` touches the file system, and only to read. Nothing here changes a
file in a watch folder; the desktop app copies a ready release out of it.

## Public API

| Item | Purpose |
| --- | --- |
| `classify(name) -> EntryKind` | What a name is: audio (with its container), an archive, artwork, a partial download, litter to ignore, or other |
| `path_has_partial(components) -> bool` | Whether any part of a path is a partial download, so a folder still being written waits |
| `list_folder(root) -> Listing` | Read a watch folder's entries and their size and modified time, at most 6 folders deep and 100,000 entries |
| `evaluate(...) -> Vec<ReleaseStatus>` | Group entries into releases and say which are `Ready` and why the rest are `Waiting` |
| `StabilityTracker` | Remember each file's size and modified time across looks, and say when it has stopped changing |
| `StabilityConfig` | How long a file must stay the same (`quiet_ms`), how many identical sightings it needs, and whether an old modified time is trusted on first sight |

## Behaviour and limits

- A release is the first folder below the watch folder, however deep its files are; a file directly in the watch
  folder is a release of its own.
- A partial download is known by its name: browser and client suffixes such as `.part`, `.crdownload`,
  `.download`, `.!ut`, `.!qb`, `.aria2` and `.bc!`, and the `INCOMPLETE~` prefix some peer-to-peer clients use.
  A release with any of them waits.
- A release also waits while its files are still changing, or while it holds nothing importable (only artwork,
  notes or litter). `Thumbs.db`, `desktop.ini` and `.DS_Store` are ignored.
- Archives (zip, rar, 7z) are recognised but not unpacked.

## Validation

From the workspace root:

```sh
cargo test -p oc-ingest --locked
cargo clippy -p oc-ingest --all-targets --locked -- -D warnings
```

The tests use names and timings made up in the test, plus temporary folders for `list_folder`.
