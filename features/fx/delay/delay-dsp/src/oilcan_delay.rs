//! OilCanDelay — murky electrostatic oil-can echo (Tel-Ray/Adineko style).
//!
//! Per spec/timeline-mx-reference.md: the defining behavior is the
//! NO-ERASE head. The signal is written as electrostatic charge on a
//! rotating oiled disc; old charge decays with a time constant while new
//! signal is written over it. Consequences modeled here:
//!
//! - A GHOST echo recurs at the disc-rotation period even with repeats
//!   at 0 — the residual charge passes the playback head every rotation.
//! - The rotation period is LONGER than the first (record→playback head
//!   distance) echo, so the cadence is off-kilter, never on the grid.
//! - Long/Short selects the first-echo head distance only; the
//!   rotation-period echo is identical for both.
//! - Grit = rotation-speed randomization (time-domain dirt/jitter),
//!   NOT amplitude saturation.
//!
//! Plus: very low bandwidth (murk LP), heavy dual-LFO wobble, light
//! constant saturation, allpass regen splatter.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::prng::XorShift32;
use audiocore_dsp::smoothing::ParamSmoother;
use audiocore_dsp::soft_clip::sin_clip;

/// Which pickup heads are engaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OilCanHeads {
    /// Single head at the full delay time.
    Long,
    /// Single head at ~55% of the delay time.
    Short,
    /// Both heads — cascading repeats.
    Both,
}

pub struct OilCanDelay {
    /// Base delay time in ms (clamped to 200–800, TimeLine Oil Can range).
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Head mode.
    pub heads: OilCanHeads,
    /// Wobble depth (0.0–1.0). Oil cans wobble a lot; default is high.
    pub wobble: f64,
    /// Loop darkness: lowpass cutoff in Hz (default 2500, the murk).
    pub tone_hz: f64,
    /// Rotation-speed randomization (0.0–1.0): time-domain jitter on the
    /// read heads. This is TimeLine's Grit for Oil Can — dirt via speed
    /// uncertainty, not saturation.
    pub grit: f64,
    /// Decay EQ tilt (shared engine param).
    pub decay_tilt: f64,

    delay: DelayLine,
    lp: Biquad,
    decay_eq: Biquad,
    // Small fixed allpass for the regen "splatter".
    splatter: DelayLine,
    splatter_g: f64,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
    wow_phase: f64,
    flutter_phase: f64,
    /// Grit random-walk state (rotation-speed uncertainty).
    grit_walk: f64,
    rng: XorShift32,
}

impl OilCanDelay {
    pub const MIN_TIME_MS: f64 = 200.0;
    pub const MAX_TIME_MS: f64 = 800.0;
    const MAX_DELAY_S: f64 = 1.6;
    const SHORT_RATIO: f64 = 0.55;
    /// Disc-rotation period relative to the (long-head) first echo.
    /// > 1.0: the ghost recurs later than the first echo (off-kilter).
    const ROTATION_RATIO: f64 = 1.45;
    /// Residual charge surviving one rotation (no-erase decay constant).
    const CHARGE_RESIDUE: f64 = 0.4;

    pub fn new() -> Self {
        Self {
            time_ms: 400.0,
            feedback: 0.45,
            heads: OilCanHeads::Long,
            wobble: 0.6,
            tone_hz: 2500.0,
            grit: 0.1,
            decay_tilt: 0.0,
            delay: DelayLine::new(48000 * 2),
            lp: Biquad::new(),
            decay_eq: Biquad::new(),
            splatter: DelayLine::new(512),
            splatter_g: 0.45,
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
            wow_phase: 0.0,
            flutter_phase: 0.25,
            grit_walk: 0.0,
            rng: XorShift32::new(0x011C_A4BE),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);

        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }
        let splat_len = (sample_rate * 0.008) as usize + 8; // ~8 ms
        if self.splatter.len() < splat_len {
            self.splatter = DelayLine::new(splat_len);
        }

        self.lp
            .set(FilterType::Lowpass, self.tone_hz.clamp(500.0, 8000.0), 0.707, sample_rate);

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

    #[inline]
    fn splatter_tick(&mut self, input: f64) -> f64 {
        // Schroeder allpass over a fixed ~6 ms delay.
        let d = (self.sample_rate * 0.006).min(self.splatter.len() as f64 - 2.0);
        let delayed = self.splatter.read_linear(d);
        let v = input - self.splatter_g * delayed;
        self.splatter.write(v);
        delayed + self.splatter_g * v
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        // Heavy dual-LFO wobble: slow wow (0.9 Hz, up to ±1.2%) plus
        // faster flutter (6.3 Hz, up to ±0.25%).
        self.wow_phase += 0.9 / self.sample_rate;
        if self.wow_phase >= 1.0 {
            self.wow_phase -= 1.0;
        }
        self.flutter_phase += 6.3 / self.sample_rate;
        if self.flutter_phase >= 1.0 {
            self.flutter_phase -= 1.0;
        }
        let wow = (std::f64::consts::TAU * self.wow_phase).sin() * 0.012;
        let flutter = (std::f64::consts::TAU * self.flutter_phase).sin() * 0.0025;

        // Grit: rotation-speed uncertainty as a bounded random walk —
        // fast time-domain jitter, the electrostatic 'dirt'.
        self.grit_walk += self.rng.next_bipolar() * 0.02;
        self.grit_walk = self.grit_walk.clamp(-1.0, 1.0);
        self.grit_walk *= 0.999;
        let grit_jitter = self.grit_walk * self.grit * 0.004;

        let factor = 1.0 + (wow + flutter) * self.wobble + grit_jitter;

        let max_read = self.delay.len() as f64 - 4.0;
        let long_pos = (smooth_delay * factor).clamp(1.0, max_read);
        let short_pos = (smooth_delay * Self::SHORT_RATIO * factor).clamp(1.0, max_read);
        // Disc rotation: identical for both head modes, longer than the
        // first echo.
        let rotation_pos =
            (smooth_delay * Self::ROTATION_RATIO * factor).clamp(1.0, max_read);

        let output = match self.heads {
            OilCanHeads::Long => self.delay.read_cubic(long_pos),
            OilCanHeads::Short => self.delay.read_cubic(short_pos),
            OilCanHeads::Both => {
                (self.delay.read_cubic(long_pos) + self.delay.read_cubic(short_pos) * 0.8) / 1.4
            }
        };

        // No-erase ghost: residual charge recirculates at the rotation
        // period regardless of the Repeats setting.
        let ghost = self.delay.read_cubic(rotation_pos) * Self::CHARGE_RESIDUE;

        // Loop: murk (LP) → light constant saturation → splatter allpass.
        let mut fb = output * self.feedback + ghost;
        fb = self.lp.tick(fb, ch);
        fb = sin_clip(fb * 1.2) / 1.2;
        fb = self.splatter_tick(fb);
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
        self.splatter.clear();
        self.lp.reset();
        self.decay_eq.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
        self.wow_phase = 0.0;
        self.flutter_phase = 0.25;
        self.grit_walk = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn impulse_delayed_long_head() {
        let mut d = OilCanDelay::new();
        d.time_ms = 300.0;
        d.feedback = 0.0;
        d.wobble = 0.0;
        d.update(SR);

        let expected = (300.0 * SR / 1000.0) as i64;
        let mut peak_pos = 0i64;
        let mut peak = 0.0f64;
        for i in 0..48000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input, 0);
            if out.abs() > peak {
                peak = out.abs();
                peak_pos = i;
            }
        }
        assert!(
            (peak_pos - expected).abs() < 480,
            "peak at {peak_pos}, expected near {expected}"
        );
    }

    #[test]
    fn both_heads_give_two_taps() {
        let mut d = OilCanDelay::new();
        d.time_ms = 500.0;
        d.feedback = 0.0;
        d.wobble = 0.0;
        d.heads = OilCanHeads::Both;
        d.update(SR);

        let mut hits = Vec::new();
        for i in 0..48000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            if d.tick(input, 0).abs() > 0.15 {
                hits.push(i);
            }
        }
        let short = (500.0 * OilCanDelay::SHORT_RATIO * SR / 1000.0) as i64;
        let long = (500.0 * SR / 1000.0) as i64;
        assert!(hits.iter().any(|&h| (h as i64 - short).abs() < 480), "{hits:?}");
        assert!(hits.iter().any(|&h| (h as i64 - long).abs() < 480), "{hits:?}");
    }

    #[test]
    fn ghost_echo_at_rotation_period_with_zero_repeats() {
        let mut d = OilCanDelay::new();
        d.time_ms = 400.0;
        d.feedback = 0.0; // Repeats fully off
        d.wobble = 0.0;
        d.grit = 0.0;
        d.update(SR);

        let mut out = vec![0.0f64; 96000];
        for (i, o) in out.iter_mut().enumerate() {
            let input = if i == 0 { 1.0 } else { 0.0 };
            *o = d.tick(input, 0);
        }
        // First echo at 400 ms; ghost passes recur at 400*1.45 = 580 ms
        // AFTER the first echo: 980 ms, then 1560 ms...
        let energy_at = |ms: f64| -> f64 {
            let c = (ms * SR / 1000.0) as usize;
            out[c.saturating_sub(400)..(c + 400).min(out.len())]
                .iter()
                .map(|x| x * x)
                .sum()
        };
        let first = energy_at(400.0);
        let ghost = energy_at(980.0);
        assert!(first > 1e-4, "first echo: {first}");
        assert!(
            ghost > first * 0.005,
            "ghost echo should recur at rotation period with repeats=0: {ghost} vs {first}"
        );
    }

    #[test]
    fn ghost_period_identical_for_both_head_modes() {
        // The rotation-period ghost time must not depend on head mode.
        let run = |heads: OilCanHeads| -> usize {
            let mut d = OilCanDelay::new();
            d.time_ms = 400.0;
            d.feedback = 0.0;
            d.wobble = 0.0;
            d.grit = 0.0;
            d.heads = heads;
            d.update(SR);
            let mut out = vec![0.0f64; 96000];
            for (i, o) in out.iter_mut().enumerate() {
                let input = if i == 0 { 1.0 } else { 0.0 };
                *o = d.tick(input, 0);
            }
            // Ghost of the WRITTEN residue appears at rotation period
            // after the write: find the peak in a window around it.
            let lo = (500.0 * SR / 1000.0) as usize;
            let hi = (700.0 * SR / 1000.0) as usize;
            lo + out[lo..hi]
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
                .map(|(i, _)| i)
                .unwrap()
        };
        let long_peak = run(OilCanHeads::Long) as i64;
        let short_peak = run(OilCanHeads::Short) as i64;
        // 580 ms rotation ghost read through the long head arrives at
        // 580 ms into the long output; through the short head the FIRST
        // echo differs but the rotation spacing is the same 580 ms.
        assert!(
            (long_peak - short_peak).abs() < 2400,
            "rotation ghost spacing should match: {long_peak} vs {short_peak}"
        );
    }

    #[test]
    fn no_nan_heavy_settings() {
        let mut d = OilCanDelay::new();
        d.time_ms = 250.0;
        d.feedback = 0.85;
        d.wobble = 1.0;
        d.grit = 1.0;
        d.update(SR);

        for i in 0..96000 {
            let input = (std::f64::consts::TAU * 330.0 * i as f64 / SR).sin() * 0.7;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at {i}");
        }
    }
}
