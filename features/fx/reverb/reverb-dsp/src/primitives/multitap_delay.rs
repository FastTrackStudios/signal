//! Multi-tap delay line with seed-based tap positioning.
//!
//! Ported from CloudSeedCore MultitapDelay.h (MIT, Ghost Note Audio).
//! Supports both manual tap placement and CloudSeed's randomized
//! tap distribution with phase-randomized gains and exponential decay.

use super::lcg_random::random_buffer_cross_seed;

/// Maximum number of taps.
pub const MAX_TAPS: usize = 256;
/// Buffer size: 2 seconds at 192kHz.
const DELAY_BUFFER_SIZE: usize = 384000;

/// A single tap with delay time (in samples) and gain.
#[derive(Clone, Copy)]
pub struct Tap {
    pub delay_samples: usize,
    pub gain: f64,
}

pub struct MultitapDelay {
    buffer: Vec<f64>,
    write_idx: usize,
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
    pub fn new(_max_delay: usize) -> Self {
        let mut mt = Self {
            buffer: vec![0.0; DELAY_BUFFER_SIZE],
            write_idx: 0,
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

    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.update_seeds();
    }

    pub fn set_cross_seed(&mut self, cross_seed: f64) {
        self.cross_seed = cross_seed;
        self.update_seeds();
    }

    pub fn set_tap_count(&mut self, count: usize) {
        self.count = count.max(1).min(MAX_TAPS);
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

        self.buffer[self.write_idx] = input;
        let mut output = 0.0;

        for j in 0..self.count {
            let offset = self.tap_positions[j] * length_scaler;
            let decay_effective =
                (-offset / self.length_samples * 3.3).exp() * self.decay + (1.0 - self.decay);
            let offset_int = (offset as usize).min(DELAY_BUFFER_SIZE - 1);
            let read_idx = self.write_idx as isize - offset_int as isize;
            let read_idx = if read_idx < 0 {
                (read_idx + DELAY_BUFFER_SIZE as isize) as usize
            } else {
                read_idx as usize
            };
            output += self.buffer[read_idx] * self.tap_gains[j] * decay_effective * total_gain;
        }

        self.write_idx = (self.write_idx + 1) % DELAY_BUFFER_SIZE;
        output
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
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
                self.tap_gains[i] = db2gain(-20.0 + r * 20.0) * phase;
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

fn db2gain(db: f64) -> f64 {
    10.0_f64.powf(db * 0.05)
}
