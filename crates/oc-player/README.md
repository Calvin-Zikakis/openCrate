# oc-player

Playing one track at a time, and the waveform coloured by sound. Decoding is symphonia (AIFF, WAV, FLAC, MP3, and
AAC or ALAC in MP4); output is cpal, on the system's default sound device. Files are only read.

## Public API

| Item | Purpose |
| --- | --- |
| `Player::start()` | Start the engine thread. The sound device opens when the first track loads |
| `load(path, key, start_ms, play)`, `play`, `pause`, `seek(ms)`, `stop`, `set_volume` | Commands to the engine; they return at once |
| `status() -> Status` | The loaded track's key, whether it plays, position, length, whether it ended, and any error |
| `bands(path) -> Bands` | The close-up waveform: 300 slices a second, each with the level of the lows (below 200 Hz), mids and highs (above 2.5 kHz) from 0 to 255 |
| `overview(detail, buckets)` | The whole track in a fixed number of slices, each band's loudest moment, for the bar under the player |

## Behaviour and limits

- One engine thread decodes half a second ahead into a buffer, and the sound device drains it, so the window never
  waits on decoding.
- A file at another sample rate than the device (a 44.1 kHz file on a 48 kHz output) is resampled by linear
  interpolation, which is fine for previewing a track.
- Waveforms are made by decoding the file at 44.1 kHz, splitting it with Butterworth filters and measuring each
  slice's RMS level. The desktop app keeps the overview in the library and the close-up in memory.

## Validation

From the workspace root:

```sh
cargo test -p oc-player --locked
cargo clippy -p oc-player --all-targets --locked -- -D warnings
```

The tests decode synthetic files made in the test and do not open a sound device, except one skipped by default
that plays a moment of silence on the real output: `cargo test -p oc-player live_device -- --ignored`.
