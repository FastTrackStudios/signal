//! True inter-sample peak detection (ITU-R BS.1770 / EBU R128).
//!
//! Uses 4× oversampling via Catmull-Rom cubic interpolation to estimate
//! inter-sample peaks that a D/A converter would produce.

/// True inter-sample peak detector for one channel.
///
/// Maintains the running maximum across all processed blocks.
/// Call [`TruePeakDetector::process`] per block; read the peak from
/// [`TruePeakDetector::peak`].
///
/// `peak` is stored as a linear amplitude value (not dBFS).
/// Convert to dBFS with `20 * peak.log10()`.
pub struct TruePeakDetector {
    /// Running peak (linear amplitude). Reset via [`TruePeakDetector::reset`].
    pub peak: f32,
    /// Last two samples from the previous block (needed for continuity).
    prev: [f32; 2],
}

impl TruePeakDetector {
    /// Create a new, zeroed detector.
    pub fn new() -> Self {
        Self {
            peak: 0.0,
            prev: [0.0; 2],
        }
    }

    /// Process a mono block and return the block-level inter-sample peak.
    ///
    /// Also updates [`TruePeakDetector::peak`] with the running maximum.
    pub fn process(&mut self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return self.peak;
        }

        // Build a temporary slice that prepends two previous samples so
        // interpolation works at the seam between blocks.
        let mut block_peak = 0.0_f32;

        // We need at least 4 points for Catmull-Rom: s0=prev[-2], s1=prev[-1],
        // s2=samples[i], s3=samples[i+1].
        // Iterate over all original sample positions
        for i in 0..samples.len() {
            // Gather 4 points (extended index: i+2 corresponds to samples[i])
            let ext_idx = i + 2;
            let s0 = self.get_extended(ext_idx.wrapping_sub(1), samples);
            let s1 = self.get_extended(ext_idx, samples);
            let s2 = self.get_extended(ext_idx + 1, samples);
            let s3 = self.get_extended(ext_idx + 2, samples);

            // Check 3 inter-sample points at t = 0.25, 0.5, 0.75
            for &t in &[0.25_f64, 0.5, 0.75] {
                let v = catmull_rom(s0 as f64, s1 as f64, s2 as f64, s3 as f64, t);
                block_peak = block_peak.max(v.abs() as f32);
            }
            block_peak = block_peak.max(s1.abs());
        }

        // Update prev buffer with the last two samples
        let n = samples.len();
        self.prev[0] = if n >= 2 { samples[n - 2] } else { self.prev[0] };
        self.prev[1] = *samples.last().unwrap();

        self.peak = self.peak.max(block_peak);
        block_peak
    }

    /// Get a sample from the extended (prev ++ samples) view.
    fn get_extended(&self, ext_idx: usize, samples: &[f32]) -> f32 {
        match ext_idx {
            0 => self.prev[0],
            1 => self.prev[1],
            i => {
                let si = i - 2;
                if si < samples.len() {
                    samples[si]
                } else {
                    // Clamp to last sample
                    *samples.last().unwrap_or(&0.0)
                }
            }
        }
    }

    /// Reset the running peak and history.
    pub fn reset(&mut self) {
        self.peak = 0.0;
        self.prev = [0.0; 2];
    }

    /// Running peak in dBFS (returns `f32::NEG_INFINITY` if no signal).
    pub fn peak_dbfs(&self) -> f32 {
        if self.peak > 0.0 {
            20.0 * self.peak.log10()
        } else {
            f32::NEG_INFINITY
        }
    }
}

impl Default for TruePeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Catmull-Rom cubic interpolation between s1 and s2, at parameter t ∈ [0, 1].
#[inline]
fn catmull_rom(s0: f64, s1: f64, s2: f64, s3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * s1)
        + (-s0 + s2) * t
        + (2.0 * s0 - 5.0 * s1 + 4.0 * s2 - s3) * t2
        + (-s0 + 3.0 * s1 - 3.0 * s2 + s3) * t3)
}
