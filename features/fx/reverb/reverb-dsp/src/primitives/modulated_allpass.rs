//! Modulated allpass filter.
//!
//! Ported from CloudSeedCore ModulatedAllpass.h (MIT, Ghost Note Audio).
//! The LFO target updates every 8 samples (as in CloudSeed) but the
//! fractional delay ramps per-sample between updates and the buffer is
//! read with cubic interpolation — no more whole-sample stair-stepping.

use std::f64::consts::PI;

use audiocore_dsp::delay_line::DelayLine;

/// Default capacity: 100ms at 48kHz. Resized by `set_sample_rate`.
const DEFAULT_SAMPLE_RATE: f64 = 48000.0;
/// Modulation recalculation rate.
const MOD_UPDATE_RATE: u64 = 8;
/// Buffer length in seconds (100ms covers CloudSeed diffuser delays).
const BUFFER_SECONDS: f64 = 0.1;

pub struct ModulatedAllpass {
    buffer: DelayLine,
    samples_processed: u64,

    // Modulation state
    mod_phase: f64,
    current_delay: f64,
    delay_step: f64,

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
            buffer: DelayLine::new((DEFAULT_SAMPLE_RATE * BUFFER_SECONDS) as usize),
            samples_processed: 0,
            mod_phase: 0.31, // Arbitrary initial phase
            current_delay: 100.0,
            delay_step: 0.0,
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

    /// Resize the buffer for the actual sample rate. Allocates; call from
    /// setup, never from the audio tick.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let len = ((sample_rate * BUFFER_SECONDS) as usize).max(256);
        if len > self.buffer.len() {
            self.buffer = DelayLine::new(len);
        }
        self.samples_processed = 0;
        self.current_delay = self.sample_delay as f64;
        self.delay_step = 0.0;
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
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
        let delay = self.sample_delay.min(self.buffer.len() - 2).max(1);
        let buf_out = self.buffer.read(delay);
        self.allpass_step(input, buf_out)
    }

    #[inline]
    fn tick_with_mod(&mut self, input: f64) -> f64 {
        if self.samples_processed >= MOD_UPDATE_RATE {
            self.update_mod();
            self.samples_processed = 0;
        }

        self.current_delay += self.delay_step;
        let max_delay = (self.buffer.len() - 4) as f64;
        let pos = self.current_delay.clamp(1.0, max_delay);

        let buf_out = if self.interpolation_enabled {
            self.buffer.read_cubic(pos)
        } else {
            self.buffer.read(pos as usize)
        };

        self.samples_processed += 1;
        self.allpass_step(input, buf_out)
    }

    /// Shared Schroeder allpass update: write `input + fb`, output `delayed - fb * written`.
    #[inline]
    fn allpass_step(&mut self, input: f64, buf_out: f64) -> f64 {
        let in_val = input + buf_out * self.feedback;
        self.buffer.write(in_val);
        buf_out - in_val * self.feedback
    }

    fn update_mod(&mut self) {
        self.mod_phase += self.mod_rate * MOD_UPDATE_RATE as f64;
        if self.mod_phase > 1.0 {
            self.mod_phase %= 1.0;
        }

        let modulation = (self.mod_phase * 2.0 * PI).sin();

        // Prevent modulation from taking delay negative
        let effective_mod = self.mod_amount.min((self.sample_delay as f64) - 1.0);
        let target = (self.sample_delay as f64 + effective_mod * modulation).max(1.0);

        // Spread the move over the next update window.
        self.delay_step = (target - self.current_delay) / MOD_UPDATE_RATE as f64;
    }

    /// Convenience: set feedback coefficient.
    pub fn set_feedback(&mut self, g: f64) {
        self.feedback = g;
    }

    /// Convenience: set delay in samples.
    pub fn set_delay(&mut self, samples: f64) {
        self.sample_delay = (samples as usize).min(self.buffer.len() - 2).max(1);
    }

    /// Convenience: set delay in integer samples.
    pub fn set_delay_samples(&mut self, samples: usize) {
        self.sample_delay = samples.min(self.buffer.len() - 2).max(1);
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
        self.current_delay = self.sample_delay as f64;
        self.delay_step = 0.0;
    }
}

impl Default for ModulatedAllpass {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allpass_is_stable_and_finite() {
        let mut ap = ModulatedAllpass::new();
        ap.set_delay_samples(223);
        ap.set_feedback(0.7);
        ap.set_modulation(1.5, 30.0, 48000.0);

        for i in 0..96000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let y = ap.tick(x);
            assert!(y.is_finite(), "NaN at {i}");
            assert!(y.abs() < 10.0, "blowup at {i}: {y}");
        }
    }

    #[test]
    fn no_mod_path_matches_schroeder_impulse() {
        let mut ap = ModulatedAllpass::new();
        ap.modulation_enabled = false;
        ap.set_delay_samples(50);
        ap.set_feedback(0.5);

        // First output of a unit impulse through a Schroeder allpass is -g.
        let y0 = ap.tick(1.0);
        assert!((y0 - (-0.5)).abs() < 1e-12, "direct path should be -g: {y0}");

        // At the delay time, output is 1 - g^2.
        let mut y_delay = 0.0;
        for n in 1..=50 {
            y_delay = ap.tick(0.0);
            if n < 50 {
                assert!(y_delay.abs() < 1e-12, "silent before delay, sample {n}");
            }
        }
        assert!(
            (y_delay - 0.75).abs() < 1e-12,
            "1 - g^2 expected at delay time: {y_delay}"
        );
    }
}
