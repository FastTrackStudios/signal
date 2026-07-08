//! Modulated delay line for pre-delay and late reverb lines.
//!
//! Ported from CloudSeedCore ModulatedDelay.h (MIT, Ghost Note Audio).
//! The LFO target is recomputed every 8 samples (as in CloudSeed) but the
//! fractional read position ramps per-sample between updates and the buffer
//! is read with cubic interpolation, so fast modulation stays smooth instead
//! of stepping in whole samples.

use std::f64::consts::PI;

use audiocore_dsp::delay_line::DelayLine;

/// Default capacity: 2 seconds at 48kHz. Resized by `set_sample_rate`.
const DEFAULT_SAMPLE_RATE: f64 = 48000.0;
/// Modulation recalculation rate.
const MOD_UPDATE_RATE: u64 = 8;
/// Buffer length in seconds.
const BUFFER_SECONDS: f64 = 2.0;

pub struct ModulatedDelay {
    buffer: DelayLine,
    samples_processed: u64,
    mod_phase: f64,
    current_delay: f64,
    delay_step: f64,
    pub sample_delay: usize,
    pub mod_amount: f64,
    pub mod_rate: f64,
}

impl ModulatedDelay {
    pub fn new() -> Self {
        let mut d = Self {
            buffer: DelayLine::new((DEFAULT_SAMPLE_RATE * BUFFER_SECONDS) as usize),
            samples_processed: 0,
            mod_phase: 0.31,
            current_delay: 100.0,
            delay_step: 0.0,
            sample_delay: 100,
            mod_amount: 0.0,
            mod_rate: 0.0,
        };
        d.update();
        d
    }

    /// Resize the buffer for the actual sample rate. Allocates; call from
    /// setup, never from the audio tick.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let len = (sample_rate * BUFFER_SECONDS) as usize;
        if len > self.buffer.len() {
            self.buffer = DelayLine::new(len);
        }
        self.samples_processed = 0;
        self.current_delay = self.sample_delay as f64;
        self.delay_step = 0.0;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.samples_processed >= MOD_UPDATE_RATE {
            self.update();
            self.samples_processed = 0;
        }

        // Ramp the read position toward the LFO target computed in update().
        self.current_delay += self.delay_step;

        self.buffer.write(input);
        let max_delay = (self.buffer.len() - 4) as f64;
        let output = self
            .buffer
            .read_cubic(self.current_delay.clamp(1.0, max_delay));

        self.samples_processed += 1;
        output
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn reset(&mut self) {
        self.clear();
        self.samples_processed = 0;
        self.current_delay = self.sample_delay as f64;
        self.delay_step = 0.0;
    }

    fn update(&mut self) {
        self.mod_phase += self.mod_rate * MOD_UPDATE_RATE as f64;
        if self.mod_phase > 1.0 {
            self.mod_phase %= 1.0;
        }

        let modulation = (self.mod_phase * 2.0 * PI).sin();
        let target = (self.sample_delay as f64 + self.mod_amount * modulation).max(1.0);

        // Spread the move over the next update window.
        self.delay_step = (target - self.current_delay) / MOD_UPDATE_RATE as f64;
    }
}

impl Default for ModulatedDelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_arrives_at_delay_time() {
        let mut d = ModulatedDelay::new();
        d.sample_delay = 100;
        d.reset();

        let mut arrival = None;
        for n in 0..300 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let y = d.tick(x);
            if y.abs() > 0.5 && arrival.is_none() {
                arrival = Some(n);
            }
        }
        let n = arrival.expect("impulse should come out");
        assert!(
            (n as i64 - 100).unsigned_abs() <= 2,
            "impulse should arrive near sample 100, got {n}"
        );
    }

    #[test]
    fn no_nan_under_fast_modulation() {
        let mut d = ModulatedDelay::new();
        d.sample_delay = 480;
        d.mod_amount = 400.0; // heavy
        d.mod_rate = 8.0 / 48000.0; // 8 Hz
        d.reset();

        for i in 0..96000 {
            let x = ((i as f64) * 0.05).sin();
            let y = d.tick(x);
            assert!(y.is_finite(), "NaN at {i}");
            assert!(y.abs() < 10.0, "blowup at {i}: {y}");
        }
    }

    #[test]
    fn read_position_ramps_smoothly() {
        // With modulation active, consecutive outputs of a pure tone should
        // not exhibit single-sample jumps (the old 8-sample stair-step).
        let mut d = ModulatedDelay::new();
        d.sample_delay = 1000;
        d.mod_amount = 50.0;
        d.mod_rate = 2.0 / 48000.0;
        d.reset();

        let mut prev = 0.0;
        let mut max_jump: f64 = 0.0;
        for i in 0..48000 {
            let x = (2.0 * PI * 220.0 * i as f64 / 48000.0).sin();
            let y = d.tick(x);
            if i > 2000 {
                max_jump = max_jump.max((y - prev).abs());
            }
            prev = y;
        }
        // A 220 Hz tone moves at most ~0.029/sample; modulated read adds a
        // little. The old stair-step implementation produced jumps several
        // times larger at the 8-sample boundaries.
        assert!(
            max_jump < 0.1,
            "output should be smooth, max jump {max_jump}"
        );
    }
}
