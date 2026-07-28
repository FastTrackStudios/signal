//! ReverbDelay — the TimeLine MX bonus "Reverb" machine (Flint-inspired).
//!
//! Knob remap on this machine: TIME = **pre-delay** (2 ms–2.5 s),
//! REPEATS = **decay** (0.15 s → 40 s, infinite at the top), Mod
//! Speed/Depth = **tremolo on the wet signal**, FILTER = bandwidth in
//! the regeneration, GRIT = distortion both **into and after** the
//! reverb (the lofi broken-verb character).
//!
//! Core: pre-delay line → grit drive → two Schroeder input diffusers →
//! 4×4 Householder FDN with per-line one-pole damping and two gently
//! modulated line lengths (keeps the tail from going metallic). Each
//! engine instance is one channel; the chain runs L/R instances.

use crate::tilt::DecayTilt;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::one_pole::OnePoleLp;
use audiocore_dsp::smoothing::ParamSmoother;

/// Schroeder allpass over a fixed delay.
struct Allpass {
    line: DelayLine,
    delay: f64,
    g: f64,
}

impl Allpass {
    fn new(delay_samples: f64, g: f64) -> Self {
        Self {
            line: DelayLine::new(delay_samples as usize + 8),
            delay: delay_samples,
            g,
        }
    }

    fn resize(&mut self, delay_samples: f64) {
        let needed = delay_samples as usize + 8;
        if self.line.len() < needed {
            self.line = DelayLine::new(needed);
        }
        self.delay = delay_samples;
    }

    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        let delayed = self.line.read_linear(self.delay);
        let v = input - self.g * delayed;
        self.line.write(v);
        delayed + self.g * v
    }

    fn reset(&mut self) {
        self.line.clear();
    }
}

/// FDN line lengths in milliseconds — mutually detuned, ~36–50 ms.
const LINE_MS: [f64; 4] = [36.2, 40.7, 45.6, 50.1];
/// Input diffuser lengths (ms) and coefficient.
const DIFF_MS: [f64; 2] = [5.1, 7.9];
const DIFF_G: f64 = 0.62;

pub struct ReverbDelay {
    /// Pre-delay in ms (the machine's TIME knob, 2–2500).
    pub time_ms: f64,
    /// Decay (the REPEATS knob, 0.0–1.0): 0.15 s → 40 s, ≥0.97 infinite.
    pub feedback: f64,
    /// Regeneration bandwidth (FILTER): damping lowpass cutoff in Hz,
    /// 0 = open (≈14 kHz).
    pub hicut_freq: f64,
    /// Distortion into AND after the reverb (GRIT), 0.0–1.0.
    pub grit: f64,
    /// Wet tremolo rate in Hz (Mod Speed).
    pub trem_rate_hz: f64,
    /// Wet tremolo depth (Mod Depth), 0.0–1.0.
    pub trem_depth: f64,
    /// Decay EQ tilt (shared engine param; applied to the wet output).
    pub decay_tilt: f64,

    predelay: DelayLine,
    pre_smoother: ParamSmoother,
    diffusers: [Allpass; 2],
    lines: [DelayLine; 4],
    line_len: [f64; 4],
    damp: [OnePoleLp; 4],
    decay_tilt_eq: DecayTilt,
    line_g: f64,
    infinite: bool,
    damp_open: bool,
    trem_phase: f64,
    mod_phase: [f64; 2],
    feedback_sample: f64,
    sample_rate: f64,
}

impl Default for ReverbDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverbDelay {
    pub const MIN_TIME_MS: f64 = 2.0;
    pub const MAX_TIME_MS: f64 = 2500.0;
    const MAX_PRE_S: f64 = 2.6;
    /// Decay range mapped from the REPEATS knob.
    const MIN_RT60_S: f64 = 0.15;
    const MAX_RT60_S: f64 = 40.0;
    /// Line-length modulation: depth (ms) and rates (Hz) for lines 0/2.
    const LINE_MOD_MS: f64 = 0.18;
    const LINE_MOD_HZ: [f64; 2] = [0.61, 0.83];

    pub fn new() -> Self {
        let sr = 48000.0;
        let ms = |m: f64| m * sr / 1000.0;
        Self {
            time_ms: 20.0,
            feedback: 0.4,
            hicut_freq: 0.0,
            grit: 0.0,
            trem_rate_hz: 4.0,
            trem_depth: 0.0,
            decay_tilt: 0.0,
            predelay: DelayLine::new((sr * Self::MAX_PRE_S) as usize + 1024),
            pre_smoother: ParamSmoother::new(0.0),
            diffusers: [
                Allpass::new(ms(DIFF_MS[0]), DIFF_G),
                Allpass::new(ms(DIFF_MS[1]), DIFF_G),
            ],
            lines: core::array::from_fn(|i| DelayLine::new(ms(LINE_MS[i]) as usize + 64)),
            line_len: core::array::from_fn(|i| ms(LINE_MS[i])),
            damp: core::array::from_fn(|_| OnePoleLp::new(14000.0, sr)),
            decay_tilt_eq: DecayTilt::new(),
            line_g: 0.7,
            infinite: false,
            damp_open: true,
            trem_phase: 0.0,
            mod_phase: [0.0, 0.25],
            feedback_sample: 0.0,
            sample_rate: sr,
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.time_ms = self.time_ms.clamp(Self::MIN_TIME_MS, Self::MAX_TIME_MS);

        let pre_len = (sample_rate * Self::MAX_PRE_S) as usize + 1024;
        if self.predelay.len() < pre_len {
            self.predelay = DelayLine::new(pre_len);
        }
        let ms = |m: f64| m * sample_rate / 1000.0;
        for (i, ap) in self.diffusers.iter_mut().enumerate() {
            ap.resize(ms(DIFF_MS[i]));
        }
        for i in 0..4 {
            self.line_len[i] = ms(LINE_MS[i]);
            let needed = self.line_len[i] as usize + 64;
            if self.lines[i].len() < needed {
                self.lines[i] = DelayLine::new(needed);
            }
        }

        // REPEATS → RT60, exponential feel across the knob; the top of
        // the travel latches infinite (freeze pins feedback to 1.0).
        let decay = self.feedback.clamp(0.0, 1.0);
        self.infinite = decay >= 0.97;
        let rt60 =
            Self::MIN_RT60_S * (Self::MAX_RT60_S / Self::MIN_RT60_S).powf(decay);
        let mean_len_s = self.line_len.iter().sum::<f64>() / 4.0 / sample_rate;
        self.line_g = if self.infinite {
            1.0
        } else {
            10.0f64.powf(-3.0 * mean_len_s / rt60)
        };

        // FILTER = bandwidth in the regeneration. Open (= 0) sits at
        // ~14 kHz; infinite hold bypasses damping so the wash never
        // erodes while held.
        self.damp_open = self.infinite;
        let cutoff = if self.hicut_freq > 0.0 {
            self.hicut_freq.clamp(400.0, 16000.0)
        } else {
            14000.0
        };
        for d in &mut self.damp {
            d.set_cutoff(cutoff, sample_rate);
        }

        self.decay_tilt_eq.configure(self.decay_tilt, sample_rate);

        self.pre_smoother
            .set_time_seeded(0.15, sample_rate, self.time_ms * 0.001 * sample_rate);
    }

    #[inline]
    fn drive(x: f64, grit: f64) -> f64 {
        if grit <= 0.001 {
            return x;
        }
        let d = 1.0 + grit * 6.0;
        (x * d).tanh() / d.tanh()
    }

    pub fn tick(&mut self, input: f64, _ch: usize) -> f64 {
        // Pre-delay (smoothed against zipper on the TIME knob).
        self.predelay.write(input);
        self.pre_smoother
            .set_target(self.time_ms * 0.001 * self.sample_rate);
        let pre_pos = self
            .pre_smoother
            .tick()
            .clamp(1.0, self.predelay.len() as f64 - 4.0);
        let pre = self.predelay.read_cubic(pre_pos);

        // Grit INTO the reverb.
        let mut x = Self::drive(pre, self.grit);

        // Input diffusion.
        x = self.diffusers[0].tick(x);
        x = self.diffusers[1].tick(x);

        // FDN read (lines 0 and 2 gently modulated).
        let mut outs = [0.0f64; 4];
        for i in 0..4 {
            let mut len = self.line_len[i];
            if i == 0 || i == 2 {
                let m = i / 2;
                self.mod_phase[m] += Self::LINE_MOD_HZ[m] / self.sample_rate;
                if self.mod_phase[m] >= 1.0 {
                    self.mod_phase[m] -= 1.0;
                }
                len += (self.mod_phase[m] * core::f64::consts::TAU).sin()
                    * Self::LINE_MOD_MS
                    * self.sample_rate
                    / 1000.0;
            }
            let max = self.lines[i].len() as f64 - 4.0;
            outs[i] = self.lines[i].read_cubic(len.clamp(1.0, max));
        }

        // Damping inside the loop (bypassed while infinite).
        let mut damped = outs;
        if !self.damp_open {
            for i in 0..4 {
                damped[i] = self.damp[i].tick(damped[i]);
            }
        }

        // Householder feedback: y_i = x_i − (2/4)·Σx.
        let sum: f64 = damped.iter().sum();
        let inject = [1.0, -1.0, 1.0, -1.0];
        for i in 0..4 {
            let mixed = damped[i] - 0.5 * sum;
            self.lines[i].write(x * inject[i] * 0.5 + mixed * self.line_g);
        }

        // Wet tap: alternating signs decorrelate the line sum.
        let mut wet = (outs[0] - outs[1] + outs[2] - outs[3]) * 0.45;

        // Grit AFTER the reverb.
        wet = Self::drive(wet, self.grit);

        // Flint-style tremolo on the wet only.
        if self.trem_depth > 0.0 {
            self.trem_phase += self.trem_rate_hz / self.sample_rate;
            if self.trem_phase >= 1.0 {
                self.trem_phase -= 1.0;
            }
            let lfo = 0.5 + 0.5 * (self.trem_phase * core::f64::consts::TAU).sin();
            wet *= 1.0 - self.trem_depth.clamp(0.0, 1.0) * lfo;
        }

        wet = self.decay_tilt_eq.tick(wet, 0);

        self.feedback_sample = wet * self.line_g;
        wet
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.predelay.clear();
        self.pre_smoother.reset(0.0);
        for ap in &mut self.diffusers {
            ap.reset();
        }
        for l in &mut self.lines {
            l.clear();
        }
        for d in &mut self.damp {
            d.reset();
        }
        self.decay_tilt_eq.reset();
        self.trem_phase = 0.0;
        self.mod_phase = [0.0, 0.25];
        self.feedback_sample = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn impulse_response(d: &mut ReverbDelay, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| d.tick(if i == 0 { 1.0 } else { 0.0 }, 0))
            .collect()
    }

    #[test]
    fn silence_in_silence_out() {
        let mut d = ReverbDelay::new();
        d.feedback = 0.4;
        d.update(SR);
        for _ in 0..48000 {
            assert!(d.tick(0.0, 0).abs() < 1e-10);
        }
    }

    #[test]
    fn produces_a_tail() {
        let mut d = ReverbDelay::new();
        d.time_ms = 10.0;
        d.feedback = 0.5;
        d.update(SR);
        let out = impulse_response(&mut d, 48000);
        let early: f64 = out[2400..7200].iter().map(|x| x * x).sum();
        let late: f64 = out[24000..28800].iter().map(|x| x * x).sum();
        assert!(early > 1e-6, "no early reverb energy");
        assert!(late > 1e-9, "no tail energy");
        assert!(late < early, "tail should decay");
    }

    #[test]
    fn decay_knob_lengthens_the_tail() {
        let tail_energy = |decay: f64| -> f64 {
            let mut d = ReverbDelay::new();
            d.time_ms = 5.0;
            d.feedback = decay;
            d.update(SR);
            let out = impulse_response(&mut d, 96000);
            out[48000..].iter().map(|x| x * x).sum()
        };
        let short = tail_energy(0.2);
        let long = tail_energy(0.8);
        assert!(
            long > short * 10.0,
            "REPEATS should stretch the decay: {long} vs {short}"
        );
    }

    #[test]
    fn max_decay_is_infinite() {
        let mut d = ReverbDelay::new();
        d.time_ms = 5.0;
        d.feedback = 1.0;
        d.update(SR);
        // Excite, then measure two late windows a second apart.
        let out = impulse_response(&mut d, 4 * 48000);
        let w1: f64 = out[96000..120000].iter().map(|x| x * x).sum();
        let w2: f64 = out[168000..192000].iter().map(|x| x * x).sum();
        assert!(w1 > 1e-9);
        assert!(
            w2 > w1 * 0.5,
            "max decay should hold (near) indefinitely: {w2} vs {w1}"
        );
    }

    #[test]
    fn predelay_moves_the_onset() {
        let onset = |pre_ms: f64| -> usize {
            let mut d = ReverbDelay::new();
            d.time_ms = pre_ms;
            d.feedback = 0.4;
            d.update(SR);
            let out = impulse_response(&mut d, 96000);
            out.iter().position(|x| x.abs() > 1e-4).unwrap_or(usize::MAX)
        };
        let near = onset(2.0);
        let far = onset(400.0);
        let expected_gap = (398.0 * SR / 1000.0) as usize;
        assert!(
            far > near + expected_gap / 2,
            "pre-delay should move the reverb onset: {near} vs {far}"
        );
    }

    #[test]
    fn filter_darkens_the_regeneration() {
        let hf = |cutoff: f64| -> f64 {
            let mut d = ReverbDelay::new();
            d.time_ms = 5.0;
            d.feedback = 0.6;
            d.hicut_freq = cutoff;
            d.update(SR);
            let out = impulse_response(&mut d, 48000);
            out[12000..]
                .windows(2)
                .map(|w| (w[1] - w[0]) * (w[1] - w[0]))
                .sum()
        };
        let open = hf(0.0);
        let dark = hf(900.0);
        assert!(
            dark < open * 0.5,
            "FILTER should darken the tail: {dark} vs {open}"
        );
    }

    #[test]
    fn tremolo_modulates_the_wet() {
        let mut d = ReverbDelay::new();
        d.time_ms = 5.0;
        d.feedback = 0.9;
        d.trem_depth = 1.0;
        d.trem_rate_hz = 6.0;
        d.update(SR);
        // Steady input → the wet envelope must swing at the trem rate.
        let mut env_min = f64::MAX;
        let mut env_max = 0.0f64;
        let mut env = 0.0;
        for i in 0..96000 {
            let input = (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin() * 0.3;
            let out = d.tick(input, 0).abs();
            env += (out - env) * 0.002;
            if i > 48000 {
                env_min = env_min.min(env);
                env_max = env_max.max(env);
            }
        }
        assert!(
            env_max > env_min * 1.8,
            "tremolo should swing the wet level: {env_min}..{env_max}"
        );
    }

    #[test]
    fn grit_distorts() {
        let run = |grit: f64| -> Vec<f64> {
            let mut d = ReverbDelay::new();
            d.time_ms = 5.0;
            d.feedback = 0.5;
            d.grit = grit;
            d.update(SR);
            (0..24000)
                .map(|i| {
                    let input =
                        (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin() * 0.8;
                    d.tick(input, 0)
                })
                .collect()
        };
        let clean = run(0.0);
        let dirty = run(1.0);
        let ref_energy: f64 = clean.iter().map(|x| x * x).sum();
        let diff: f64 = clean
            .iter()
            .zip(&dirty)
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
        assert!(diff > ref_energy * 0.01, "grit should color: {diff}");
    }

    #[test]
    fn no_nan_heavy_settings() {
        let mut d = ReverbDelay::new();
        d.time_ms = 2500.0;
        d.feedback = 1.0;
        d.grit = 1.0;
        d.trem_depth = 1.0;
        d.hicut_freq = 500.0;
        d.decay_tilt = 0.7;
        d.update(SR);
        for i in 0..192000 {
            let input = (core::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.7;
            let out = d.tick(input, 0);
            assert!(out.is_finite(), "NaN at {i}");
            assert!(out.abs() < 50.0, "runaway at {i}: {out}");
        }
    }
}
