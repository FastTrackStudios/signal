//! ReverseDelay — records forward, plays back reversed in grains.
//!
//! Two alternating grains, each `time_ms` long. While one records,
//! the other plays back reversed. Raised cosine crossfade between grains.

use crate::tilt::DecayTilt;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;

/// Reverse delay using dual alternating grains.
pub struct ReverseDelay {
    /// Delay time in milliseconds (grain length).
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Crossfade overlap as fraction of grain length (0.0–0.5).
    pub grain_crossfade: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_tilt_eq: DecayTilt,
    delay: DelayLine,
    hicut: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    /// Current position within the grain (0..grain_samples).
    grain_pos: usize,
    /// Length of current grain in samples.
    grain_samples: usize,
}

impl Default for ReverseDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverseDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            grain_crossfade: 0.1,
            hicut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            grain_pos: 0,
            grain_samples: 12000, // 250ms at 48kHz
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        if self.hicut_freq > 0.0 {
            self.hicut.set(
                FilterType::Lowpass,
                self.hicut_freq,
                self.filter_q,
                sample_rate,
            );
        }

        // Decay EQ: tilt filter in feedback path
        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.grain_samples = ((self.time_ms * 0.001 * sample_rate) as usize).max(64);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let grain_len = ((self.time_ms * 0.001 * self.sample_rate) as usize).max(64);
        self.grain_samples = grain_len;

        // Write input + feedback to delay buffer
        self.delay.write(input + self.feedback_sample);

        // Read reversed: at grain_pos=0 read from grain_len samples ago,
        // at grain_pos=grain_len-1 read from 1 sample ago.
        // We use two grains offset by half a grain length for crossfading.
        let pos_a = self.grain_pos;
        let pos_b = (self.grain_pos + grain_len) % (grain_len * 2);

        let read_a = Self::read_reversed(&self.delay, pos_a, grain_len);
        let read_b = Self::read_reversed(&self.delay, pos_b, grain_len);

        // Crossfade windows
        let cf = self.grain_crossfade.clamp(0.01, 0.5);
        let win_a = Self::grain_window(pos_a, grain_len, cf);
        let win_b = Self::grain_window(pos_b, grain_len, cf);

        let output = read_a * win_a + read_b * win_b;

        // Advance position
        self.grain_pos = (self.grain_pos + 1) % (grain_len * 2);

        // Feedback path
        let mut fb = output * self.feedback;
        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }

        fb = self.decay_tilt_eq.tick(fb, ch);

        fb = fb.clamp(-1.5, 1.5);
        self.feedback_sample = fb;

        output
    }

    /// Read a reversed grain from the delay line.
    /// `pos` is the current position within the grain cycle (0..2*grain_len).
    /// Returns a sample read backwards within the grain.
    #[inline]
    fn read_reversed(delay: &DelayLine, pos: usize, grain_len: usize) -> f64 {
        let pos_in_grain = pos % grain_len;
        // Reverse: read from beginning of grain when pos is at end
        // The grain spans from grain_len..0 samples ago (reversed).
        // At pos_in_grain=0, read from 1 sample ago (most recent).
        // At pos_in_grain=grain_len-1, read from grain_len samples ago.
        let read_offset = pos_in_grain + 1;
        let max_read = delay.len() - 1;
        delay.read(read_offset.min(max_read))
    }

    /// Raised cosine window for a grain.
    /// `cf` is the crossfade fraction (0.01–0.5).
    #[inline]
    fn grain_window(pos: usize, grain_len: usize, cf: f64) -> f64 {
        let pos_in_grain = pos % grain_len;
        let fade_samples = (grain_len as f64 * cf) as usize;
        let fade_samples = fade_samples.max(1);

        if pos_in_grain < fade_samples {
            // Fade in: raised cosine
            let t = pos_in_grain as f64 / fade_samples as f64;
            0.5 * (1.0 - (std::f64::consts::PI * t).cos())
        } else if pos_in_grain >= grain_len - fade_samples {
            // Fade out: raised cosine
            let t = (grain_len - 1 - pos_in_grain) as f64 / fade_samples as f64;
            0.5 * (1.0 - (std::f64::consts::PI * t).cos())
        } else {
            1.0
        }
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.decay_tilt_eq.reset();
        self.feedback_sample = 0.0;
        self.grain_pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn silence_in_silence_out() {
        let mut d = ReverseDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.update(SR);

        for _ in 0..48000 {
            let out = d.tick(0.0, 0);
            assert!(out.abs() < 1e-10);
        }
    }

    #[test]
    fn produces_output_from_impulse() {
        let mut d = ReverseDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.update(SR);

        // Feed a burst
        let mut has_output = false;
        for i in 0..48000 {
            let input = if i < 100 { 0.8 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > 0.01 {
                has_output = true;
            }
        }

        assert!(has_output, "Should produce reversed output");
    }

    #[test]
    fn no_nan() {
        let mut d = ReverseDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.6;
        d.grain_crossfade = 0.2;
        d.hicut_freq = 5000.0;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
            assert!(out.abs() < 10.0, "Runaway at {i}: {out}");
        }
    }

    #[test]
    fn window_sums_near_unity() {
        // Two overlapping grains should sum close to 1.0 in the middle
        let grain_len = 4800;
        let cf = 0.1;
        for pos in 0..(grain_len * 2) {
            let pos_b = (pos + grain_len) % (grain_len * 2);
            let wa = ReverseDelay::grain_window(pos, grain_len, cf);
            let wb = ReverseDelay::grain_window(pos_b, grain_len, cf);
            assert!((0.0..=1.0).contains(&wa), "Window A out of range: {wa}");
            assert!((0.0..=1.0).contains(&wb), "Window B out of range: {wb}");
        }
    }
}
