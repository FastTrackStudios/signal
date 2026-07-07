//! Constant-octave display smoothing.
//!
//! For each bin `i`, average the bins spanning ±half-the-smoothing-octave around
//! it. Window edges are precomputed; the average itself is a prefix-sum boxcar
//! applied twice (a triangular smoothing). Port of ZLEqualizer's
//! `SpectrumSmoother`.

pub struct SpectrumSmoother {
    low_idx: Vec<usize>,
    high_idx: Vec<usize>,
    inv_count: Vec<f32>,
    prefix: Vec<f64>,
    enabled: bool,
}

impl SpectrumSmoother {
    pub fn new(num_bins: usize) -> Self {
        let mut s = Self {
            low_idx: vec![0; num_bins],
            high_idx: vec![0; num_bins],
            inv_count: vec![1.0; num_bins],
            prefix: vec![0.0; num_bins + 1],
            enabled: false,
        };
        s.set_smooth(0.0);
        s
    }

    pub fn resize(&mut self, num_bins: usize) {
        self.low_idx.resize(num_bins, 0);
        self.high_idx.resize(num_bins, 0);
        self.inv_count.resize(num_bins, 1.0);
        self.prefix.resize(num_bins + 1, 0.0);
    }

    /// Configure the smoothing width in octaves (0 disables smoothing).
    pub fn set_smooth(&mut self, smooth_oct: f64) {
        self.enabled = smooth_oct > 0.0;
        if !self.enabled {
            return;
        }
        let factor = 2.0_f64.powf(smooth_oct / 2.0);
        let inv_factor = 1.0 / factor;
        let max_idx = self.low_idx.len().saturating_sub(1);
        for i in 0..self.low_idx.len() {
            let lower = i as f64 * inv_factor;
            let upper = i as f64 * factor;
            let lo = lower.round() as usize;
            let hi = ((upper.round() + 1.0) as usize).min(max_idx);
            let hi = hi.max(lo + 1).min(max_idx.max(1));
            self.low_idx[i] = lo;
            self.high_idx[i] = hi;
            self.inv_count[i] = 1.0 / (hi - lo) as f32;
        }
    }

    /// Smooth `data` (squared magnitudes) in place.
    pub fn smooth(&mut self, data: &mut [f32]) {
        if !self.enabled {
            return;
        }
        debug_assert_eq!(data.len(), self.low_idx.len());
        self.boxcar(data);
        self.boxcar(data);
    }

    // Indices address several parallel arrays (prefix / low_idx / high_idx),
    // so a range loop is the clearest form here.
    #[allow(clippy::needless_range_loop)]
    fn boxcar(&mut self, data: &mut [f32]) {
        self.prefix[0] = 0.0;
        for i in 0..data.len() {
            self.prefix[i + 1] = self.prefix[i] + data[i] as f64;
        }
        for i in 0..data.len() {
            let sum = self.prefix[self.high_idx[i]] - self.prefix[self.low_idx[i]];
            data[i] = sum as f32 * self.inv_count[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_stays_flat() {
        let mut s = SpectrumSmoother::new(64);
        s.set_smooth(1.0);
        let mut data = vec![5.0f32; 64];
        s.smooth(&mut data);
        for v in &data {
            assert!((v - 5.0).abs() < 1e-3, "got {v}");
        }
    }

    #[test]
    fn disabled_is_identity() {
        let mut s = SpectrumSmoother::new(8);
        let mut data = [1.0, 7.0, 2.0, 9.0, 0.0, 3.0, 4.0, 5.0];
        let orig = data;
        s.smooth(&mut data);
        assert_eq!(data, orig);
    }
}
