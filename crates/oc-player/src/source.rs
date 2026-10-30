//! One file decoded to interleaved stereo `f32` at the output's rate.

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use crate::resample::Resampler;
use crate::Error;

pub struct Source {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    rate: u32,
    /// The track's own clock, for turning a seek result back into milliseconds.
    time_base: Option<TimeBase>,
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
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|error| unreadable(&error))?;
        let (track_id, time_base, num_frames, params) = format
            .tracks()
            .iter()
            .find_map(|track| {
                let params = track.codec_params.as_ref()?.audio()?;
                Some((track.id, track.time_base, track.num_frames, params.clone()))
            })
            .ok_or(Error::NoAudio)?;
        let rate = params.sample_rate.ok_or(Error::NoAudio)?;
        let duration_ms = num_frames.map(|frames| frames.saturating_mul(1000) / u64::from(rate));
        // Gapless trims the encoder delay and padding, so a track starts and ends where it should.
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default().gapless(true))
            .map_err(|error| unreadable(&error))?;
        Ok(Self {
            track: track_id,
            format,
            decoder,
            rate,
            time_base,
            resampler: Resampler::new(rate, if out_rate == 0 { rate } else { out_rate }),
            duration_ms,
        })
    }

    /// The next block of interleaved stereo samples, or `None` at the end. A damaged packet is
    /// skipped rather than ending playback.
    pub fn next_block(&mut self) -> Result<Option<Vec<f32>>, Error> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(None),
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
            if packet.track_id != self.track {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(DecodeError::DecodeError(_)) => continue,
                Err(error) => return Err(unreadable(&error)),
            };
            let frames = decoded.frames();
            if frames == 0 {
                continue;
            }
            let channels = decoded.spec().channels().count();
            let mut samples: Vec<f32> = Vec::with_capacity(frames * channels);
            decoded.copy_to_vec_interleaved(&mut samples);
            let stereo = to_stereo(&samples, channels);
            return Ok(Some(self.resampler.process(&stereo)));
        }
    }

    /// Jump to `ms` from the start. Returns where playback really resumes, which can be a little
    /// earlier for formats that seek to a packet boundary.
    pub fn seek(&mut self, ms: u64) -> Result<u64, Error> {
        let time = Time::try_from_nanos_u128(u128::from(ms) * 1_000_000)
            .ok_or_else(|| Error::Unreadable("seek position out of range".to_owned()))?;
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
        let resumed_ms = match self.time_base {
            Some(time_base) => time_base.calc_time_saturating(seeked.actual_ts).as_millis(),
            None => i128::from(seeked.actual_ts.get()) * 1000 / i128::from(self.rate),
        };
        Ok(u64::try_from(resumed_ms).unwrap_or(0))
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

    /// FLAC and MP3 go through symphonia's codec bundles, which the hand-made fixtures cannot
    /// exercise (they hold no audio frames). With ffmpeg on the path the WAV fixture is encoded
    /// into both; without it this test checks nothing, like the converter's own tests.
    #[test]
    fn flac_and_mp3_decode_to_their_length_and_seek() {
        use std::process::Command;
        let ffmpeg_present = Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !ffmpeg_present {
            return;
        }
        let folder = tempfile::tempdir().unwrap();
        let wav = folder.path().join("a.wav");
        fs::write(&wav, oc_tag::fixtures::wav(2)).unwrap();
        for (name, options) in [("a.flac", &[][..]), ("a.mp3", &["-b:a", "128k"][..])] {
            let path = folder.path().join(name);
            let status = Command::new("ffmpeg")
                .args(["-y", "-loglevel", "error", "-i"])
                .arg(&wav)
                .args(options)
                .arg(&path)
                .status()
                .unwrap();
            assert!(status.success(), "{name}: ffmpeg failed");
            let mut source = Source::open(&path, 44_100).unwrap();
            let all = frames(&mut source);
            // Gapless decoding trims the MP3 encoder's delay and padding, so both land near
            // the two seconds the WAV held.
            assert!((87_000..=89_500).contains(&all), "{name}: {all}");
            // A FLAC seek lands on a block boundary (4096 frames here, 93 ms), so the reported
            // position matters more than the exact spot: what plays after it has to agree.
            let at = source.seek(1_000).unwrap();
            assert!((900..=1_000).contains(&at), "{name}: {at}");
            let rest = frames(&mut source);
            let expected = all - usize::try_from(at).unwrap() * 44_100 / 1000;
            assert!(
                rest.abs_diff(expected) <= 1_200,
                "{name}: {rest} frames after a seek reported at {at} ms, expected about {expected}"
            );
        }
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
