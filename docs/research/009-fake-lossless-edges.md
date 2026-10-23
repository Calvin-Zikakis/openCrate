# Research 009: Where lossy encoders cut the spectrum, and whether the edge finder sees it

Date: 2026-10-23. Synthetic audio only. Apple M3 Pro, macOS 27.0.1, ffmpeg 9.0.2 (Homebrew,
with libmp3lame), Rust 1.99.0, rustfft 6.4. Release build for the timing.

## Method

`oc_convert::spectrum::check` decodes 30 seconds from 30 seconds in to mono 44.1 kHz with
ffmpeg, averages a 4,096-point Hann-windowed power spectrum, groups it into 8-bin bands (about
86 Hz), takes the median band level between 1 and 8 kHz as the reference, and reports the
highest pair of neighbouring bands within 40 dB of it.

Dataset: 90 seconds each of pink, white and brown noise (`anoisesrc`, stereo, 44.1 kHz) saved
as FLAC; each encoded to MP3 (libmp3lame CBR 128, 192, 256, 320 kbps) and AAC (ffmpeg's encoder,
256 kbps), then decoded back to FLAC, as a fake lossless download would be.

The sweep was a throwaway test, removed afterwards. The kept test
(`cargo test -p oc-convert mp3s_dressed_as_flac`) repeats the pink-noise 128 and 320 kbps cases.

## Results (edge in Hz; identical for pink, white and brown noise except where noted)

| Source | Edge | Verdict |
|---|---|---|
| Lossless | 22,007 | none |
| MP3 128 kbps | 16,753 (brown 16,667) | probably lossy |
| MP3 192 kbps | 18,820 | probably lossy |
| MP3 256 kbps | 19,509 | may be lossy |
| MP3 320 kbps | 20,198 | may be lossy |
| AAC 256 kbps (ffmpeg) | 22,007 | none (not detected) |

One check took 46 to 49 ms (release build), most of it ffmpeg decoding.

## Thresholds chosen

- At or above 20,500 Hz: no doubt.
- 19,000 to 20,499 Hz: "may have been made from a high-bitrate MP3 or AAC".
- Below 19,000 Hz: "probably made from a lossy file".

## Limits

- ffmpeg's AAC encoder at 256 kbps keeps the full band here, so such a file is not caught. Other
  AAC encoders (Apple's, at lower rates) low-pass and may be; not measured.
- Noise is not music. Real masters sometimes roll off near 20 kHz on purpose, which the "may"
  band would flag; the threshold should be checked on real known-good and known-bad files before
  the check can ever block anything (the design says it only flags).
- Sources above 44.1 kHz are judged after resampling to 44.1 kHz, so a 96 kHz file made from a
  44.1 kHz master is not distinguished.
