//! One file decoded to interleaved stereo `f32` at the output's rate.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use crate::resample::Resampler;
use crate::Error;

pub struct Source {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track: u32,
    rate: u32,
    resampler: Resampler,
    /// Total length, when the container says.
    pub duration_ms: Option<u64>,
}

fn unreadable(error: &DecodeError) -> Error {
    Error::Unreadable(error.to_string())
}

impl Source {
    /// Open `path` (read only) for output at `out_rate` frames a second; `0` keeps the file's
    /// own rate.
    pub fn open(path: &Path, out_rate: u32) -> Result<Self, Error> {
        let file = File::open(path)?;
        let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(extension);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                stream,
                &FormatOptions {
                    enable_gapless: true,
                    ..FormatOptions::default()
                },
                &MetadataOptions::default(),
            )
            .map_err(|error| unreadable(&error))?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or(Error::NoAudio)?;
        let params = track.codec_params.clone();
        let rate = params.sample_rate.ok_or(Error::NoAudio)?;
        let duration_ms = params
            .n_frames
            .map(|frames| frames.saturating_mul(1000) / u64::from(rate));
        let decoder = symphonia::default::get_codecs()
            .make(&params, &DecoderOptions::default())
            .map_err(|error| unreadable(&error))?;
        Ok(Self {
            track: track.id,
            format,
            decoder,
            rate,
            resampler: Resampler::new(rate, if out_rate == 0 { rate } else { out_rate }),
            duration_ms,
        })
    }

    /// The next block of interleaved stereo samples, or `None` at the end. A damaged packet is
    /// skipped rather than ending playback.
    pub fn next_block(&mut self) -> Result<Option<Vec<f32>>, Error> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(DecodeError::IoError(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(DecodeError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(error) => return Err(unreadable(&error)),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(DecodeError::DecodeError(_)) => continue,
                Err(error) => return Err(unreadable(&error)),
            };
            let spec = *decoded.spec();
            let frames = decoded.frames();
            if frames == 0 {
                continue;
            }
            let mut samples = SampleBuffer::<f32>::new(frames as u64, spec);
            samples.copy_interleaved_ref(decoded);
            let stereo = to_stereo(samples.samples(), spec.channels.count());
            return Ok(Some(self.resampler.process(&stereo)));
        }
    }

    /// Jump to `ms` from the start. Returns where playback really resumes, which can be a little
    /// earlier for formats that seek to a packet boundary.
    pub fn seek(&mut self, ms: u64) -> Result<u64, Error> {
        let time = Time::new(
            ms / 1000,
            f64::from(u32::try_from(ms % 1000).unwrap_or(0)) / 1000.0,
        );
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track),
                },
            )
            .map_err(|error| unreadable(&error))?;
        self.decoder.reset();
        self.resampler.reset();
        Ok(seeked.actual_ts.saturating_mul(1000) / u64::from(self.rate))
    }
}

/// Mono becomes both sides; more than two channels keep the first two.
fn to_stereo(interleaved: &[f32], channels: usize) -> Vec<f32> {
    match channels {
        2 => interleaved.to_vec(),
        0 => Vec::new(),
        1 => interleaved
            .iter()
            .flat_map(|sample| [*sample, *sample])
            .collect(),
        many => interleaved
            .chunks_exact(many)
            .flat_map(|frame| [frame[0], frame[1]])
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn channels_become_stereo() {
        assert_eq!(to_stereo(&[0.1, 0.2], 1), [0.1, 0.1, 0.2, 0.2]);
        assert_eq!(
            to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3),
            [1.0, 2.0, 4.0, 5.0]
        );
        assert_eq!(to_stereo(&[0.5, -0.5], 2), [0.5, -0.5]);
    }

    fn frames(source: &mut Source) -> usize {
        let mut total = 0;
        while let Some(block) = source.next_block().unwrap() {
            total += block.len() / 2;
        }
        total
    }

    #[test]
    fn wav_and_aiff_decode_to_their_length_and_seek() {
        let folder = tempfile::tempdir().unwrap();
        for (name, bytes) in [
            ("a.wav", oc_tag::fixtures::wav(2)),
            ("a.aif", oc_tag::fixtures::aiff(2)),
        ] {
            let path = folder.path().join(name);
            fs::write(&path, bytes).unwrap();
            let mut source = Source::open(&path, 44_100).unwrap();
            assert_eq!(source.duration_ms, Some(2_000), "{name}");
            let all = frames(&mut source);
            assert!((88_000..=88_200).contains(&all), "{name}: {all}");
            let at = source.seek(1_500).unwrap();
            assert!((1_400..=1_500).contains(&at), "{name}: {at}");
            let rest = frames(&mut source);
            assert!((21_000..=26_500).contains(&rest), "{name}: {rest}");
        }
    }

    #[test]
    fn a_different_output_rate_keeps_the_length_in_time() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.wav");
        fs::write(&path, oc_tag::fixtures::wav(1)).unwrap();
        let mut source = Source::open(&path, 48_000).unwrap();
        let all = frames(&mut source);
        assert!((47_900..=48_000).contains(&all), "{all}");
    }

    #[test]
    fn not_audio_is_an_error_not_a_panic() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("x.mp3");
        fs::write(&path, b"not audio at all").unwrap();
        assert!(Source::open(&path, 44_100).is_err());
        assert!(Source::open(&folder.path().join("missing.flac"), 44_100).is_err());
    }
}
