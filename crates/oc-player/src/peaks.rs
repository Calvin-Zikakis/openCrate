//! A waveform overview: the loudest sample in each of a fixed number of slices of a track, as
//! one byte each. Computed once per file and stored; the player only draws it.

use std::path::Path;

use crate::source::Source;
use crate::Error;

/// Frames folded into one first-pass peak, which keeps memory small for long tracks.
const STEP: usize = 512;

/// `buckets` peaks from 0 (silence) to 255 (full scale), in order. Decodes the whole file once,
/// read only.
pub fn peaks(path: &Path, buckets: usize) -> Result<Vec<u8>, Error> {
    let mut source = Source::open(path, 0)?;
    let mut coarse: Vec<f32> = Vec::new();
    let mut current = 0.0_f32;
    let mut count = 0;
    while let Some(block) = source.next_block()? {
        for frame in block.as_chunks::<2>().0 {
            current = current.max(frame[0].abs()).max(frame[1].abs());
            count += 1;
            if count == STEP {
                coarse.push(current);
                current = 0.0;
                count = 0;
            }
        }
    }
    if count > 0 {
        coarse.push(current);
    }
    Ok(fold(&coarse, buckets))
}

/// The largest value in each of `buckets` equal slices of `coarse`, scaled to a byte.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 before the cast"
)]
fn fold(coarse: &[f32], buckets: usize) -> Vec<u8> {
    if coarse.is_empty() || buckets == 0 {
        return Vec::new();
    }
    (0..buckets)
        .map(|bucket| {
            let from = bucket * coarse.len() / buckets;
            let to = ((bucket + 1) * coarse.len() / buckets)
                .max(from + 1)
                .min(coarse.len());
            let loudest = coarse[from.min(coarse.len() - 1)..to]
                .iter()
                .fold(0.0_f32, |a, b| a.max(*b));
            (loudest.clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_keep_their_loudest_moment_and_short_input_still_fills_every_bucket() {
        assert_eq!(fold(&[0.0, 1.0, 0.5, 0.25], 2), [255, 128]);
        assert_eq!(fold(&[0.5], 3), [128, 128, 128]);
        assert_eq!(fold(&[2.0, -1.0], 2), [255, 0], "clamped");
        assert_eq!(fold(&[], 10), Vec::<u8>::new());
    }

    #[test]
    fn a_silent_file_gives_a_flat_line_of_the_asked_length() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.aif");
        std::fs::write(&path, oc_tag::fixtures::aiff(3)).unwrap();
        let found = peaks(&path, 400).unwrap();
        assert_eq!(found.len(), 400);
        assert!(found.iter().all(|peak| *peak == 0));
    }

    #[test]
    fn a_loud_middle_shows_in_the_middle() {
        let Some(ffmpeg) = [
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ]
        .iter()
        .map(Path::new)
        .find(|path| path.is_file()) else {
            eprintln!("ffmpeg not found; a real waveform is not exercised");
            return;
        };
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("shape.flac");
        // Two seconds of silence, two of a loud tone, two of silence.
        let ok = std::process::Command::new(ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("aevalsrc='if(between(t,2,4),0.9*sin(2*PI*440*t),0)':s=44100:d=6")
            .arg(&path)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let found = peaks(&path, 6).unwrap();
        assert_eq!(found.len(), 6);
        assert!(found[0] < 5 && found[5] < 5, "{found:?}");
        assert!(found[2] > 200 && found[3] > 200, "{found:?}");
    }
}
