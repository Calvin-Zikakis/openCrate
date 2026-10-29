# oc-convert

Format conversion on import, and the check for lossless files made from lossy ones. Both run ffmpeg. Nothing here
reads or writes tags: the importer writes tags and cover into the converted copy afterwards.

## Public API

| Item | Purpose |
| --- | --- |
| `Format` | A target format: AIFF, WAV, FLAC or MP3, with its file extension and container name |
| `ConvertOptions { lossless, lossy }` | The owner's two choices: a target for lossless files and one for lossy files, or none |
| `target(options, container, bit_depth) -> Option<Format>` | What a file becomes; `None` when it stays as it is |
| `is_lossless(container, bit_depth) -> bool` | Whether a file is lossless (an M4A is ALAC when it reports a bit depth) |
| `find_ffmpeg() -> Option<PathBuf>` | ffmpeg on the `PATH`, then in `/opt/homebrew/bin`, `/usr/local/bin` and `/usr/bin` |
| `convert(ffmpeg, source, output, format, bit_depth)` | Convert one file, writing audio only, never over an existing file |
| `spectrum::check(ffmpeg, path) -> Result<Option<u32>>` | The frequency where the audio's content stops, from 30 seconds of the file |
| `spectrum::verdict(cutoff_hz) -> Option<String>` | A plain warning when that edge looks like an MP3 or AAC encoder's |
| `process::command(program) -> Command` | A `Command` that opens no console window on Windows |

## Behaviour and limits

- A file already in its target format is never converted.
- AIFF, WAV and FLAC keep 16 bits, or 24 for anything deeper. MP3 is 320 kbps and needs an ffmpeg built with LAME.
- The output gets the first audio stream only, with no metadata and no picture (`-map 0:a:0 -map_metadata -1 -vn`).
- The fake-lossless check looks for the low-pass a lossy encoder leaves: real lossless audio goes on to about
  22 kHz, while a file made from a 128 kbps MP3 stops near 16 kHz and one from a high bitrate near 19 to 20 kHz.
  It flags a file; it never blocks one.
- ffmpeg is not bundled. The Game Plan ships it as a sidecar later; until then it must be installed. An app
  started from the macOS Dock gets no shell `PATH`, which is why Homebrew's folders are searched too.
- Every program the workspace starts goes through `process::command`, so Windows never flashes a console window
  per file. A test fails if any other code calls `Command::new`.

## Validation

From the workspace root:

```sh
cargo test -p oc-convert --locked
cargo clippy -p oc-convert --all-targets --locked -- -D warnings
```

The tests that run real encoders need ffmpeg; without it they return without checking anything.
