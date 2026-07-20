//! Multi-tap delay line with seed-based tap positioning.
//!
//! Ported from CloudSeedCore MultitapDelay.h (MIT, Ghost Note Audio).
//! Supports both manual tap placement and CloudSeed's randomized
//! tap distribution with phase-randomized gains and exponential decay.

use audiocore_dsp::db::db_to_linear;
use audiocore_dsp::delay_line::DelayLine;

use super::lcg_random::random_buffer_cross_seed;

/// Maximum number of taps.
pub const MAX_TAPS: usize = 256;
/// Default capacity: 2 seconds at 48kHz. Resized by `set_sample_rate`.
const DEFAULT_SAMPLE_RATE: f64 = 48000.0;
/// Buffer length in seconds.
const BUFFER_SECONDS: f64 = 2.0;

/// A single tap with delay time (in samples) and gain.
#[derive(Clone, Copy)]
pub struct Tap {
    pub delay_samples: usize,
    pub gain: f64,
}

pub struct MultitapDelay {
    buffer: DelayLine,
    tap_gains: [f64; MAX_TAPS],
    tap_positions: [f64; MAX_TAPS],
    seed_values: Vec<f64>,
    seed: u64,
    cross_seed: f64,
    count: usize,
    length_samples: f64,
    decay: f64,
}

impl MultitapDelay {
    pub fn new(max_delay: usize) -> Self {
        let default_len = (DEFAULT_SAMPLE_RATE * BUFFER_SECONDS) as usize;
        let mut mt = Self {
            buffer: DelayLine::new((max_delay + 2).max(default_len)),
            tap_gains: [0.0; MAX_TAPS],
            tap_positions: [0.0; MAX_TAPS],
            seed_values: Vec::new(),
            seed: 0,
            cross_seed: 0.0,
            count: 1,
            length_samples: 1000.0,
            decay: 1.0,
        };
        mt.update_seeds();
        mt
    }

    /// Resize the buffer for the actual sample rate. Allocates; call from
    /// setup, never from the audio tick.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let len = (sample_rate * BUFFER_SECONDS) as usize;
        if len > self.buffer.len() {
            self.buffer = DelayLine::new(len);
        }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.update_seeds();
    }

    pub fn set_cross_seed(&mut self, cross_seed: f64) {
        self.cross_seed = cross_seed;
        self.update_seeds();
    }

    pub fn set_tap_count(&mut self, count: usize) {
        self.count = count.clamp(1, MAX_TAPS);
        self.update_taps();
    }

    pub fn set_tap_length(&mut self, length_samples: usize) {
        self.length_samples = (length_samples as f64).max(10.0);
        self.update_taps();
    }

    pub fn set_tap_decay(&mut self, decay: f64) {
        self.decay = decay;
    }

    /// Set taps manually from a slice of Tap structs (for Room/Reflections).
    pub fn set_taps(&mut self, taps: &[Tap]) {
        self.count = taps.len().min(MAX_TAPS);
        // Set length_samples = count so that length_scaler = 1.0 in tick(),
        // making tap_positions work as absolute sample offsets.
        self.length_samples = self.count as f64;
        self.decay = 0.0; // Gains are already baked into tap_gains
        for (i, t) in taps.iter().enumerate().take(MAX_TAPS) {
            self.tap_positions[i] = t.delay_samples as f64;
            self.tap_gains[i] = t.gain;
        }
    }

    /// Generate randomized taps with exponential decay (legacy API).
    pub fn set_random_taps(&mut self, count: usize, max_delay: usize, decay: f64, seed: u32) {
        self.seed = seed as u64;
        self.count = count.min(MAX_TAPS);
        self.length_samples = max_delay as f64;
        self.decay = decay;
        self.update_seeds();
    }

    /// Write a sample and return the sum of all taps.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let length_scaler = self.length_samples / self.count.max(1) as f64;
        let total_gain = 3.0 / (1.0 + self.count as f64).sqrt() * (1.0 + self.decay * 2.0);

        self.buffer.write(input);
        let max_offset = self.buffer.len() - 2;
        let mut output = 0.0;

        for j in 0..self.count {
            let offset = self.tap_positions[j] * length_scaler;
            let decay_effective =
                (-offset / self.length_samples * 3.3).exp() * self.decay + (1.0 - self.decay);
            // +1 because the read is relative to the write that just happened:
            // read(1) is the sample written this tick (offset 0 in the old code).
            let read_offset = (offset as usize).min(max_offset) + 1;
            output +=
                self.buffer.read(read_offset) * self.tap_gains[j] * decay_effective * total_gain;
        }

        output
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn reset(&mut self) {
        self.clear();
    }

    /// CloudSeed tap generation: seed-based positions with phase-randomized gains.
    fn update_taps(&mut self) {
        let mut s = 0;
        for i in 0..MAX_TAPS {
            if s + 2 < self.seed_values.len() {
                let phase = if self.seed_values[s] < 0.5 { 1.0 } else { -1.0 };
                s += 1;
                let r = self.seed_values[s];
                self.tap_gains[i] = db_to_linear(-20.0 + r * 20.0) * phase;
                s += 1;
                self.tap_positions[i] = i as f64 + self.seed_values[s];
                s += 1;
            }
        }
    }

    fn update_seeds(&mut self) {
        self.seed_values = random_buffer_cross_seed(self.seed, MAX_TAPS * 3, self.cross_seed);
        self.update_taps();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_taps_arrive_on_time() {
        let mut mt = MultitapDelay::new(48000);
        mt.set_taps(&[
            Tap {
                delay_samples: 100,
                gain: 0.5,
            },
            Tap {
                delay_samples: 250,
                gain: 0.25,
            },
        ]);

        let mut hits = Vec::new();
        for n in 0..400 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let y = mt.tick(x);
            if y.abs() > 1e-6 {
                hits.push((n, y));
            }
        }
        assert_eq!(hits.len(), 2, "two taps expected: {hits:?}");
        assert_eq!(hits[0].0, 100, "first tap timing");
        assert_eq!(hits[1].0, 250, "second tap timing");
        // total_gain scaling applies on top of the raw tap gain.
        assert!(hits[0].1 > hits[1].1, "earlier tap should be louder");
    }

    #[test]
    fn no_nan() {
        let mut mt = MultitapDelay::new(48000);
        mt.set_random_taps(64, 24000, 0.8, 1234);
        for i in 0..48000 {
            let y = mt.tick(((i as f64) * 0.1).sin());
            assert!(y.is_finite());
        }
    }
}
