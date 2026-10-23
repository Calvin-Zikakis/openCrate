//! Linear interpolation between sample rates, carried across blocks. Plenty for previewing a
//! track when the output runs at a different rate (44.1 kHz files on a 48 kHz device).

pub struct Resampler {
    /// Input frames per output frame.
    step: f64,
    /// Where the next output frame falls, in input frames, counted from `last`.
    position: f64,
    /// The final frame of the previous block, so blocks join without a click.
    last: Option<[f32; 2]>,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            step: f64::from(input_rate) / f64::from(output_rate.max(1)),
            position: 0.0,
            last: None,
        }
    }

    pub fn reset(&mut self) {
        self.position = 0.0;
        self.last = None;
    }

    /// Interleaved stereo in, interleaved stereo out.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "positions are small, non-negative frame counts"
    )]
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if (self.step - 1.0).abs() < f64::EPSILON {
            return input.to_vec();
        }
        let mut frames: Vec<[f32; 2]> = Vec::with_capacity(input.len() / 2 + 1);
        frames.extend(self.last);
        frames.extend(input.as_chunks::<2>().0.iter().copied());
        if frames.len() < 2 {
            self.last = frames.last().copied();
            return Vec::new();
        }
        let mut out = Vec::with_capacity((input.len() as f64 / self.step) as usize + 4);
        while self.position + 1.0 < frames.len() as f64 {
            let index = self.position as usize;
            let fraction = (self.position - index as f64) as f32;
            let (a, b) = (frames[index], frames[index + 1]);
            out.push(a[0] + (b[0] - a[0]) * fraction);
            out.push(a[1] + (b[1] - a[1]) * fraction);
            self.position += self.step;
        }
        self.position -= (frames.len() - 1) as f64;
        self.last = frames.last().copied();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_rate_passes_through() {
        let mut same = Resampler::new(44_100, 44_100);
        assert_eq!(same.process(&[0.1, 0.2, 0.3, 0.4]), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn blocks_join_into_the_right_length_and_values_follow_a_ramp() {
        let mut up = Resampler::new(44_100, 48_000);
        let ramp: Vec<f32> = (0..44_100)
            .flat_map(|n| {
                #[expect(clippy::cast_precision_loss, reason = "test ramp")]
                let value = n as f32 / 44_100.0;
                [value, -value]
            })
            .collect();
        let mut out = Vec::new();
        for block in ramp.chunks(2 * 1_000) {
            out.extend(up.process(block));
        }
        let frames = out.len() / 2;
        assert!((47_990..=48_000).contains(&frames), "{frames}");
        // Interpolated values stay on the ramp, so no block edge leaves a jump.
        for pair in out.as_chunks::<4>().0 {
            assert!(pair[2] >= pair[0], "rising");
            assert!(
                (pair[2] - pair[0]) < 0.000_05,
                "no jump: {} -> {}",
                pair[0],
                pair[2]
            );
        }
        let mut down = Resampler::new(48_000, 44_100);
        let flat = vec![0.25_f32; 2 * 48_000];
        let out = down.process(&flat);
        assert!((44_090..=44_100).contains(&(out.len() / 2)));
        assert!(out.iter().all(|sample| (sample - 0.25).abs() < 1e-6));
    }
}
