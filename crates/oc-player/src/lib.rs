//! Playing one track at a time: decoding with symphonia (AIFF, WAV, FLAC, MP3, AAC and ALAC in
//! MP4) and output through cpal, per the Game Plan. Files are only read.

mod engine;
mod output;
mod peaks;
mod resample;
mod source;

pub use engine::{Player, Status};
pub use peaks::peaks;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("the file could not be opened: {0}")]
    Io(#[from] std::io::Error),
    #[error("this file cannot be played: {0}")]
    Unreadable(String),
    #[error("this file holds no audio that can be played")]
    NoAudio,
    #[error("no sound: {0}")]
    NoDevice(String),
}
