//! Running mean of squared-magnitude frames.
//!
//! When several FFT frames are available in one UI tick, their power spectra are
//! averaged so the displayed spectrum is the mean over the tick rather than just
//! the last frame. Port of ZLEqualizer's `SpectrumAccumulator`.

/// Accumulates squared-magnitude spectra and reports their running mean in place.
pub struct SpectrumAccumulator {
    sums: Vec<f64>,
    count: u64,
}

impl SpectrumAccumulator {
    pub fn new(num_bins: usize) -> Self {
        Self {
            sums: vec![0.0; num_bins],
            count: 0,
        }
    }

    pub fn resize(&mut self, num_bins: usize) {
        self.sums.resize(num_bins, 0.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.count = 0;
        self.sums.iter_mut().for_each(|s| *s = 0.0);
    }

    /// Add `frame` (squared magnitudes) and overwrite it with the running mean.
    pub fn process(&mut self, frame: &mut [f32]) {
        debug_assert_eq!(frame.len(), self.sums.len());
        self.count += 1;
        let mul = 1.0 / self.count as f64;
        for (sum, x) in self.sums.iter_mut().zip(frame.iter_mut()) {
            *sum += *x as f64;
            *x = (*sum * mul) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_mean_of_constant_converges() {
        let mut acc = SpectrumAccumulator::new(4);
        for _ in 0..10 {
            let mut frame = [2.0f32; 4];
            acc.process(&mut frame);
            assert!((frame[0] - 2.0).abs() < 1e-6);
        }
    }
}
