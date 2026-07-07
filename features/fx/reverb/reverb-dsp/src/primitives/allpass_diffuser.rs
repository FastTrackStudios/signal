//! Cascade of modulated allpass filters for diffusion.
//!
//! Ported from CloudSeedCore AllpassDiffuser.h (MIT, Ghost Note Audio).
//! Chains up to 12 modulated allpass stages in series with seed-based
//! delay distribution: `d = pow(10, r) * 0.1 * baseDelay`.

use super::lcg_random::random_buffer_cross_seed;
use super::modulated_allpass::ModulatedAllpass;

/// Maximum number of diffusion stages.
pub const MAX_STAGES: usize = 12;

pub struct AllpassDiffuser {
    filters: [ModulatedAllpass; MAX_STAGES],
    seed_values: Vec<f64>,
    delay: usize,
    mod_rate: f64,
    seed: u64,
    cross_seed: f64,
    sample_rate: f64,
    pub stages: usize,
}

impl AllpassDiffuser {
    pub fn new_default() -> Self {
        let filters = std::array::from_fn(|_| ModulatedAllpass::new());
        let mut d = Self {
            filters,
            seed_values: Vec::new(),
            delay: 100,
            mod_rate: 0.0,
            seed: 23456,
            cross_seed: 0.0,
            sample_rate: 48000.0,
            stages: 1,
        };
        d.update_seeds();
        d
    }

    /// Create a diffuser with delay lengths derived from seed-based distribution.
    pub fn new(delay_lengths: &[usize]) -> Self {
        let mut d = Self::new_default();
        // Set individual stage delays directly (non-seed mode)
        for (i, &len) in delay_lengths.iter().enumerate().take(MAX_STAGES) {
            d.filters[i].sample_delay = len.max(1);
        }
        d.stages = delay_lengths.len().min(MAX_STAGES);
        d
    }

    /// Create a diffuser with default delay lengths scaled by a size factor.
    pub fn with_defaults(sample_rate: f64, size: f64) -> Self {
        let mut d = Self::new_default();
        d.sample_rate = sample_rate;
        d.delay = (sample_rate * 0.01 * size.max(0.1)) as usize; // ~10ms base
        d.update_seeds();
        d.stages = 8;
        d
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.set_mod_rate(self.mod_rate);
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.update_seeds();
    }

    pub fn set_cross_seed(&mut self, cross_seed: f64) {
        self.cross_seed = cross_seed;
        self.update_seeds();
    }

    pub fn set_active_stages(&mut self, count: usize) {
        self.stages = count.min(MAX_STAGES);
    }

    pub fn set_delay(&mut self, delay_samples: usize) {
        self.delay = delay_samples;
        self.update_delays();
    }

    pub fn set_feedback(&mut self, feedback: f64) {
        for f in &mut self.filters {
            f.feedback = feedback;
        }
    }

    pub fn set_mod_amount(&mut self, amount: f64) {
        for i in 0..MAX_STAGES {
            let sv = if i + MAX_STAGES < self.seed_values.len() {
                self.seed_values[MAX_STAGES + i]
            } else {
                0.5
            };
            self.filters[i].mod_amount = amount * (0.85 + 0.3 * sv);
        }
    }

    pub fn set_mod_rate(&mut self, rate: f64) {
        self.mod_rate = rate;
        for i in 0..MAX_STAGES {
            let sv = if i + MAX_STAGES * 2 < self.seed_values.len() {
                self.seed_values[MAX_STAGES * 2 + i]
            } else {
                0.5
            };
            self.filters[i].mod_rate = rate * (0.85 + 0.3 * sv) / self.sample_rate;
        }
    }

    /// Convenience: set modulation from rate_hz, depth, and sample_rate.
    pub fn set_modulation(&mut self, rate_hz: f64, depth: f64, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.set_mod_amount(depth);
        self.set_mod_rate(rate_hz);
        let enabled = depth > 0.0;
        for f in &mut self.filters {
            f.modulation_enabled = enabled;
        }
    }

    pub fn set_modulation_enabled(&mut self, enabled: bool) {
        for f in &mut self.filters {
            f.modulation_enabled = enabled;
        }
    }

    pub fn set_interpolation_enabled(&mut self, enabled: bool) {
        for f in &mut self.filters {
            f.interpolation_enabled = enabled;
        }
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let mut x = input;
        for i in 0..self.stages {
            x = self.filters[i].tick(x);
        }
        x
    }

    pub fn clear(&mut self) {
        for f in &mut self.filters {
            f.clear();
        }
    }

    pub fn reset(&mut self) {
        self.clear();
    }

    /// CloudSeed delay distribution: `d = pow(10, r) * 0.1 * baseDelay`.
    fn update_delays(&mut self) {
        for i in 0..MAX_STAGES {
            if i < self.seed_values.len() {
                let r = self.seed_values[i];
                let d = 10.0_f64.powf(r) * 0.1; // 0.1 ... 1.0
                self.filters[i].sample_delay = ((self.delay as f64 * d) as usize).max(1);
            }
        }
    }

    fn update_seeds(&mut self) {
        self.seed_values = random_buffer_cross_seed(self.seed, MAX_STAGES * 3, self.cross_seed);
        self.update_delays();
    }
}
