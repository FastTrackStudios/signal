//! BbdDelay — Bucket Brigade Device emulation.
//!
//! Warm, dark, chorused analog delay with clock modulation and jitter.
//! Simplified BBD model: delay line with LFO-modulated read position,
//! anti-aliasing/reconstruction filters, and clock jitter.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

/// BBD/Analog delay with clock modulation and filtering.
pub struct BbdDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// LFO modulation depth (0.0–1.0).
    pub mod_depth: f64,
    /// LFO modulation rate in Hz.
    pub mod_rate: f64,
    /// Tone / low-pass cutoff frequency in Hz.
    pub tone: f64,
    /// Clock jitter amount (0.0–1.0).
    pub clock_jitter: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_eq: Biquad,
    delay: DelayLine,
    // Anti-aliasing (input) and reconstruction (output) filters
    aa_filter: Biquad,
    recon_filter: Biquad,
    // Feedback tone filter
    tone_filter: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    // LFO state
    lfo_phase: f64,
    // Clock jitter PRNG
    rng: XorShift32,
}

impl BbdDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            mod_depth: 0.3,
            mod_rate: 1.0,
            tone: 4000.0,
            clock_jitter: 0.3,
            decay_tilt: 0.0,
            decay_eq: Biquad::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            aa_filter: Biquad::new(),
            recon_filter: Biquad::new(),
            tone_filter: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            lfo_phase: 0.0,
            rng: XorShift32::new(0xDEAD_BEEF),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        // BBD anti-aliasing: gentle LP at ~10kHz (simulates input sample-and-hold)
        let aa_freq = 10000.0f64.min(sample_rate * 0.45);
        self.aa_filter
            .set(FilterType::Lowpass, aa_freq, 0.707, sample_rate);

        // Reconstruction filter (same characteristic)
        self.recon_filter
            .set(FilterType::Lowpass, aa_freq, 0.707, sample_rate);

        // Tone / feedback LP
        let tone_freq = self.tone.clamp(200.0, sample_rate * 0.45);
        self.tone_filter
            .set(FilterType::Lowpass, tone_freq, 0.707, sample_rate);

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
        // Anti-aliasing filter on input
        let filtered_input = self.aa_filter.tick(input, ch);

        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        // LFO modulation on read position
        let lfo_inc = self.mod_rate / self.sample_rate;
        self.lfo_phase += lfo_inc;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo = (self.lfo_phase * std::f64::consts::TAU).sin();

        // Clock jitter: small random offset
        let jitter = self.rng.next_bipolar() * self.clock_jitter * 2.0;

        // Modulated read position
        let mod_offset = lfo * self.mod_depth * smooth_delay * 0.02 + jitter;
        let max_read = self.delay.len() as f64 - 4.0;
        let read_pos = (smooth_delay + mod_offset).clamp(1.0, max_read);

        let raw_output = self.delay.read_cubic(read_pos);

        // Reconstruction filter
        let output = self.recon_filter.tick(raw_output, ch);

        // Feedback path: tone filter → limit
        let mut fb = output * self.feedback;
        fb = self.tone_filter.tick(fb, ch);

        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, ch);
        }

        fb = fb.clamp(-1.5, 1.5);

        self.delay.write(filtered_input + fb);
        self.feedback_sample = fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.aa_filter.reset();
        self.recon_filter.reset();
        self.tone_filter.reset();
        self.decay_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.lfo_phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn silence_in_silence_out() {
        let mut d = BbdDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.mod_depth = 0.0;
        d.clock_jitter = 0.0;
        d.update(SR);

        for _ in 0..48000 {
            let out = d.tick(0.0, 0);
            assert!(out.abs() < 1e-6, "Expected silence: {out}");
        }
    }

    #[test]
    fn impulse_delayed() {
        let mut d = BbdDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.mod_depth = 0.0;
        d.clock_jitter = 0.0;
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

        // Peak should be near 4800 (100ms at 48kHz), but filters add latency
        assert!(
            peak_pos > 4700 && peak_pos < 5100,
            "Peak at {peak_pos}, expected near 4800"
        );
        assert!(peak_val > 0.1, "Peak should be significant: {peak_val}");
    }

    #[test]
    fn modulation_changes_output() {
        let mut d_clean = BbdDelay::new();
        d_clean.time_ms = 100.0;
        d_clean.feedback = 0.0;
        d_clean.mod_depth = 0.0;
        d_clean.clock_jitter = 0.0;
        d_clean.update(SR);

        let mut d_mod = BbdDelay::new();
        d_mod.time_ms = 100.0;
        d_mod.feedback = 0.0;
        d_mod.mod_depth = 0.8;
        d_mod.mod_rate = 2.0;
        d_mod.clock_jitter = 0.0;
        d_mod.update(SR);

        let mut diff = 0.0;
        for i in 0..19200 {
            let s = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let a = d_clean.tick(s, 0);
            let b = d_mod.tick(s, 0);
            diff += (a - b).abs();
        }

        assert!(diff > 0.1, "Modulation should change output: diff={diff}");
    }

    #[test]
    fn no_nan() {
        let mut d = BbdDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.mod_depth = 0.5;
        d.mod_rate = 3.0;
        d.clock_jitter = 0.5;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
            assert!(out.abs() < 10.0, "Runaway at {i}: {out}");
        }
    }
}
