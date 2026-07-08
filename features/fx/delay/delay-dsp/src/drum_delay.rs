//! DrumDelay — multi-head "drum echo" (Binson Echorec style).
//!
//! TimeLine MX "Drum" machine parity: four playback heads on one
//! rotating-drum delay line, each individually enabled with its own
//! level, feedback contribution, and pan. Default spacing follows the
//! golden ratio, like the Echorec's magnetic drum head layout.
//!
//! First-pass implementation: heads read one shared `DelayLine`; a slow
//! random wobble models drum-motor irregularity. Per-head `pan` is
//! stored for API parity but not applied — the engine is mono-per-channel
//! inside `DelayChain`; stereo head placement lands with the deep pass.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;

/// One playback head.
#[derive(Debug, Clone, Copy)]
pub struct DrumHead {
    pub enabled: bool,
    /// Head position as a fraction of the base delay time (0.0–1.0].
    pub position: f64,
    /// Output level (0.0–1.0).
    pub level: f64,
    /// Contribution to the feedback sum (0.0–1.0).
    pub feedback: f64,
    /// Stereo pan (-1.0–1.0). Stored for parity; applied in the deep pass.
    pub pan: f64,
}

/// Golden-ratio head spacing (1/φ³, 1/φ², 1/φ, 1) — Echorec-like.
pub const GOLDEN_HEADS: [f64; 4] = [0.236, 0.382, 0.618, 1.0];
/// Silver-ratio head spacing (1/δ³ …), tighter early cluster.
pub const SILVER_HEADS: [f64; 4] = [0.071, 0.172, 0.414, 1.0];

pub struct DrumDelay {
    /// Base delay time in ms (clamped to 200–2000, TimeLine Drum range).
    pub time_ms: f64,
    /// Global feedback scale applied to the per-head feedback sum (0.0–1.0).
    pub feedback: f64,
    /// The four playback heads.
    pub heads: [DrumHead; 4],
    /// Low-frequency shaping of the echoes: 0 = full low end,
    /// 1 = heavily thinned (progressive high-pass up to ~500 Hz).
    pub lo_cut: f64,
    /// Drum-motor wobble depth (0.0–1.0).
    pub wobble: f64,
    /// Decay EQ tilt (shared engine param; applied in the feedback path).
    pub decay_tilt: f64,

    delay: DelayLine,
    lo_cut_filter: Biquad,
    decay_eq: Biquad,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,

    // Slow random wobble: sample-and-hold noise glide.
    wobble_phase: f64,
    wobble_current: f64,
    wobble_target: f64,
    rng: XorShift32,
}

impl DrumDelay {
    pub const MIN_TIME_MS: f64 = 200.0;
    pub const MAX_TIME_MS: f64 = 2000.0;
    const MAX_DELAY_S: f64 = 2.5;

    pub fn new() -> Self {
        let heads = GOLDEN_HEADS.map(|position| DrumHead {
            enabled: true,
            position,
            level: 0.7,
            feedback: if position == 1.0 { 1.0 } else { 0.0 },
            pan: 0.0,
        });
        Self {
            time_ms: 400.0,
            feedback: 0.4,
            heads,
            lo_cut: 0.2,
            wobble: 0.15,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 3),
            lo_cut_filter: Biquad::new(),
            decay_eq: Biquad::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            wobble_phase: 0.0,
            wobble_current: 0.0,
            wobble_target: 0.0,
            rng: XorShift32::new(0xD2D2_1717),
        }
    }

    /// Apply a spacing preset to the head positions (keeps other head params).
    pub fn set_spacing(&mut self, spacing: [f64; 4]) {
        for (head, pos) in self.heads.iter_mut().zip(spacing) {
            head.position = pos;
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);

        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }

        // Progressive high-pass: lo_cut 0..1 → off..500 Hz.
        if self.lo_cut > 0.01 {
            let freq = 20.0 + self.lo_cut * 480.0;
            self.lo_cut_filter
                .set(FilterType::Highpass, freq, 0.707, sample_rate);
        }

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

        // Drum-motor wobble: sample-and-hold noise glided at ~1.3 Hz,
        // scaled to ±0.3% of the delay time at full depth.
        self.wobble_phase += 1.3 / self.sample_rate;
        if self.wobble_phase >= 1.0 {
            self.wobble_phase -= 1.0;
            self.wobble_target = self.rng.next_bipolar();
        }
        self.wobble_current += (self.wobble_target - self.wobble_current) * 0.0005;
        let wobble_factor = 1.0 + self.wobble_current * self.wobble * 0.003;

        let max_read = self.delay.len() as f64 - 4.0;

        let mut output = 0.0;
        let mut fb_sum = 0.0;
        for head in &self.heads {
            if !head.enabled {
                continue;
            }
            let pos = (smooth_delay * head.position * wobble_factor).clamp(1.0, max_read);
            let sample = self.delay.read_cubic(pos);
            output += sample * head.level;
            fb_sum += sample * head.feedback;
        }

        let mut fb = fb_sum * self.feedback;
        if self.lo_cut > 0.01 {
            fb = self.lo_cut_filter.tick(fb, ch);
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
        self.lo_cut_filter.reset();
        self.decay_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.wobble_phase = 0.0;
        self.wobble_current = 0.0;
        self.wobble_target = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn impulse_produces_four_head_taps() {
        let mut d = DrumDelay::new();
        d.time_ms = 500.0;
        d.feedback = 0.0;
        d.wobble = 0.0;
        d.lo_cut = 0.0;
        d.update(SR);

        let mut peaks = Vec::new();
        for i in 0..48000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > 0.2 {
                peaks.push(i);
            }
        }
        // 4 golden-spaced heads → distinct clusters near 118/191/309/500 ms.
        for expected_ms in [118.0, 191.0, 309.0, 500.0] {
            let expected = (expected_ms * SR / 1000.0) as i64;
            assert!(
                peaks.iter().any(|&p| (p as i64 - expected).abs() < 240),
                "expected a head tap near {expected_ms} ms, peaks: {peaks:?}"
            );
        }
    }

    #[test]
    fn time_clamped_to_drum_range() {
        let mut d = DrumDelay::new();
        d.time_ms = 50.0;
        d.update(SR);
        assert_eq!(d.time_ms, DrumDelay::MIN_TIME_MS);
        d.time_ms = 9999.0;
        d.update(SR);
        assert_eq!(d.time_ms, DrumDelay::MAX_TIME_MS);
    }

    #[test]
    fn no_nan_with_feedback_and_wobble() {
        let mut d = DrumDelay::new();
        d.time_ms = 300.0;
        d.feedback = 0.7;
        d.wobble = 1.0;
        d.lo_cut = 0.8;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::TAU * 220.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at {i}");
        }
    }
}
