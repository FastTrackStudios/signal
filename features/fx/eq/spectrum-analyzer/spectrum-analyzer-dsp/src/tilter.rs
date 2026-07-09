//! Spectral tilt around 1 kHz.
//!
//! Adds `log2(f / 1000) * slope` dB to every bin, so the displayed spectrum is
//! tilted around the 1 kHz pivot. A 4.5 dB/oct tilt roughly compensates for the
//! natural slope of music, flattening typical program material. Port of
//! ZLEqualizer's `SpectrumTilter`.

pub struct SpectrumTilter {
    shift_db: Vec<f32>,
}

impl SpectrumTilter {
    pub fn new(num_bins: usize) -> Self {
        Self {
            shift_db: vec![0.0; num_bins],
        }
    }

    pub fn resize(&mut self, num_bins: usize) {
        self.shift_db.resize(num_bins, 0.0);
    }

    /// Recompute the per-bin shift for the given sample rate and slope (dB/oct).
    pub fn set_slope(&mut self, sample_rate: f64, slope_per_oct: f64) {
        let n = self.shift_db.len();
        if n < 2 {
            return;
        }
        let delta = sample_rate * 0.5 / (n - 1) as f64;
        for i in 1..n {
            let freq = i as f64 * delta;
            self.shift_db[i] = (freq / 1000.0).log2() as f32 * slope_per_oct as f32;
        }
        self.shift_db[0] = self.shift_db[1];
    }

    /// Apply the tilt to a dB spectrum in place.
    pub fn tilt(&self, spectrum_db: &mut [f32]) {
        for (s, shift) in spectrum_db.iter_mut().zip(self.shift_db.iter()) {
            *s += *shift;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pivot_at_1khz_is_zero_octave_doubles() {
        // 8192-pt FFT @ 48 kHz: bin spacing ~5.86 Hz.
        let n = 4097;
        let sr = 48_000.0;
        let mut t = SpectrumTilter::new(n);
        t.set_slope(sr, 4.5);
        let delta = sr * 0.5 / (n as f64 - 1.0);
        let bin_1k = (1000.0 / delta).round() as usize;
        let bin_2k = (2000.0 / delta).round() as usize;
        // ~0 dB at 1 kHz, ~+4.5 dB one octave up.
        assert!(
            t.shift_db[bin_1k].abs() < 0.1,
            "1k shift {}",
            t.shift_db[bin_1k]
        );
        assert!(
            (t.shift_db[bin_2k] - 4.5).abs() < 0.1,
            "2k shift {}",
            t.shift_db[bin_2k]
        );
    }
}
