//! The fake-lossless check: a lossless file made from an MP3 or AAC keeps the lossy encoder's
//! low-pass, so its spectrum stops dead near 16 kHz (128 kbps) or 19 to 20 kHz (high bitrates),
//! where real lossless audio goes on to 22 kHz. This finds that edge. It flags; it never blocks.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

use crate::Error;

/// Everything is judged at this rate, so the edge means the same for every source.
pub const RATE: u32 = 44_100;
const FFT: usize = 4_096;
/// Bins per band: about 86 Hz, smooth enough to ignore single spikes.
const BAND: usize = 8;
/// A band counts as carrying sound when within this much of the 1 to 8 kHz level.
const BELOW_REFERENCE_DB: f64 = 40.0;
/// Sound above this is what lossless audio has and lossy encoders cut.
const FULL_BAND_HZ: u32 = 20_500;
/// Below this the cut is the 128 to 192 kbps kind and the file is almost certainly not lossless.
const CLEAR_CUT_HZ: u32 = 19_000;

/// The highest frequency that still carries sound, or `None` for silence. `samples` are mono
/// at [`RATE`].
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "bin counts and frequencies are far below f64 precision"
)]
pub fn cutoff_hz(samples: &[f32]) -> Option<u32> {
    if samples.len() < FFT {
        return None;
    }
    let fft = FftPlanner::<f64>::new().plan_fft_forward(FFT);
    let window: Vec<f64> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / FFT as f64).cos())
        .collect();
    let mut power = vec![0.0_f64; FFT / 2];
    let mut frames = 0_usize;
    let mut buffer = vec![Complex::new(0.0, 0.0); FFT];
    for chunk in samples.as_chunks::<FFT>().0 {
        for ((slot, sample), weight) in buffer.iter_mut().zip(chunk).zip(&window) {
            *slot = Complex::new(f64::from(*sample) * weight, 0.0);
        }
        fft.process(&mut buffer);
        for (total, value) in power.iter_mut().zip(&buffer) {
            *total += value.norm_sqr();
        }
        frames += 1;
    }
    let bin_hz = f64::from(RATE) / FFT as f64;
    let levels: Vec<(f64, f64)> = power
        .chunks(BAND)
        .enumerate()
        .map(|(index, band)| {
            let mean = band.iter().sum::<f64>() / (band.len() * frames) as f64;
            let centre = (index as f64 + 0.5) * BAND as f64 * bin_hz;
            (centre, 10.0 * (mean + 1e-30).log10())
        })
        .collect();
    let mut reference: Vec<f64> = levels
        .iter()
        .filter(|(hz, _)| (1_000.0..8_000.0).contains(hz))
        .map(|(_, db)| *db)
        .collect();
    reference.sort_by(f64::total_cmp);
    let reference = *reference.get(reference.len() / 2)?;
    if reference < -200.0 {
        return None;
    }
    let floor = reference - BELOW_REFERENCE_DB;
    // The highest pair of neighbouring bands both above the floor, so one stray band is ignored.
    levels
        .windows(2)
        .rev()
        .find(|pair| pair[0].1 > floor && pair[1].1 > floor)
        .map(|pair| whole_hz(pair[1].0))
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=RATE first"
)]
fn whole_hz(hz: f64) -> u32 {
    hz.round().clamp(0.0, f64::from(RATE)) as u32
}

/// What the edge says about a file that claims to be lossless, in plain words, or `None`.
#[must_use]
pub fn verdict(cutoff_hz: u32) -> Option<String> {
    let khz = f64::from(cutoff_hz) / 1000.0;
    if cutoff_hz >= FULL_BAND_HZ {
        None
    } else if cutoff_hz >= CLEAR_CUT_HZ {
        Some(format!(
            "May have been made from a high-bitrate MP3 or AAC: there is no sound above {khz:.1} kHz."
        ))
    } else {
        Some(format!(
            "Probably made from a lossy file such as an MP3: there is no sound above {khz:.1} kHz."
        ))
    }
}

/// Decode up to 30 seconds from 30 seconds in (from the start for a short file) to mono at
/// [`RATE`] and find its edge. The file is only read.
pub fn check(ffmpeg: &Path, path: &Path) -> Result<Option<u32>, Error> {
    let mut samples = decode(ffmpeg, path, 30)?;
    if samples.len() < RATE as usize * 5 {
        samples = decode(ffmpeg, path, 0)?;
    }
    Ok(cutoff_hz(&samples))
}

fn decode(ffmpeg: &Path, path: &Path, from_seconds: u32) -> Result<Vec<f32>, Error> {
    let mut child = crate::process::command(ffmpeg)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-ss"])
        .arg(from_seconds.to_string())
        .args(["-t", "30", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-ac", "1", "-ar"])
        .arg(RATE.to_string())
        .args(["-f", "f32le", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut bytes = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_end(&mut bytes)?;
    }
    let status = child.wait()?;
    if !status.success() && bytes.is_empty() {
        return Err(Error::Failed("the audio could not be decoded".into()));
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// Noise made of sines every 50 Hz up to `top_hz`, with fixed phases: a full or cut band.
    #[expect(clippy::cast_precision_loss, reason = "test signal")]
    fn tones(top_hz: u32, seconds: usize) -> Vec<f32> {
        let count = RATE as usize * seconds;
        let mut samples = vec![0.0_f32; count];
        let mut frequency = 50;
        let mut phase = 0.1_f64;
        while frequency <= top_hz {
            phase = (phase * 7.3 + 0.37).fract();
            for (n, sample) in samples.iter_mut().enumerate() {
                let t = n as f64 / f64::from(RATE);
                #[expect(clippy::cast_possible_truncation, reason = "test signal")]
                let add = (0.01
                    * (std::f64::consts::TAU * (f64::from(frequency) * t + phase)).sin())
                    as f32;
                *sample += add;
            }
            frequency += 50;
        }
        samples
    }

    #[test]
    fn the_edge_of_the_spectrum_is_found_where_the_sound_stops() {
        let full = cutoff_hz(&tones(21_800, 1)).unwrap();
        assert!(full >= 21_000, "{full}");
        let cut = cutoff_hz(&tones(16_000, 1)).unwrap();
        assert!((15_800..=16_300).contains(&cut), "{cut}");
        assert_eq!(cutoff_hz(&vec![0.0; RATE as usize]), None, "silence");
        assert_eq!(cutoff_hz(&[0.1; 100]), None, "too short");
    }

    #[test]
    fn the_verdict_names_the_edge_and_leaves_full_band_files_alone() {
        assert_eq!(verdict(21_500), None);
        assert!(verdict(19_800).unwrap().starts_with("May have been made"));
        assert!(verdict(16_000).unwrap().contains("no sound above 16.0 kHz"));
    }

    /// Real encoders: noise kept lossless, and the same noise through MP3 at 128 and 320 kbps
    /// and back to FLAC. The edges were measured with these encoders before the thresholds were chosen.
    #[test]
    fn mp3s_dressed_as_flac_are_caught_and_real_flac_is_not() {
        let Some(ffmpeg) = crate::find_ffmpeg() else {
            eprintln!("ffmpeg not found; real encoders not exercised");
            return;
        };
        let folder = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let status = Command::new(&ffmpeg)
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .current_dir(folder.path())
                .status()
                .unwrap();
            assert!(status.success(), "{args:?}");
        };
        run(&[
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=40:c=pink:r=44100:a=0.3",
            "-ac",
            "2",
            "real.flac",
        ]);
        for bitrate in ["128k", "320k"] {
            run(&[
                "-i",
                "real.flac",
                "-c:a",
                "libmp3lame",
                "-b:a",
                bitrate,
                &format!("{bitrate}.mp3"),
            ]);
            run(&[
                "-i",
                &format!("{bitrate}.mp3"),
                &format!("from-{bitrate}.flac"),
            ]);
        }
        let edge = |name: &str| check(&ffmpeg, &folder.path().join(name)).unwrap().unwrap();
        let (real, low, high) = (
            edge("real.flac"),
            edge("from-128k.flac"),
            edge("from-320k.flac"),
        );
        eprintln!("edges: real {real} Hz, from 128k {low} Hz, from 320k {high} Hz");
        assert_eq!(verdict(real), None, "{real}");
        assert!(verdict(low).unwrap().starts_with("Probably"), "{low}");
        assert!(verdict(high).is_some(), "{high}");
    }
}
