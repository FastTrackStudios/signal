//! Modulated allpass filter.
//!
//! Ported from CloudSeedCore ModulatedAllpass.h (MIT, Ghost Note Audio).
//! Fixed-size delay buffer with sinusoidal delay-time modulation.
//! Modulation parameters update every 8 samples for efficiency.

use std::f64::consts::PI;

/// Buffer size: 100ms at 192kHz.
const DELAY_BUFFER_SIZE: usize = 19200;
/// Modulation recalculation rate.
const MOD_UPDATE_RATE: u64 = 8;

pub struct ModulatedAllpass {
    buffer: Vec<f64>,
    index: usize,
    samples_processed: u64,

    // Modulation state
    mod_phase: f64,
    delay_a: usize,
    delay_b: usize,
    gain_a: f64,
    gain_b: f64,

    // Parameters
    pub sample_delay: usize,
    pub feedback: f64,
    pub mod_amount: f64,
    pub mod_rate: f64,
    pub interpolation_enabled: bool,
    pub modulation_enabled: bool,
}

impl ModulatedAllpass {
    pub fn new() -> Self {
        let mut ap = Self {
            buffer: vec![0.0; DELAY_BUFFER_SIZE],
            index: DELAY_BUFFER_SIZE - 1,
            samples_processed: 0,
            mod_phase: 0.31, // Arbitrary initial phase
            delay_a: 0,
            delay_b: 0,
            gain_a: 0.0,
            gain_b: 0.0,
            sample_delay: 100,
            feedback: 0.5,
            mod_amount: 0.0,
            mod_rate: 0.0,
            interpolation_enabled: true,
            modulation_enabled: true,
        };
        ap.update_mod();
        ap
    }

    /// Create with a specific initial phase for stereo decorrelation.
    pub fn with_phase(phase: f64) -> Self {
        let mut ap = Self::new();
        ap.mod_phase = phase;
        ap
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.modulation_enabled {
            self.tick_with_mod(input)
        } else {
            self.tick_no_mod(input)
        }
    }

    #[inline]
    fn tick_no_mod(&mut self, input: f64) -> f64 {
        let mut delayed_idx = self.index as isize - self.sample_delay as isize;
        if delayed_idx < 0 {
            delayed_idx += DELAY_BUFFER_SIZE as isize;
        }

        let buf_out = self.buffer[delayed_idx as usize];
        let in_val = input + buf_out * self.feedback;
        self.buffer[self.index] = in_val;
        let output = buf_out - in_val * self.feedback;

        self.index += 1;
        if self.index >= DELAY_BUFFER_SIZE {
            self.index -= DELAY_BUFFER_SIZE;
        }
        self.samples_processed += 1;

        output
    }

    #[inline]
    fn tick_with_mod(&mut self, input: f64) -> f64 {
        if self.samples_processed >= MOD_UPDATE_RATE {
            self.update_mod();
            self.samples_processed = 0;
        }

        let buf_out = if self.interpolation_enabled {
            let mut idx_a = self.index as isize - self.delay_a as isize;
            let mut idx_b = self.index as isize - self.delay_b as isize;
            if idx_a < 0 {
                idx_a += DELAY_BUFFER_SIZE as isize;
            }
            if idx_b < 0 {
                idx_b += DELAY_BUFFER_SIZE as isize;
            }
            self.buffer[idx_a as usize] * self.gain_a + self.buffer[idx_b as usize] * self.gain_b
        } else {
            let mut idx_a = self.index as isize - self.delay_a as isize;
            if idx_a < 0 {
                idx_a += DELAY_BUFFER_SIZE as isize;
            }
            self.buffer[idx_a as usize]
        };

        let in_val = input + buf_out * self.feedback;
        self.buffer[self.index] = in_val;
        let output = buf_out - in_val * self.feedback;

        self.index += 1;
        if self.index >= DELAY_BUFFER_SIZE {
            self.index -= DELAY_BUFFER_SIZE;
        }
        self.samples_processed += 1;

        output
    }

    fn update_mod(&mut self) {
        self.mod_phase += self.mod_rate * MOD_UPDATE_RATE as f64;
        if self.mod_phase > 1.0 {
            self.mod_phase %= 1.0;
        }

        let modulation = (self.mod_phase * 2.0 * PI).sin();

        // Prevent modulation from taking delay negative
        let effective_mod = self.mod_amount.min((self.sample_delay as f64) - 1.0);
        let total_delay = self.sample_delay as f64 + effective_mod * modulation;
        let total_delay = total_delay.max(1.0);

        self.delay_a = total_delay as usize;
        self.delay_b = self.delay_a + 1;

        let partial = total_delay - self.delay_a as f64;
        self.gain_a = 1.0 - partial;
        self.gain_b = partial;
    }

    /// Convenience: set feedback coefficient.
    pub fn set_feedback(&mut self, g: f64) {
        self.feedback = g;
    }

    /// Convenience: set delay in samples.
    pub fn set_delay(&mut self, samples: f64) {
        self.sample_delay = (samples as usize).min(DELAY_BUFFER_SIZE - 2).max(1);
    }

    /// Convenience: set delay in integer samples.
    pub fn set_delay_samples(&mut self, samples: usize) {
        self.sample_delay = samples.min(DELAY_BUFFER_SIZE - 2).max(1);
    }

    /// Convenience: set modulation rate and depth.
    /// `rate_hz` is the modulation frequency, `depth` is in samples,
    /// `sample_rate` is used to normalize rate.
    pub fn set_modulation(&mut self, rate_hz: f64, depth: f64, sample_rate: f64) {
        self.mod_rate = rate_hz / sample_rate;
        self.mod_amount = depth;
        self.modulation_enabled = depth > 0.0;
    }

    /// Convenience: set modulation phase (0.0 to 1.0).
    pub fn set_phase(&mut self, phase: f64) {
        self.mod_phase = phase;
    }

    pub fn reset(&mut self) {
        self.clear();
        self.samples_processed = 0;
    }
}
