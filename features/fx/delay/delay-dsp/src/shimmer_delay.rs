//! ShimmerDelay — pitch-shifted delay for ethereal/ambient textures.
//!
//! Delay line with pitch shifter in the feedback path. Each repeat
//! is shifted by `pitch_ratio`, creating cascading shimmer effects.
//! The pitched path runs through pitch-dsp's `WsolaShifter`
//! (waveform-aligned splices) — measured ~50x less inharmonic energy
//! than the dual-grain granular approach on tonal material, which is
//! exactly what a recirculating shimmer stack needs. The tap is read
//! `latency()` samples early so pitched and unpitched paths stay
//! time-aligned.

use crate::tilt::DecayTilt;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;
use pitch_dsp::wsola::WsolaShifter;

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
    /// WSOLA pitch shifter (pitch-dsp) for the pitched feedback path.
    shifter: WsolaShifter,
    /// Shifter latency in samples (constant, speed-independent).
    shifter_latency: f64,
    /// Grain size in ms for pitch shifter (10–100). Larger = smoother.
    pub grain_ms: f64,
}

impl Default for ShimmerDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl ShimmerDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        let mut shifter = WsolaShifter::new();
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
            shifter_latency: 1024.0,
            grain_ms: 30.0,
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

        // WSOLA grain: derived from grain_ms but bounded — the splice
        // correlation search cost grows with grain size. Reconfigure only
        // on real change (update resets the shifter pipeline).
        let base_grain = ((self.grain_ms * 0.001 * 48000.0) as usize).clamp(256, 2048);
        if base_grain != self.shifter.base_grain_size
            || (sample_rate - self.sample_rate).abs() > 1e-9
        {
            self.shifter.base_grain_size = base_grain;
            self.shifter.update(sample_rate);
            self.shifter_latency = self.shifter.latency() as f64;
        }

        self.dc_blocker.set_cutoff(10.0, sample_rate);
        self.smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let max_read = self.delay.len() as f64 - 4.0;

        // === Normal (unpitched) read ===
        let normal_output = self.delay.read_cubic(smooth_delay.clamp(1.0, max_read));

        // === Pitched read: tap `latency()` early (WSOLA latency is
        // constant and speed-independent), so both paths land at the
        // delay time.
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
            fb = fb * (3.0 - fb.abs() * 2.0).max(0.0) / 3.0;
        }
        // Pitch-shifted feedback is the classic DC/subsonic accumulator —
        // block it inside the loop.
        fb = self.dc_blocker.tick(fb.clamp(-1.5, 1.5));

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
            (peak_pos as i64 - 4800).unsigned_abs() < 10,
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
            let s = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
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
            let input = (std::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
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
