# Research 011: What polling the download folders costs

Date: 2026-10-23. Apple M3 Pro, macOS 27, Rust 1.99.0. Synthetic folders in a temporary directory.

## Question

The Import screen asks the host every 3 seconds for the releases in the download folders, and the host lists them each
time (`downloads::poll`). Is file-system watching needed instead (build plan, milestone 3: "watching")?

## Command

A throwaway ignored test in `src-tauri/src/downloads.rs` (not kept): one download folder added first, then 2,000
release folders of 12 tiny `.flac` files and a `cover.jpg` each created in it, then five polls 3 seconds apart on the
test clock.

```sh
cargo test -p opencrate-desktop [--release] -- --ignored --nocapture zz_big_downloads
```

## Results

| Build | First poll | Later polls |
| --- | --- | --- |
| Release | 96 ms | 86 ms |
| Debug | 186 ms | 170 ms |

The same 2,000 releases present when the folder was added (so counted as already there) cost 0.3 ms for the first poll
and under 0.01 ms afterwards.

## What it shows

- Even with 2,000 new releases waiting, a poll every 3 seconds costs about 3% of one core in a release build, and only
  while the Import screen is open. A real download folder holds far fewer new releases.
- File-system watching would add a dependency and a second path (events plus a fallback scan) for no measurable gain,
  so it is not built. Revisit if a real folder measures over about 50 ms per poll.
- Tiny files: a release of real audio has the same number of directory entries, so listing cost is the same; sizes are
  read from metadata, not file contents.
