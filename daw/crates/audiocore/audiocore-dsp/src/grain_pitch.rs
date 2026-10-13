//! Dual-head granular pitch shifter.
//!
//! Two read heads drift through a delay buffer at `1 - speed` samples per
//! sample, half a grain apart, crossfaded with a raised-cosine window so
//! one head is always silent when it jumps. Designed for feedback paths
//! (shimmer reverb/delay, chorale) — low latency, no FFT, no allocation
//! after construction.

use std::f64::consts::PI;

use crate::delay_line::DelayLine;

pub struct GrainPitchShifter {
    buffer: DelayLine,
    grain_size: f64, // in samples
    offset_a: f64,
    offset_b: f64,
    speed: f64, // 2.0 = octave up, 0.5 = octave down, 1.0 = bypass
}

impl GrainPitchShifter {
    /// `max_grain_samples` bounds the grain size (and the buffer allocation).
    pub fn new(max_grain_samples: usize) -> Self {
        let grain = max_grain_samples as f64;
        Self {
            buffer: DelayLine::new(max_grain_samples + 4),
            grain_size: grain,
            // Quarter/three-quarter start keeps the heads half a grain
            // apart with window gains summing to 1, so unity speed
            // (stationary heads) still passes signal.
            offset_a: grain * 0.25,
            offset_b: grain * 0.75,
            speed: 1.0,
        }
    }

    /// Pitch ratio: 2.0 = octave up, 1.5 = fifth up, 0.5 = octave down.
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }

    /// Grain window in samples; clamped to the allocated buffer.
    /// Re-seats the read heads, so call at setup, not per-sample.
    pub fn set_grain_samples(&mut self, grain: f64) {
        self.grain_size = grain.clamp(64.0, (self.buffer.len() - 4) as f64);
        self.offset_a = self.grain_size * 0.25;
        self.offset_b = self.grain_size * 0.75;
    }

    pub fn set_grain_ms(&mut self, grain_ms: f64, sample_rate: f64) {
        self.set_grain_samples(grain_ms * 0.001 * sample_rate);
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        self.buffer.write(input);

        // Heads drift backward/forward depending on ratio.
        let drift = 1.0 - self.speed;
        self.offset_a += drift;
        self.offset_b += drift;

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

        let a = self.buffer.read_cubic(self.offset_a.max(1.0));
        let b = self.buffer.read_cubic(self.offset_b.max(1.0));

        // Raised-cosine (sin^2) crossfade over head position in the grain.
        let fade_a = (self.offset_a / self.grain_size * PI).sin();
        let fade_b = (self.offset_b / self.grain_size * PI).sin();
        let gain_a = fade_a * fade_a;
        let gain_b = fade_b * fade_b;
        let norm = (gain_a + gain_b).max(0.001);

        (a * gain_a + b * gain_b) / norm
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.offset_a = self.grain_size * 0.25;
        self.offset_b = self.grain_size * 0.75;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Estimate output frequency by counting zero crossings.
    fn measure_ratio(speed: f64) -> f64 {
        let sr = 48000.0;
        let freq = 440.0;
        let mut ps = GrainPitchShifter::new(4800);
        ps.set_grain_samples(2400.0);
        ps.set_speed(speed);

        let mut crossings = 0u32;
        let mut prev = 0.0f64;
        let n = 48000;
        for i in 0..n * 2 {
            let x = (2.0 * PI * freq * i as f64 / sr).sin();
            let y = ps.tick(x);
            if i >= n {
                if prev <= 0.0 && y > 0.0 {
                    crossings += 1;
                }
                prev = y;
            }
        }
        let measured = crossings as f64; // Hz over exactly 1 second
        measured / freq
    }

    #[test]
    fn octave_up_doubles_frequency() {
        let r = measure_ratio(2.0);
        assert!((r - 2.0).abs() < 0.15, "octave up ratio: {r}");
    }

    #[test]
    fn octave_down_halves_frequency() {
        let r = measure_ratio(0.5);
        assert!((r - 0.5).abs() < 0.1, "octave down ratio: {r}");
    }

    #[test]
    fn unity_speed_passes_pitch() {
        let r = measure_ratio(1.0);
        assert!((r - 1.0).abs() < 0.05, "unity ratio: {r}");
    }

    #[test]
    fn no_nan_and_bounded() {
        let mut ps = GrainPitchShifter::new(4800);
        ps.set_speed(2.0);
        for i in 0..96000 {
            let x = ((i as f64) * 0.1).sin();
            let y = ps.tick(x);
            assert!(y.is_finite());
            assert!(y.abs() < 4.0, "unexpected gain blowup: {y}");
        }
    }
}
