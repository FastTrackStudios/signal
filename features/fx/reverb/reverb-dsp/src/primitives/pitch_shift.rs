//! Simplified pitch shifter for reverb feedback paths (shimmer/chorale).
//!
//! Dual-head grain-based shifter optimized for octave shifts in
//! feedback loops. Not a full pitch processor — just enough for
//! shimmer and chorale effects.

use audiocore_dsp::delay_line::DelayLine;

pub struct PitchShifter {
    buffer: DelayLine,
    grain_size: f64, // In samples
    offset_a: f64,
    offset_b: f64,
    speed: f64, // 2.0 = octave up, 0.5 = octave down
}

impl PitchShifter {
    pub fn new(max_grain_samples: usize) -> Self {
        Self {
            buffer: DelayLine::new(max_grain_samples + 4),
            grain_size: max_grain_samples as f64,
            offset_a: 0.0,
            offset_b: 0.0,
            speed: 2.0,
        }
    }

    /// Set pitch ratio (2.0 = octave up, 0.5 = octave down, 1.5 = fifth up).
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }

    /// Set grain size in milliseconds.
    pub fn set_grain_ms(&mut self, ms: f64, sample_rate: f64) {
        self.grain_size = (ms * 0.001 * sample_rate)
            .max(64.0)
            .min((self.buffer.len() - 4) as f64);
        // Initialize offset_b at half the grain
        self.offset_b = self.grain_size * 0.5;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.buffer.write(input);

        let drift = 1.0 - self.speed;

        // Advance both read heads
        self.offset_a += drift;
        self.offset_b += drift;

        // Wrap heads within grain window
        if self.offset_a < 0.0 {
            self.offset_a += self.grain_size;
        } else if self.offset_a >= self.grain_size {
            self.offset_a -= self.grain_size;
        }
        if self.offset_b < 0.0 {
            self.offset_b += self.grain_size;
        } else if self.offset_b >= self.grain_size {
            self.offset_b -= self.grain_size;
        }

        // Read from both heads
        let a = self.buffer.read_linear(self.offset_a.max(1.0));
        let b = self.buffer.read_linear(self.offset_b.max(1.0));

        // Crossfade: raised cosine based on position within grain
        let fade_a = (self.offset_a / self.grain_size * std::f64::consts::PI).sin();
        let fade_b = (self.offset_b / self.grain_size * std::f64::consts::PI).sin();

        let gain_a = fade_a * fade_a;
        let gain_b = fade_b * fade_b;
        let norm = (gain_a + gain_b).max(0.001);

        (a * gain_a + b * gain_b) / norm
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.offset_a = 0.0;
        self.offset_b = self.grain_size * 0.5;
    }
}
