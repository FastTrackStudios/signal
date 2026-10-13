//! `ShimmerDelay` — pitch-shifted delay for ethereal/ambient textures.
//!
//! Delay line with pitch shifter in the feedback path. Each repeat
//! is shifted by `pitch_ratio`, creating cascading shimmer effects.
//! The pitched path runs through pitch-dsp's `SpectralShifter` (a
//! phase-locked phase vocoder): no splices at all, so nothing for the
//! recirculating stack to compound — on a steady tone at +12 it leaves
//! −74 dB of non-harmonic energy where the WSOLA shifter it replaced left
//! −29 dB, and it is polyphonic (WSOLA's single splice point fails on
//! chords). The tap is read `latency()` samples early so pitched and
//! unpitched paths stay time-aligned.

use crate::tilt::DecayTilt;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;
use dsp_core::num;
use pitch_dsp::spectral::SpectralShifter;

/// Shimmer delay with pitch shifting in the feedback path.
pub struct ShimmerDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Pitch ratio (0.5–4.0). 2.0 = octave up, 1.498 = fifth up.
    pub pitch_ratio: f64,
    /// Shimmer mix (0.0–1.0). Blend between pitched and unpitched feedback.
    pub shimmer_mix: f64,
    /// High-cut filter frequency in Hz (0 = disabled).
    pub hicut_freq: f64,
    /// Filter Q.
    pub filter_q: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_tilt_eq: DecayTilt,
    delay: DelayLine,
    hicut: Biquad,
    dc_blocker: DcBlocker,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    /// Phase-vocoder pitch shifter (pitch-dsp) for the pitched path.
    shifter: SpectralShifter,
    /// Shifter latency in samples (constant, speed-independent).
    shifter_latency: f64,
    /// Shifter frame (48 kHz samples) and rate in effect.
    shifter_frame: usize,
    shifter_rate: f64,
    /// Shifter analysis frame in ms (10–170; rounded to a power-of-two
    /// frame, 1024–8192 samples @ 48 kHz). Larger = smoother, more latency
    /// (compensated, but capped at the delay time).
    pub grain_ms: f64,
}

impl Default for ShimmerDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl ShimmerDelay {
    const MAX_DELAY_S: f64 = 5.0;

    #[must_use]
    pub fn new() -> Self {
        let mut shifter = SpectralShifter::new();
        shifter.mix = 1.0;
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            pitch_ratio: 2.0,
            shimmer_mix: 0.5,
            hicut_freq: 8000.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            dc_blocker: DcBlocker::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            shifter,
            shifter_latency: 0.0,
            shifter_frame: 0,
            shifter_rate: 0.0,
            grain_ms: 90.0,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = num::f64_to_index(sample_rate * Self::MAX_DELAY_S).saturating_add(1024);
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

        // Shifter frame from grain_ms, capped at the delay time so the
        // early tap can compensate its latency. Reconfigure only on real
        // change (update clears the shifter).
        let want = num::f64_to_index(self.grain_ms * 48.0);
        let fit = num::f64_to_index(self.time_ms * 48.0);
        let mut frame = 1024usize;
        while frame < 8192 && frame.saturating_mul(2) <= want.min(fit).max(1024) {
            frame = frame.saturating_mul(2);
        }
        if frame != self.shifter_frame || (sample_rate - self.shifter_rate).abs() > 1e-9 {
            self.shifter_frame = frame;
            self.shifter_rate = sample_rate;
            self.shifter.fft_size = frame;
            self.shifter.update(sample_rate);
            self.shifter_latency = num::count_to_f64(self.shifter.latency());
        }

        self.dc_blocker.set_cutoff(10.0, sample_rate);
        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let max_read = num::count_to_f64(self.delay.len()) - 4.0;

        // === Normal (unpitched) read ===
        let normal_output = self.delay.read_cubic(smooth_delay.clamp(1.0, max_read));

        // === Pitched read: tap `latency()` early (the shifter's latency is
        // its frame — constant and speed-independent), so both paths land
        // at the delay time.
        let tap_delay = (smooth_delay - self.shifter_latency).clamp(1.0, max_read);
        let tap = self.delay.read_cubic(tap_delay);
        self.shifter.speed = self.pitch_ratio;
        let pitched_output = self.shifter.tick(tap);

        // Blend pitched and unpitched output
        let output = normal_output * (1.0 - self.shimmer_mix) + pitched_output * self.shimmer_mix;

        // Feedback path: use the blended signal
        let mut fb = output * self.feedback;

        if self.hicut_freq > 0.0 {
            fb = self.hicut.tick(fb, ch);
        }

        fb = self.decay_tilt_eq.tick(fb, ch);

        // Self-limiting feedback (from PitchDelay)
        if fb.abs() > 0.001 {
            fb = fb * fb.abs().mul_add(-2.0, 3.0).max(0.0) / 3.0;
        }
        // Pitch-shifted feedback is the classic DC/subsonic accumulator —
        // block it inside the loop.
        fb = self.dc_blocker.tick(fb.clamp(-1.5, 1.5));

        self.delay.write(input + fb);
        self.feedback_sample = fb;

        output
    }

    #[must_use]
    pub const fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.hicut.reset();
        self.decay_tilt_eq.reset();
        self.dc_blocker.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.shifter.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn impulse_delayed() {
        let mut d = ShimmerDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.pitch_ratio = 1.0; // No pitch shift
        d.shimmer_mix = 0.0;
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
            (i64::from(peak_pos) - 4800).unsigned_abs() < 10,
            "Peak at {peak_pos}, expected near 4800"
        );
    }

    #[test]
    fn shimmer_changes_output() {
        let mut d_dry = ShimmerDelay::new();
        d_dry.time_ms = 100.0;
        d_dry.feedback = 0.5;
        d_dry.pitch_ratio = 1.0;
        d_dry.shimmer_mix = 0.0;
        d_dry.update(SR);

        let mut d_shimmer = ShimmerDelay::new();
        d_shimmer.time_ms = 100.0;
        d_shimmer.feedback = 0.5;
        d_shimmer.pitch_ratio = 2.0;
        d_shimmer.shimmer_mix = 1.0;
        d_shimmer.update(SR);

        let mut diff = 0.0;
        for i in 0..19200 {
            let s = (std::f64::consts::PI * 2.0 * 440.0 * f64::from(i) / SR).sin() * 0.5;
            let a = d_dry.tick(s, 0);
            let b = d_shimmer.tick(s, 0);
            diff += (a - b).abs();
        }

        assert!(diff > 0.1, "Shimmer should change output: diff={diff}");
    }

    #[test]
    fn no_nan() {
        let mut d = ShimmerDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.7;
        d.pitch_ratio = 2.0;
        d.shimmer_mix = 0.8;
        d.hicut_freq = 6000.0;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::PI * 2.0 * 440.0 * f64::from(i) / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn feedback_self_limits() {
        let mut d = ShimmerDelay::new();
        d.time_ms = 50.0;
        d.feedback = 0.99;
        d.pitch_ratio = 2.0;
        d.shimmer_mix = 1.0;
        d.update(SR);

        for _ in 0..480 {
            d.tick(1.0, 0);
        }

        let mut max_out: f64 = 0.0;
        for _ in 0..96000 {
            let out = d.tick(0.0, 0);
            max_out = max_out.max(out.abs());
        }

        assert!(max_out < 5.0, "Should self-limit: max={max_out}");
    }
}
