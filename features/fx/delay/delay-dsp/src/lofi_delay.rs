//! LoFiDelay — bit-crushed, sample-rate-reduced delay for degraded sound.
//!
//! Applies bit depth reduction and sample rate reduction to create
//! lo-fi, retro delay textures.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

/// Lo-Fi delay with bit crushing and sample rate reduction.
pub struct LoFiDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Bit depth for quantization (4–32).
    pub bit_depth: f64,
    /// Sample rate divisor (1–64). 1 = no reduction.
    pub sample_rate_div: f64,
    /// Noise floor injection (0.0–1.0). Adds hiss/noise to feedback path.
    pub noise: f64,
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
    // Sample-rate reduction state
    sr_counter: f64,
    sr_hold: f64,
    // Noise PRNG
    rng: XorShift32,
}

impl LoFiDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            bit_depth: 12.0,
            sample_rate_div: 4.0,
            noise: 0.0,
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
            sr_counter: 0.0,
            sr_hold: 0.0,
            rng: XorShift32::new(0xCAFE_BABE),
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

    /// Quantize to simulated bit depth.
    #[inline]
    fn quantize(x: f64, bits: f64) -> f64 {
        let steps = (2.0f64).powf(bits);
        (x * steps).round() / steps
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let max_read = self.delay.len() as f64 - 4.0;
        let read_pos = smooth_delay.clamp(1.0, max_read);
        let mut output = self.delay.read_cubic(read_pos);

        // Apply lo-fi degradation to output
        // Sample rate reduction (hold-and-sample)
        self.sr_counter += 1.0;
        if self.sr_counter >= self.sample_rate_div {
            self.sr_counter = 0.0;
            self.sr_hold = output;
        }
        output = self.sr_hold;

        // Bit depth reduction
        output = Self::quantize(output, self.bit_depth);

        // Feedback path
        let mut fb = output * self.feedback;

        // Noise injection (lo-fi hiss/noise floor)
        if self.noise > 0.0 {
            fb += self.rng.next_bipolar() * self.noise * 0.05;
        }

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
        self.sr_counter = 0.0;
        self.sr_hold = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn quantize_reduces_precision() {
        // 8-bit quantization should snap to 256 levels
        let q = LoFiDelay::quantize(0.123456, 8.0);
        let step = 1.0 / 256.0;
        let remainder = (q / step).fract();
        assert!(
            remainder.abs() < 1e-10 || (1.0 - remainder).abs() < 1e-10,
            "Should quantize to step: q={q}"
        );
    }

    #[test]
    fn impulse_delayed() {
        let mut d = LoFiDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.bit_depth = 24.0; // High bit depth for clean test
        d.sample_rate_div = 1.0; // No SR reduction
        d.update(SR);

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
            (peak_pos as i64 - 4800).unsigned_abs() < 10,
            "Peak at {peak_pos}, expected near 4800"
        );
    }

    #[test]
    fn no_nan() {
        let mut d = LoFiDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.bit_depth = 8.0;
        d.sample_rate_div = 8.0;
        d.hicut_freq = 4000.0;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn sr_reduction_changes_output() {
        let mut d_clean = LoFiDelay::new();
        d_clean.time_ms = 50.0;
        d_clean.feedback = 0.0;
        d_clean.bit_depth = 24.0;
        d_clean.sample_rate_div = 1.0;
        d_clean.update(SR);

        let mut d_lofi = LoFiDelay::new();
        d_lofi.time_ms = 50.0;
        d_lofi.feedback = 0.0;
        d_lofi.bit_depth = 24.0;
        d_lofi.sample_rate_div = 16.0;
        d_lofi.update(SR);

        let mut diff = 0.0;
        for i in 0..9600 {
            let s = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let a = d_clean.tick(s, 0);
            let b = d_lofi.tick(s, 0);
            diff += (a - b).abs();
        }

        assert!(diff > 0.1, "SR reduction should change output: diff={diff}");
    }
}
