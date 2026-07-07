//! Modulated delay line for pre-delay and late reverb lines.
//!
//! Ported from CloudSeedCore ModulatedDelay.h (MIT, Ghost Note Audio).
//! Large buffer (2 seconds at 192kHz) with sinusoidal delay-time modulation.
//! Modulation parameters update every 8 samples for efficiency.

use std::f64::consts::PI;

/// Buffer size: 2 seconds at 192kHz.
const DELAY_BUFFER_SIZE: usize = 384000;
/// Modulation recalculation rate.
const MOD_UPDATE_RATE: u64 = 8;

pub struct ModulatedDelay {
    buffer: Vec<f64>,
    write_index: usize,
    read_index_a: usize,
    read_index_b: usize,
    samples_processed: u64,
    mod_phase: f64,
    gain_a: f64,
    gain_b: f64,
    pub sample_delay: usize,
    pub mod_amount: f64,
    pub mod_rate: f64,
}

impl ModulatedDelay {
    pub fn new() -> Self {
        let mut d = Self {
            buffer: vec![0.0; DELAY_BUFFER_SIZE],
            write_index: 0,
            read_index_a: 0,
            read_index_b: 0,
            samples_processed: 0,
            mod_phase: 0.31,
            gain_a: 0.0,
            gain_b: 0.0,
            sample_delay: 100,
            mod_amount: 0.0,
            mod_rate: 0.0,
        };
        d.update();
        d
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.samples_processed >= MOD_UPDATE_RATE {
            self.update();
            self.samples_processed = 0;
        }

        self.buffer[self.write_index] = input;
        let output = self.buffer[self.read_index_a] * self.gain_a
            + self.buffer[self.read_index_b] * self.gain_b;

        self.write_index = (self.write_index + 1) % DELAY_BUFFER_SIZE;
        self.read_index_a = (self.read_index_a + 1) % DELAY_BUFFER_SIZE;
        self.read_index_b = (self.read_index_b + 1) % DELAY_BUFFER_SIZE;
        self.samples_processed += 1;

        output
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
    }

    pub fn reset(&mut self) {
        self.clear();
        self.samples_processed = 0;
    }

    fn update(&mut self) {
        self.mod_phase += self.mod_rate * MOD_UPDATE_RATE as f64;
        if self.mod_phase > 1.0 {
            self.mod_phase %= 1.0;
        }

        let modulation = (self.mod_phase * 2.0 * PI).sin();
        let total_delay = self.sample_delay as f64 + self.mod_amount * modulation;
        let total_delay = total_delay.max(1.0);

        let delay_a = total_delay as usize;
        let delay_b = delay_a + 1;

        let partial = total_delay - delay_a as f64;
        self.gain_a = 1.0 - partial;
        self.gain_b = partial;

        let wi = self.write_index as isize;
        let mut ra = wi - delay_a as isize;
        let mut rb = wi - delay_b as isize;
        if ra < 0 {
            ra += DELAY_BUFFER_SIZE as isize;
        }
        if rb < 0 {
            rb += DELAY_BUFFER_SIZE as isize;
        }
        self.read_index_a = ra as usize;
        self.read_index_b = rb as usize;
    }
}
