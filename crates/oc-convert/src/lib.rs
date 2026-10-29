//! Which files to convert on import, and converting them.
//!
//! The owner chooses one target for lossless files and one for lossy files. A file already in
//! its target format is never converted. Conversion runs ffmpeg (the Game Plan's choice; it
//! ships as a sidecar later) and writes audio only: tags and cover are written afterwards by
//! the importer, so nothing from the source's tag layout leaks through.

pub mod process;
pub mod spectrum;

#[cfg(test)]
mod process_tests;

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// An output format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Aiff,
    Wav,
    Flac,
    /// MP3 at 320 kbps.
    Mp3,
}

impl Format {
    /// The extension a converted file gets (`.aif`, the commoner AIFF extension).
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Aiff => "aif",
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
        }
    }

    /// The container name `oc_tag` reads for a file in this format.
    #[must_use]
    pub const fn container(self) -> &'static str {
        match self {
            Self::Aiff => "AIFF",
            Self::Wav => "WAV",
            Self::Flac => "FLAC",
            Self::Mp3 => "MP3",
        }
    }

    #[must_use]
    pub const fn is_lossless(self) -> bool {
        !matches!(self, Self::Mp3)
    }
}

/// What to do with each kind of file on import. `None` keeps files as they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOptions {
    pub lossless: Option<Format>,
    pub lossy: Option<Format>,
}

/// Whether a file is lossless, from the container its content was read as. M4A holds either
/// ALAC (lossless, which reports a bit depth) or AAC (lossy, which does not).
#[must_use]
pub fn is_lossless(container: &str, bit_depth: Option<u8>) -> bool {
    match container {
        "AIFF" | "FLAC" | "WAV" => true,
        "M4A" => bit_depth.is_some(),
        _ => false,
    }
}

/// The format a file should be converted to, or `None` to keep it as it is.
#[must_use]
pub fn target(options: ConvertOptions, container: &str, bit_depth: Option<u8>) -> Option<Format> {
    let chosen = if is_lossless(container, bit_depth) {
        options.lossless
    } else {
        options.lossy
    }?;
    (chosen.container() != container).then_some(chosen)
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Converting needs ffmpeg, which was not found on this computer.")]
    Missing,
    #[error("ffmpeg could not convert this file: {0}")]
    Failed(String),
    #[error("ffmpeg could not be started: {0}")]
    Io(#[from] io::Error),
}

/// Where ffmpeg is. An app started from the Dock does not get the shell's `PATH`, so the usual
/// Homebrew and system places are looked at too.
#[must_use]
pub fn find_ffmpeg() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let from_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let usual = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .iter()
        .map(PathBuf::from);
    from_path
        .into_iter()
        .chain(usual)
        .map(|folder| folder.join(name))
        .find(|candidate| candidate.is_file())
}

/// The codec arguments for a format. PCM keeps 16 bits, or 24 for anything deeper.
fn codec(format: Format, bit_depth: Option<u8>) -> Vec<&'static str> {
    let deep = bit_depth.is_some_and(|depth| depth > 16);
    match (format, deep) {
        (Format::Aiff, false) => vec!["-c:a", "pcm_s16be", "-f", "aiff"],
        (Format::Aiff, true) => vec!["-c:a", "pcm_s24be", "-f", "aiff"],
        (Format::Wav, false) => vec!["-c:a", "pcm_s16le", "-f", "wav"],
        (Format::Wav, true) => vec!["-c:a", "pcm_s24le", "-f", "wav"],
        (Format::Flac, false) => vec!["-c:a", "flac", "-sample_fmt", "s16", "-f", "flac"],
        (Format::Flac, true) => vec!["-c:a", "flac", "-sample_fmt", "s32", "-f", "flac"],
        (Format::Mp3, _) => vec!["-c:a", "libmp3lame", "-b:a", "320k", "-f", "mp3"],
    }
}

/// Convert the first audio stream of `source` into `output`, which must not exist. Audio only:
/// no tags, no pictures. The source is only read.
pub fn convert(
    ffmpeg: &Path,
    source: &Path,
    output: &Path,
    format: Format,
    bit_depth: Option<u8>,
) -> Result<(), Error> {
    // ffmpeg's `-n` declines to overwrite but still reports success, so look first.
    if output.symlink_metadata().is_ok() {
        return Err(Error::Failed("the output already exists".to_owned()));
    }
    let result = process::command(ffmpeg)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-n", "-i"])
        .arg(source)
        .args(["-map", "0:a:0", "-map_metadata", "-1", "-vn"])
        .args(codec(format, bit_depth))
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if result.status.success() {
        Ok(())
    } else {
        let message = String::from_utf8_lossy(&result.stderr);
        let first = message
            .lines()
            .next()
            .unwrap_or("it stopped with an error")
            .trim();
        Err(Error::Failed(first.chars().take(200).collect()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const KEEP: ConvertOptions = ConvertOptions {
        lossless: None,
        lossy: None,
    };

    #[test]
    fn only_files_not_already_in_the_target_format_are_converted() {
        let to_aiff_mp3 = ConvertOptions {
            lossless: Some(Format::Aiff),
            lossy: Some(Format::Mp3),
        };
        assert_eq!(target(to_aiff_mp3, "FLAC", Some(16)), Some(Format::Aiff));
        assert_eq!(target(to_aiff_mp3, "WAV", Some(24)), Some(Format::Aiff));
        assert_eq!(target(to_aiff_mp3, "AIFF", Some(16)), None, "already AIFF");
        assert_eq!(target(to_aiff_mp3, "MP3", None), None, "already MP3");
        assert_eq!(target(to_aiff_mp3, "OGG", None), Some(Format::Mp3));
        assert_eq!(target(to_aiff_mp3, "M4A", None), Some(Format::Mp3), "AAC");
        assert_eq!(
            target(to_aiff_mp3, "M4A", Some(16)),
            Some(Format::Aiff),
            "ALAC"
        );
        for container in ["FLAC", "MP3", "M4A", "AIFF"] {
            assert_eq!(target(KEEP, container, Some(16)), None);
        }
        let only_lossless = ConvertOptions {
            lossless: Some(Format::Flac),
            lossy: None,
        };
        assert_eq!(target(only_lossless, "WAV", Some(16)), Some(Format::Flac));
        assert_eq!(target(only_lossless, "OGG", None), None);
    }

    #[test]
    fn options_read_as_the_page_sends_them() {
        let options: ConvertOptions =
            serde_json::from_str(r#"{"lossless":"aiff","lossy":null}"#).unwrap();
        assert_eq!(options.lossless, Some(Format::Aiff));
        assert_eq!(options.lossy, None);
        assert!(
            serde_json::from_str::<ConvertOptions>(r#"{"lossless":"ogg","lossy":null}"#).is_err()
        );
    }

    #[test]
    fn a_wav_becomes_an_aiff_and_the_source_is_untouched() {
        let Some(ffmpeg) = find_ffmpeg() else {
            eprintln!("ffmpeg not found; conversion not exercised");
            return;
        };
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("in.wav");
        fs::write(&source, oc_tag::fixtures::wav(1)).unwrap();
        let before = fs::read(&source).unwrap();
        for format in [Format::Aiff, Format::Flac, Format::Mp3, Format::Wav] {
            let output = folder.path().join(format!("out.{}", format.extension()));
            convert(&ffmpeg, &source, &output, format, Some(16)).unwrap();
            let read = oc_tag::read_file(&output).unwrap();
            assert_eq!(read.container, format.container(), "{format:?}");
            assert!(read.tags.is_empty(), "no tags carried over");
        }
        assert_eq!(fs::read(&source).unwrap(), before);
        let taken = folder.path().join("out.aif");
        assert!(
            matches!(
                convert(&ffmpeg, &source, &taken, Format::Aiff, Some(16)),
                Err(Error::Failed(_))
            ),
            "an existing output is never overwritten"
        );
    }
}
