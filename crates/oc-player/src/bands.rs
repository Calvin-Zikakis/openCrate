//! A waveform coloured by sound, in the style of `MiniMeters`' multi-band waveform: each slice of
//! a track gets the level of its lows, mids and highs. Decoded once at a fixed rate, split with
//! Butterworth filters, measured as RMS.

use std::path::Path;

use crate::source::Source;
use crate::Error;

/// Every file is decoded at this rate, so the filters are the same for all of them.
const RATE: u32 = 44_100;
const RATE_F: f32 = 44_100.0;
/// Below this the lows; above `HIGH_HZ` the highs; the mids between.
const LOW_HZ: f32 = 200.0;
const HIGH_HZ: f32 = 2_500.0;
/// Slices a second in the close-up view.
pub const DETAIL_PER_SECOND: u32 = 100;

/// One second-order section (RBJ cookbook), Butterworth Q.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    fn new(cutoff: f32, high_pass: bool) -> Self {
        let omega = std::f32::consts::TAU * cutoff / RATE_F;
        let (sin, cos) = omega.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let b = if high_pass {
            [
                f32::midpoint(1.0, cos),
                -(1.0 + cos),
                f32::midpoint(1.0, cos),
            ]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Self {
            b: b.map(|value| value / a0),
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, input: f32) -> f32 {
        let output = self.b[0] * input + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [output, self.y[0]];
        output
    }
}

/// Splits a mono signal into lows, mids and highs.
pub(crate) struct Splitter {
    low: Biquad,
    mid_high_pass: Biquad,
    mid_low_pass: Biquad,
    high: Biquad,
}

impl Splitter {
    pub(crate) fn new() -> Self {
        Self {
            low: Biquad::new(LOW_HZ, false),
            mid_high_pass: Biquad::new(LOW_HZ, true),
            mid_low_pass: Biquad::new(HIGH_HZ, false),
            high: Biquad::new(HIGH_HZ, true),
        }
    }

    pub(crate) fn push(&mut self, sample: f32) -> [f32; 3] {
        let mid = self.mid_low_pass.run(self.mid_high_pass.run(sample));
        [self.low.run(sample), mid, self.high.run(sample)]
    }
}

/// The close-up waveform: [`DETAIL_PER_SECOND`] slices a second, each the lows', mids' and
/// highs' level from 0 to 255.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bands {
    pub per_second: u32,
    pub slices: Vec<[u8; 3]>,
}

/// A level from 0 to 1 as a byte; RMS times the square root of two reads a full-scale sine as 1.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "clamped to 0..=255 before the cast; a slice holds a few hundred frames"
)]
fn byte(sum_of_squares: f32, count: usize) -> u8 {
    let rms = (sum_of_squares / count.max(1) as f32).sqrt() * std::f32::consts::SQRT_2;
    (rms.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Decode `path` (read only) and measure each band in every slice.
pub fn bands(path: &Path) -> Result<Bands, Error> {
    let mut source = Source::open(path, RATE)?;
    let window = (RATE / DETAIL_PER_SECOND) as usize;
    let mut splitter = Splitter::new();
    let mut sums = [0.0_f32; 3];
    let mut count = 0;
    let mut slices = Vec::new();
    while let Some(block) = source.next_block()? {
        for frame in block.as_chunks::<2>().0 {
            let parts = splitter.push(f32::midpoint(frame[0], frame[1]));
            for (sum, part) in sums.iter_mut().zip(parts) {
                *sum += part * part;
            }
            count += 1;
            if count == window {
                slices.push(sums.map(|sum| byte(sum, count)));
                sums = [0.0; 3];
                count = 0;
            }
        }
    }
    if count > 0 {
        slices.push(sums.map(|sum| byte(sum, count)));
    }
    Ok(Bands {
        per_second: DETAIL_PER_SECOND,
        slices,
    })
}

/// The whole track in `buckets` slices, each band's loudest moment in the slice, interleaved as
/// low, mid, high: `3 * buckets` bytes, or none for a track with no audio.
pub fn overview(detail: &Bands, buckets: usize) -> Vec<u8> {
    let slices = &detail.slices;
    if slices.is_empty() || buckets == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(buckets * 3);
    for bucket in 0..buckets {
        let from = (bucket * slices.len() / buckets).min(slices.len() - 1);
        let to = ((bucket + 1) * slices.len() / buckets).clamp(from + 1, slices.len());
        let loudest = slices[from..to].iter().fold([0_u8; 3], |most, slice| {
            [
                most[0].max(slice[0]),
                most[1].max(slice[1]),
                most[2].max(slice[2]),
            ]
        });
        out.extend_from_slice(&loudest);
    }
    out
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, reason = "test tones are short")]
mod tests {
    use super::*;

    /// The share of each band's energy for a steady tone at `hertz`.
    fn shares(hertz: f32) -> [f32; 3] {
        let mut splitter = Splitter::new();
        let mut energy = [0.0_f32; 3];
        for n in 0..RATE {
            let sample = (std::f32::consts::TAU * hertz * n as f32 / RATE_F).sin();
            let parts = splitter.push(sample);
            // Skip the filters' first moments.
            if n > RATE / 10 {
                for (sum, part) in energy.iter_mut().zip(parts) {
                    *sum += part * part;
                }
            }
        }
        let total: f32 = energy.iter().sum();
        energy.map(|band| band / total)
    }

    #[test]
    fn a_tone_lands_in_its_own_band() {
        let bass = shares(60.0);
        assert!(bass[0] > 0.9, "{bass:?}");
        let voice = shares(800.0);
        assert!(voice[1] > 0.8, "{voice:?}");
        let hats = shares(9_000.0);
        assert!(hats[2] > 0.9, "{hats:?}");
    }

    #[test]
    fn a_full_scale_sine_reads_near_full_level() {
        let mut sum = 0.0;
        for n in 0..1000 {
            let sample = (std::f32::consts::TAU * n as f32 / 100.0).sin();
            sum += sample * sample;
        }
        assert!(byte(sum, 1000) >= 250);
        assert_eq!(byte(0.0, 0), 0);
    }

    #[test]
    fn the_overview_keeps_each_bands_loudest_moment() {
        let detail = Bands {
            per_second: 100,
            slices: vec![[10, 0, 0], [200, 5, 1], [0, 0, 90], [0, 50, 0]],
        };
        assert_eq!(overview(&detail, 2), [200, 5, 1, 0, 50, 90]);
        assert_eq!(
            overview(&detail, 8).len(),
            24,
            "short input still fills every bucket"
        );
        assert_eq!(
            overview(
                &Bands {
                    per_second: 100,
                    slices: Vec::new()
                },
                4
            ),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn a_silent_file_gives_silent_slices_at_the_detail_rate() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.aif");
        std::fs::write(&path, oc_tag::fixtures::aiff(3)).unwrap();
        let found = bands(&path).unwrap();
        assert_eq!(found.per_second, 100);
        assert!(
            (299..=301).contains(&found.slices.len()),
            "{}",
            found.slices.len()
        );
        assert!(found.slices.iter().all(|slice| *slice == [0, 0, 0]));
    }

    #[test]
    fn bass_then_hats_show_up_in_order() {
        let Some(ffmpeg) = [
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ]
        .iter()
        .map(Path::new)
        .find(|path| path.is_file()) else {
            eprintln!("ffmpeg not found; a real file is not exercised");
            return;
        };
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("bands.flac");
        let ok = std::process::Command::new(ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("aevalsrc='if(lt(t,2),0.8*sin(2*PI*60*t),0.8*sin(2*PI*9000*t))':s=48000:d=4")
            .arg(&path)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let detail = bands(&path).unwrap();
        let summary = overview(&detail, 4);
        let first = &summary[3..6];
        assert!(first[0] > 150 && first[2] < 20, "bass first: {summary:?}");
        let end = &summary[9..12];
        assert!(end[2] > 150 && end[0] < 20, "hats last: {summary:?}");
    }
}

/// `cargo test -p oc-player [--release] -- --ignored --nocapture six_minute_track`
#[cfg(test)]
#[test]
#[ignore = "benchmark: run explicitly with --ignored --nocapture"]
fn six_minute_track() {
    let ffmpeg = Path::new("/opt/homebrew/bin/ffmpeg");
    assert!(ffmpeg.is_file(), "needs ffmpeg");
    let folder = tempfile::tempdir().unwrap();
    for name in ["six.aif", "six.flac", "six.mp3"] {
        let path = folder.path().join(name);
        let ok = std::process::Command::new(ffmpeg)
            .args([
                "-nostdin",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "anoisesrc=d=360:c=pink:r=44100:a=0.5",
                "-ac",
                "2",
            ])
            .arg(&path)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let started = std::time::Instant::now();
        let detail = bands(&path).unwrap();
        let _ = overview(&detail, 800);
        eprintln!(
            "{name}: {} slices in {:?}",
            detail.slices.len(),
            started.elapsed()
        );
    }
}
