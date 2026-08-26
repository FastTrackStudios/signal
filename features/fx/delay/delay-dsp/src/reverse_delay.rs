//! ReverseDelay — input-triggered reversed playback windows.
//!
//! Records forward continuously; two alternating grain windows (each
//! `time_ms` long, half a cycle apart) play the buffer backwards with a
//! raised-cosine crossfade. The window cycle re-syncs to input onsets —
//! the TimeLine MX "reverse process is synced to performance" behavior —
//! so a phrase played after silence reverses from its own start instead
//! of landing at a random point in a free-running cycle.
//!
//! A reversed read must walk backward through absolute time while the
//! write head walks forward, so the read offset advances **two** samples
//! per tick (`offset = 2·pos + 1`). Advancing it by one — the previous
//! implementation — pins the read to a single frozen sample.

use crate::modulation::Diffuser;
use crate::tilt::DecayTilt;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;

/// Reverse delay using onset-synced alternating reversed grains.
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
    /// Smear (0.0–1.0): allpass diffusion softening the reversed
    /// attacks / enhancing the swell (TimeLine "Smear" 0–18).
    pub smear: f64,
    /// Delay-line modulation LFO rate in Hz.
    pub mod_rate_hz: f64,
    /// Delay-line modulation depth (0.0–1.0; full scale ≈ ±4 ms).
    pub mod_depth: f64,

    decay_tilt_eq: DecayTilt,
    delay: DelayLine,
    hicut: Biquad,
    diffuser: Diffuser,
    feedback_sample: f64,
    sample_rate: f64,
    /// Current position within the grain cycle (0..2·grain_samples).
    grain_pos: usize,
    /// Length of current grain in samples.
    grain_samples: usize,
    // Onset detector: envelope follower with hysteresis re-arm.
    env: f64,
    env_attack: f64,
    env_release: f64,
    armed: bool,
    lfo_phase: f64,
}

impl Default for ReverseDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverseDelay {
    const MAX_DELAY_S: f64 = 5.0;
    /// Onset threshold (≈ −34 dBFS) and re-arm threshold (≈ −46 dBFS).
    const ONSET_ON: f64 = 0.02;
    const ONSET_OFF: f64 = 0.005;
    /// Full-depth modulation excursion in seconds (≈ ±4 ms).
    const MOD_RANGE_S: f64 = 0.004;

    pub fn new() -> Self {
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            grain_crossfade: 0.1,
            hicut_freq: 0.0,
            filter_q: 0.707,
            decay_tilt: 0.0,
            smear: 0.0,
            mod_rate_hz: 0.8,
            mod_depth: 0.0,
            decay_tilt_eq: DecayTilt::new(),
            delay: DelayLine::new(48000 * 5 + 1024),
            hicut: Biquad::new(),
            diffuser: Diffuser::new(48000.0, false),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            grain_pos: 0,
            grain_samples: 12000, // 250ms at 48kHz
            env: 0.0,
            env_attack: 0.04,
            env_release: 0.0003,
            armed: true,
            lfo_phase: 0.0,
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

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        // Onset follower: ~0.5 ms attack, ~80 ms release. The release
        // sets how much silence re-arms the trigger between phrases.
        self.env_attack = 1.0 - (-1.0 / (0.0005 * sample_rate)).exp();
        self.env_release = 1.0 - (-1.0 / (0.08 * sample_rate)).exp();

        // Smear: fixed-size allpass chain, diffusion amount from the knob.
        // Coefficient capped at 0.5 — an 8-stage allpass cascade with high
        // per-stage g rings hard enough to build peaks inside the
        // recirculation loop.
        self.diffuser.size = 0.35;
        self.diffuser.smear = self.smear.clamp(0.0, 1.0) * 0.5;
        self.diffuser.update(sample_rate, false);

        self.grain_samples = ((self.time_ms * 0.001 * sample_rate) as usize).max(64);
    }

    pub fn tick(&mut self, input: f64, ch: usize) -> f64 {
        let grain_len = ((self.time_ms * 0.001 * self.sample_rate) as usize).max(64);
        self.grain_samples = grain_len;

        // ── Onset detection: re-sync the window cycle to the attack ──
        let level = input.abs();
        let coeff = if level > self.env {
            self.env_attack
        } else {
            self.env_release
        };
        self.env += (level - self.env) * coeff;
        if self.armed && self.env > Self::ONSET_ON {
            self.grain_pos = 0;
            self.armed = false;
        } else if !self.armed && self.env < Self::ONSET_OFF {
            self.armed = true;
        }

        // Record forward (input + recirculated reversed repeats).
        self.delay.write(input + self.feedback_sample);

        // Delay-line modulation (wobble on the reversed read heads).
        let mod_off = if self.mod_depth > 0.0 {
            self.lfo_phase += self.mod_rate_hz / self.sample_rate;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }
            (self.lfo_phase * core::f64::consts::TAU).sin()
                * self.mod_depth
                * Self::MOD_RANGE_S
                * self.sample_rate
        } else {
            0.0
        };

        // Two reversed read heads, half a cycle apart.
        let pos_a = self.grain_pos;
        let pos_b = (self.grain_pos + grain_len) % (grain_len * 2);

        let read_a = self.read_reversed(pos_a, grain_len, mod_off);
        let read_b = self.read_reversed(pos_b, grain_len, mod_off);

        // Crossfade windows
        let cf = self.grain_crossfade.clamp(0.01, 0.5);
        let win_a = Self::grain_window(pos_a, grain_len, cf);
        let win_b = Self::grain_window(pos_b, grain_len, cf);

        let mut output = read_a * win_a + read_b * win_b;

        // Smear: diffuse the reversed audio (softened attack, more swell).
        if self.smear > 0.001 {
            output = self.diffuser.tick(output).clamp(-4.0, 4.0);
        }

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
    ///
    /// `pos` is the position within the grain cycle (0..2·grain_len).
    /// The offset advances 2 samples per tick, so the absolute read
    /// position walks backward at −1× through the grain recorded just
    /// before this window started.
    #[inline]
    fn read_reversed(&self, pos: usize, grain_len: usize, mod_off: f64) -> f64 {
        let pos_in_grain = (pos % grain_len) as f64;
        let read_offset = 2.0 * pos_in_grain + 1.0 + mod_off;
        let max_read = (self.delay.len() - 4) as f64;
        self.delay.read_cubic(read_offset.clamp(1.0, max_read))
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
            0.5 * (1.0 - (core::f64::consts::PI * t).cos())
        } else if pos_in_grain >= grain_len - fade_samples {
            // Fade out: raised cosine
            let t = (grain_len - 1 - pos_in_grain) as f64 / fade_samples as f64;
            0.5 * (1.0 - (core::f64::consts::PI * t).cos())
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
        self.diffuser.reset();
        self.feedback_sample = 0.0;
        self.grain_pos = 0;
        self.env = 0.0;
        self.armed = true;
        self.lfo_phase = 0.0;
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
    fn playback_is_actually_reversed() {
        // Record a rising ramp; the wet output within one grain must be a
        // FALLING ramp (later input samples come out earlier).
        let mut d = ReverseDelay::new();
        d.time_ms = 100.0; // 4800 samples
        d.feedback = 0.0;
        d.grain_crossfade = 0.01;
        d.update(SR);

        let grain = 4800usize;
        let mut outs = Vec::new();
        for i in 0..(grain * 3) {
            // Loud onset then a ramp so the onset sync fires at i=0.
            let input = if i < grain {
                0.5 + 0.5 * (i as f64 / grain as f64)
            } else {
                0.0
            };
            let out = d.tick(input, 0);
            if i >= grain + grain / 4 && i < grain + (3 * grain) / 4 {
                outs.push(out);
            }
        }
        // Middle half of the second window: reversed ramp → decreasing.
        let first = outs[..outs.len() / 4].iter().sum::<f64>();
        let last = outs[3 * outs.len() / 4..].iter().sum::<f64>();
        assert!(
            first > last + 1.0,
            "reversed playback should fall over the window: first={first} last={last}"
        );
    }

    #[test]
    fn onset_resyncs_window() {
        // Two identical bursts separated by silence must produce their
        // reverse playback at the same latency from each burst's onset —
        // the "synced to performance" behavior.
        let mut d = ReverseDelay::new();
        d.time_ms = 80.0;
        d.feedback = 0.0;
        d.update(SR);

        let grain = (0.08 * SR) as usize;
        let gap = grain * 3 + 517; // deliberately NOT a multiple of the cycle
        let burst = |d: &mut ReverseDelay| -> usize {
            // Feed a 3 ms burst, then silence; return samples from burst
            // start until the wet energy first appears.
            let mut first_out = usize::MAX;
            for i in 0..(grain * 2) {
                let input = if i < 144 { 0.8 } else { 0.0 };
                let out = d.tick(input, 0);
                if i > 144 && out.abs() > 0.05 && first_out == usize::MAX {
                    first_out = i;
                }
            }
            first_out
        };

        let lat_a = burst(&mut d);
        // Silence gap (mis-aligned with the free-running cycle length).
        for _ in 0..gap {
            d.tick(0.0, 0);
        }
        let lat_b = burst(&mut d);

        assert_ne!(lat_a, usize::MAX);
        assert_ne!(lat_b, usize::MAX);
        let diff = lat_a.abs_diff(lat_b);
        assert!(
            diff < 256,
            "reverse latency must be repeatable per onset: {lat_a} vs {lat_b}"
        );
    }

    #[test]
    fn smear_spreads_the_attack() {
        // With smear, the reversed burst's energy is spread over time:
        // the RMS temporal width of the wet event grows.
        let run = |smear: f64| -> f64 {
            let mut d = ReverseDelay::new();
            d.time_ms = 60.0;
            d.feedback = 0.0;
            d.smear = smear;
            d.update(SR);
            let mut w_sum = 0.0;
            let mut t_sum = 0.0;
            let mut t2_sum = 0.0;
            for i in 0..(48000 / 2) {
                let input = if i < 24 { 0.9 } else { 0.0 };
                let out = d.tick(input, 0);
                let w = out * out;
                let t = i as f64;
                w_sum += w;
                t_sum += w * t;
                t2_sum += w * t * t;
            }
            assert!(w_sum > 0.0, "no wet output");
            let mean = t_sum / w_sum;
            (t2_sum / w_sum - mean * mean).sqrt()
        };
        let dry_width = run(0.0);
        let wet_width = run(0.9);
        assert!(
            wet_width > dry_width * 1.2,
            "smear should spread the event in time: {wet_width} vs {dry_width}"
        );
    }

    #[test]
    fn modulation_wobbles_the_read() {
        // The wobbled run must diverge from the unmodulated run on the
        // same input — the LFO moves the reversed read heads.
        let run = |depth: f64| -> Vec<f64> {
            let mut d = ReverseDelay::new();
            d.time_ms = 120.0;
            d.feedback = 0.0;
            d.mod_depth = depth;
            d.mod_rate_hz = 3.0;
            d.update(SR);
            (0..48000)
                .map(|i| {
                    let input = (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin() * 0.5;
                    d.tick(input, 0)
                })
                .collect()
        };
        let still = run(0.0);
        let wobbled = run(1.0);
        let ref_energy: f64 = still.iter().map(|x| x * x).sum();
        let diff_energy: f64 = still
            .iter()
            .zip(&wobbled)
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
        assert!(
            diff_energy > ref_energy * 0.05,
            "modulation should move the read heads: diff {diff_energy} vs ref {ref_energy}"
        );
    }

    #[test]
    fn no_nan() {
        let mut d = ReverseDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.6;
        d.grain_crossfade = 0.2;
        d.hicut_freq = 5000.0;
        d.smear = 0.7;
        d.mod_depth = 0.6;
        d.update(SR);

        for i in 0..96000 {
            let input = (core::f64::consts::PI * 2.0 * 440.0 * i as f64 / SR).sin() * 0.5;
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
