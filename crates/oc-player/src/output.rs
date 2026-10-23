//! The default sound output, through cpal. Only this file touches the device.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

use crate::engine::Shared;
use crate::Error;

fn device_error(error: impl std::fmt::Display) -> Error {
    Error::NoDevice(error.to_string())
}

/// Open the default output in 32-bit float and start it. Returns the stream, which must be kept
/// alive, and its sample rate.
pub fn open(shared: Arc<Shared>) -> Result<(cpal::Stream, u32), Error> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| Error::NoDevice("there is no sound output".into()))?;
    let preferred = device.default_output_config().map_err(device_error)?;
    let config = if preferred.sample_format() == SampleFormat::F32 {
        preferred
    } else {
        device
            .supported_output_configs()
            .map_err(device_error)?
            .filter(|range| range.sample_format() == SampleFormat::F32)
            .find_map(|range| range.try_with_sample_rate(preferred.sample_rate()))
            .ok_or_else(|| {
                Error::NoDevice("the sound output does not take 32-bit float audio".into())
            })?
    };
    let channels = usize::from(config.channels());
    let rate = config.sample_rate();
    let stream = device
        .build_output_stream(
            config.config(),
            move |data: &mut [f32], _| shared.fill(data, channels),
            |error| eprintln!("sound output error: {error}"),
            None,
        )
        .map_err(device_error)?;
    stream.play().map_err(device_error)?;
    Ok((stream, rate))
}
