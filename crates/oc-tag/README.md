# oc-tag

Reading audio properties, tags and album art from one file, and writing tags and covers. Built on lofty.

## Public API

| Item | Purpose |
| --- | --- |
| `read_file(path) -> FileInfo` | The container, the audio facts (length, bitrate, sample rate, bit depth, channels) and the tags |
| `TagInfo` | Title, artist, album, album artist, label, genre, key, BPM, track and disc number, and year |
| `guess_from_path(components) -> TagInfo` | Values read from a path such as `Artist/Release/NN - Artist - Title.ext`, for a file whose tags are missing; only names are read |
| `tidy(tags)`, `tidy_name(value)` | Clean names before they become folders and tags: stray separators (`Artist;`, `A;B`), padding and doubled spaces |
| `embedded_cover(path)`, `folder_cover(directory)` | The front cover inside a file, or an image such as `cover.jpg` beside it |
| `write_tags(...)` | Write every tag and the cover into the importer's staged copy, before it is filed |
| `rewrite_tags(path, changes)` | Change some tags of a file already in the library: the track editor, Clean up, and a value kept from rekordbox in a sync |
| `rewrite_cover(path, cover)` | Put a cover into a library file, or take it out |
| `fixtures` (with the `fixtures` feature) | Tiny silent WAV, AIFF, FLAC and MP3 files for tests |

## Behaviour and limits

- Reading never writes. Text is trimmed and cut to 512 characters, an empty value counts as missing, and when a file
  holds several tags the primary one wins.
- Tags go into the file's own kind of tag. ID3 tags (MP3, AIFF, WAV) are written as ID3v2.3, which rekordbox reads.
- A file in the library is never written in place: the change goes into a copy, which then replaces the file in one
  rename. On any failure the copy is removed and the file is exactly as it was. A file with a damaged picture is
  refused rather than rewritten, so the picture is not lost.
- Covers are returned only as JPEG, PNG, WebP or GIF, recognised by their bytes, and nothing above 12 MB. A linked
  cover file is not followed.
- ID3v2.4 tags unsynchronised both as a whole and per frame, which some stores' downloads have, are read correctly.

## Validation

From the workspace root:

```sh
cargo test -p oc-tag --locked
cargo clippy -p oc-tag --all-targets --locked -- -D warnings
```

The tests write and read the synthetic files from `fixtures`; no real recording is used.
