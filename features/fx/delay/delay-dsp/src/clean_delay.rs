//! CleanDelay — pristine digital delay with no coloration.
//!
//! Simple delay line with cubic interpolation and optional feedback filtering.
//! No modulation, no saturation — the cleanest possible repeats.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;

/// Clean digital delay — pristine repeats with optional feedback EQ.
pub struct CleanDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Low-cut filter frequency in Hz (0 = disabled).
    pub locut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_eq: Biquad,
    delay: DelayLine,
    hicut: Biquad,
    locut: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
}

impl Default for CleanDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl CleanDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            hicut_freq: 0.0,
            locut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            decay_eq: Biquad::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            locut: Biquad::new(),
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

        // Decay EQ: tilt filter in feedback path
        if self.decay_tilt.abs() > 0.01 {
            if self.decay_tilt < 0.0 {
                let freq = 20000.0 * (1.0 + self.decay_tilt).max(0.05);
                self.decay_eq
                    .set(FilterType::Lowpass, freq, 0.707, sample_rate);
            } else {
                let freq = 20.0 + self.decay_tilt * 2000.0;
                self.decay_eq
                    .set(FilterType::Highpass, freq, 0.707, sample_rate);
            }
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
        let read_pos = smooth_delay.clamp(1.0, max_read);
        let output = self.delay.read_cubic(read_pos);

        // Feedback path: output → filter → limit
        let mut fb = output * self.feedback;

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }
        if self.locut_freq > 0.0 {
            fb = self.locut.tick(fb, ch);
        }

        if self.decay_tilt.abs() > 0.01 {
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
        let mut d = CleanDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.update(SR);

        for _ in 0..48000 {
            let out = d.tick(0.0, 0);
            assert!(out.abs() < 1e-10);
        }
    }

    #[test]
    fn impulse_delayed() {
        let mut d = CleanDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.update(SR);

        let expected = 4800; // 100ms at 48kHz
        let mut peak_pos = 0;
        let mut peak_val = 0.0f64;

        for i in 0..10000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > peak_val {
                peak_val = out.abs();
                peak_pos = i;
            }
        }

        assert!(
            (peak_pos as i64 - expected as i64).unsigned_abs() < 10,
            "Peak at {peak_pos}, expected near {expected}"
        );
        assert!(peak_val > 0.5, "Peak should be significant: {peak_val}");
    }

    #[test]
    fn no_nan() {
        let mut d = CleanDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.hicut_freq = 5000.0;
        d.locut_freq = 100.0;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }
}
