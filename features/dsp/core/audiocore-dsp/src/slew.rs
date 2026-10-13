//! Slew rate limiting — from Airwindows Loud, ToTape8 bias stage.

// r[impl dsp.slew.single]
// r[impl dsp.slew.stereo]
/// Single-stage slew rate limiter.
pub struct SlewLimiter {
    last: [f64; 2],
    threshold: f64,
}

impl SlewLimiter {
    pub fn new(threshold: f64) -> Self {
        Self {
            last: [0.0; 2],
            threshold,
        }
    }

    pub fn set_threshold(&mut self, threshold: f64) {
        self.threshold = threshold;
    }

    #[inline]
    pub fn tick(&mut self, sample: f64, ch: usize) -> f64 {
        let delta = sample - self.last[ch];
        let limited = if delta > self.threshold {
            self.last[ch] + self.threshold
        } else if delta < -self.threshold {
            self.last[ch] - self.threshold
        } else {
            sample
        };
        self.last[ch] = limited;
        limited
    }

    pub fn reset(&mut self) {
        self.last = [0.0; 2];
    }
}

// r[impl dsp.slew.golden-chain]
/// Multi-stage golden-ratio-spaced slew limiter chain (from ToTape8 bias).
pub struct GoldenSlewChain {
    stages: Vec<SlewLimiter>,
}

impl GoldenSlewChain {
    /// Create a chain with `num_stages` slew limiters spaced by golden ratio.
    pub fn new(num_stages: usize, base_threshold: f64) -> Self {
        let phi: f64 = 1.618033988749894;
        let stages = (0..num_stages)
            .map(|i| SlewLimiter::new(base_threshold * phi.powi(i as i32)))
            .collect();
        Self { stages }
    }

    pub fn set_base_threshold(&mut self, base_threshold: f64) {
        let phi: f64 = 1.618033988749894;
        for (i, stage) in self.stages.iter_mut().enumerate() {
            stage.set_threshold(base_threshold * phi.powi(i as i32));
        }
    }

    #[inline]
    pub fn tick(&mut self, mut sample: f64, ch: usize) -> f64 {
        for stage in &mut self.stages {
            sample = stage.tick(sample, ch);
        }
        sample
    }

    pub fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }
}
