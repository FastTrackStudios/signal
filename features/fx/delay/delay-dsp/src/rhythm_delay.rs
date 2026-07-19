//! RhythmDelay — multi-tap rhythm delay with 8 taps at integer multiples.
//!
//! Each tap reads at 1x, 2x, 3x… 8x the base delay time, with independent
//! per-tap level control and a decay tilt EQ in the feedback path for tonal
//! shaping of repeats.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;

/// Multi-tap rhythm delay — 8 taps at integer multiples of base time.
pub struct RhythmDelay {
    /// Base delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (clamped to -1.5..1.5 internally).
    pub feedback: f64,
    /// Per-tap levels (0.0–1.0). Tap i reads at (i+1) * time_ms.
    pub tap_levels: [f64; 8],
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay tilt (-1.0 to 1.0). Negative = darkening repeats, positive = brightening.
    pub decay_tilt: f64,

    delay: DelayLine,
    hicut: Biquad,
    locut: Biquad,
    decay_eq: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
}

impl Default for RhythmDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl RhythmDelay {
    /// 5 seconds * 8 taps = 40 seconds max delay.
    const MAX_DELAY_S: f64 = 40.0;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            tap_levels: [1.0, 0.7, 0.5, 0.35, 0.25, 0.18, 0.12, 0.08],
            hicut_freq: 0.0,
            locut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 40 + 1024),
            hicut: Biquad::new(),
            locut: Biquad::new(),
            decay_eq: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
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
        if self.locut_freq > 0.0 {
            self.locut.set(
                FilterType::Highpass,
                self.locut_freq,
                self.filter_q,
                sample_rate,
            );
        }

        // Decay tilt EQ
        if self.decay_tilt < 0.0 {
            // Negative = lowpass (darkening). Freq sweeps from 1000 Hz down to ~1000 Hz min.
            let freq = 20000.0 * (1.0 + self.decay_tilt).max(0.05);
            self.decay_eq
                .set(FilterType::Lowpass, freq, 0.707, sample_rate);
        } else if self.decay_tilt > 0.0 {
            // Positive = highpass (brightening).
            let freq = 20.0 + self.decay_tilt * 2000.0;
            self.decay_eq
                .set(FilterType::Highpass, freq, 0.707, sample_rate);
        }

        self.smoother.set_time(0.15, sample_rate);
        let target = self.time_ms * 0.001 * sample_rate;
        if self.smoother.value() == 0.0 {
            self.smoother.set_immediate(target);
        }
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let max_read = self.delay.len() as f64 - 4.0;

        // Sum all 8 taps
        let mut output = 0.0;
        let mut last_tap = 0.0;
        for i in 0..8 {
            let tap_delay = smooth_delay * (i + 1) as f64;
            let read_pos = tap_delay.clamp(1.0, max_read);
            let tap_out = self.delay.read_cubic(read_pos);
            output += tap_out * self.tap_levels[i];
            if i == 7 {
                last_tap = tap_out;
            }
        }

        // Feedback path: last tap (8x) → filters → clamp
        let mut fb = last_tap * self.feedback;

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }
        if self.decay_tilt != 0.0 {
            fb = self.decay_eq.tick(fb, ch);
        }

        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.locut.reset();
        self.decay_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn silence_in_silence_out() {
        let mut d = RhythmDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.update(SR);

        for _ in 0..48000 {
            let out = d.tick(0.0, 0);
            assert!(out.abs() < 1e-10);
        }
    }

    #[test]
    fn impulse_produces_multiple_taps() {
        let mut d = RhythmDelay::new();
        d.time_ms = 50.0; // 50ms base → taps at 50, 100, 150, 200ms...
        d.feedback = 0.0;
        d.tap_levels = [1.0; 8]; // All taps at unity for easy detection
        d.update(SR);

        let total_samples = 25000; // ~520ms, enough for tap 8 at 400ms
        let mut samples = vec![0.0f64; total_samples];

        for i in 0..total_samples {
            let input = if i == 0 { 1.0 } else { 0.0 };
            samples[i] = d.tick(input, 0);
        }

        // Expect peaks near 2400, 4800, 7200, 9600 samples (50ms multiples)
        let expected_positions = [2400, 4800, 7200, 9600];
        for &pos in &expected_positions {
            // Find peak in a window around expected position
            let window_start = (pos as i64 - 20).max(0) as usize;
            let window_end = (pos + 20).min(total_samples);
            let peak = samples[window_start..window_end]
                .iter()
                .copied()
                .fold(0.0f64, f64::max);
            assert!(
                peak > 0.5,
                "Expected tap peak near sample {pos}, got max {peak}"
            );
        }
    }

    #[test]
    fn no_nan() {
        let mut d = RhythmDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.hicut_freq = 5000.0;
        d.locut_freq = 100.0;
        d.decay_tilt = -0.5;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn tap_levels_control_output() {
        // With all taps at zero except tap 1, output should only have the first tap
        let mut d = RhythmDelay::new();
        d.time_ms = 50.0;
        d.feedback = 0.0;
        d.tap_levels = [0.0; 8];
        d.tap_levels[0] = 1.0; // Only first tap active
        d.update(SR);

        let total_samples = 25000;
        let mut samples = vec![0.0f64; total_samples];

        for i in 0..total_samples {
            let input = if i == 0 { 1.0 } else { 0.0 };
            samples[i] = d.tick(input, 0);
        }

        // Tap 1 at 50ms (2400 samples) should produce a peak
        let window = &samples[2380..2420];
        let peak = window.iter().copied().fold(0.0f64, f64::max);
        assert!(peak > 0.5, "Tap 1 should produce output: {peak}");

        // Tap 2 at 100ms (4800 samples) should be silent
        let window = &samples[4780..4820];
        let peak = window.iter().copied().fold(0.0f64, |a, b| a.max(b.abs()));
        assert!(peak < 1e-10, "Tap 2 should be silent with level=0: {peak}");
    }
}
