//! Non-Linear reverb — physics-defying decay shapes.
//!
//! Based on Strymon BigSky Non-Linear: applies envelope shaping
//! to a reverb tail, creating reverse, gated, swell, and ramp effects.

use crate::algorithm::{NlShape, AlgorithmParams, NonLinearParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use audiocore_dsp::delay_line::DelayLine;

/// Envelope shape for the reverb tail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvelopeShape {
    /// Bell-curve profile.
    Gauss,
    /// Inverted bell.
    Bounce,
    /// Reverse ramp — reverb builds to a peak then cuts.
    Reverse,
    /// Gate — full level then abrupt cutoff.
    Gate,
    /// Swoosh — exponential swell then quick decay.
    Swoosh,
    /// Ramp — linear rise then fall.
    Ramp,
}

pub struct NonLinear {
    // Reverb source
    fdn: Fdn,
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    // Envelope buffer — stores reverb output for reshaping
    env_buffer_l: DelayLine,
    env_buffer_r: DelayLine,
    env_length: usize,
    env_write_count: usize,
    // Shape
    shape: EnvelopeShape,
    // BigSky MX extras: Chop (amplitude mod on the decay), explicit
    // gate speed, and a separate Late reverb stage.
    mx: NonLinearParams,
    /// Chop LFO phase (0..1).
    chop_phase: f64,
    /// Late stage: its own long-tail FDN, level-gated behind
    /// `mx.late_level > 0` so the default costs nothing.
    late_fdn: Fdn,
    /// One-pole swell state for the late stage onset (late_speed).
    late_env: f64,
    /// PRE-DELAY remap: shaped output recirculated into the generator.
    nl_fb_state: f64,
    /// Swoosh voicing: a lowpass whose cutoff RISES through the window
    /// (dark → bright as the backward swell builds — the tap-bank
    /// per-tap filter idea mapped onto our envelope reader).
    swoosh_lp: crate::primitives::one_pole::Lp1,
    swoosh_countdown: u32,
    sample_rate: f64,
}

impl NonLinear {
    pub fn new(sample_rate: f64) -> Self {
        let max_env = (sample_rate * 2.0) as usize; // 2s max envelope

        Self {
            fdn: Self::make_fdn(sample_rate),
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.6),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.6),
            env_buffer_l: DelayLine::new(max_env + 1),
            env_buffer_r: DelayLine::new(max_env + 1),
            env_length: (sample_rate * 0.5) as usize,
            env_write_count: 0,
            shape: EnvelopeShape::Reverse,
            mx: NonLinearParams::default(),
            chop_phase: 0.0,
            late_fdn: Self::make_late_fdn(sample_rate),
            late_env: 0.0,
            nl_fb_state: 0.0,
            swoosh_lp: crate::primitives::one_pole::Lp1::new(),
            swoosh_countdown: 0,
            sample_rate,
        }
    }

    fn make_fdn(sample_rate: f64) -> Fdn {
        let base = [743, 941, 1163, 1399, 1627, 1861, 2083, 2311];
        let scale = sample_rate / 48000.0;
        let delays: Vec<usize> = base.iter().map(|&d| (d as f64 * scale) as usize).collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.95); // Long decay — envelope does the shaping
        fdn
    }

    /// The Late stage's own tank — different primes so it doesn't
    /// phase-lock with the main burst FDN.
    fn make_late_fdn(sample_rate: f64) -> Fdn {
        let base = [809, 1021, 1249, 1481, 1693, 1931, 2143, 2399];
        let scale = sample_rate / 48000.0;
        let delays: Vec<usize> = base.iter().map(|&d| (d as f64 * scale) as usize).collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.9);
        fdn
    }

    /// Compute envelope gain for current position.
    #[inline]
    fn envelope_gain(&self, position: f64) -> f64 {
        match self.shape {
            EnvelopeShape::Reverse => {
                // Ramp up linearly then cut
                position
            }
            EnvelopeShape::Gate => {
                // Even profile with an ABRUPT cut (RMX16 lineage): full
                // level until the hold point, then a short half-cosine
                // knee (~8% of the window) instead of a linear fade —
                // reads as a gate, not a decay, without clicking.
                let hold = 0.5 + 0.4 * self.mx.gate_speed.clamp(0.0, 1.0);
                const KNEE: f64 = 0.08;
                if position < hold {
                    1.0
                } else if position < hold + KNEE {
                    0.5 * (1.0
                        + (std::f64::consts::PI * (position - hold) / KNEE).cos())
                } else {
                    0.0
                }
            }
            EnvelopeShape::Swoosh => {
                // Exponential swell
                (position * 3.0).min(1.0_f64).powi(2) * (1.0 - position).max(0.0).sqrt()
            }
            EnvelopeShape::Ramp => {
                // Triangle
                if position < 0.5 {
                    position * 2.0
                } else {
                    (1.0 - position) * 2.0
                }
            }
            EnvelopeShape::Gauss => {
                // Bell curve centered mid-window.
                let d = position - 0.5;
                (-d * d / (2.0 * 0.17 * 0.17)).exp()
            }
            EnvelopeShape::Bounce => {
                // Inverted bell: full at the edges, dipped mid-window.
                let d = position - 0.5;
                1.0 - 0.92 * (-d * d / (2.0 * 0.15 * 0.15)).exp()
            }
        }
    }
}

impl ReverbAlgorithm for NonLinear {
    fn reset(&mut self) {
        self.fdn.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.env_buffer_l.clear();
        self.env_buffer_r.clear();
        self.env_write_count = 0;
        self.chop_phase = 0.0;
        self.late_fdn.reset();
        self.late_env = 0.0;
        self.nl_fb_state = 0.0;
        self.swoosh_lp.reset();
        self.swoosh_countdown = 0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        let mx = self.mx;
        *self = Self::new(sample_rate);
        self.set_nonlinear_params(&mx);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Knob remap (manual): DECAY sets the time of the NONLINEAR
        // portion (the shaped-envelope window).
        self.env_length = ((0.1 + params.decay * 1.9) * self.sample_rate) as usize;

        // Shape: the named selector wins; without it fall back to the
        // legacy extra_a thresholds.
        self.shape = match self.mx.shape {
            Some(NlShape::Swoosh) => EnvelopeShape::Swoosh,
            Some(NlShape::Reverse) => EnvelopeShape::Reverse,
            Some(NlShape::Ramp) => EnvelopeShape::Ramp,
            Some(NlShape::Gate) => EnvelopeShape::Gate,
            Some(NlShape::Gauss) => EnvelopeShape::Gauss,
            Some(NlShape::Bounce) => EnvelopeShape::Bounce,
            None => {
                if params.extra_a < 0.25 {
                    EnvelopeShape::Reverse
                } else if params.extra_a < 0.5 {
                    EnvelopeShape::Gate
                } else if params.extra_a < 0.75 {
                    EnvelopeShape::Swoosh
                } else {
                    EnvelopeShape::Ramp
                }
            }
        };

        // Diffusion
        let stages = (params.diffusion * 8.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.5 + params.diffusion * 0.2);
        self.diffuser_r.set_feedback(0.5 + params.diffusion * 0.2);

        // Damping on FDN
        let damp_coeff = params.damping * 0.5;
        self.fdn.set_damping_coeff(damp_coeff);

        // Decay (internal reverb)
        self.fdn.set_decay(0.7 + params.decay * 0.28);
    }

    fn set_nonlinear_params(&mut self, params: &NonLinearParams) -> bool {
        self.mx = *params;
        // late_decay 0..1 → late tank decay 0.7..0.98 (same span as the
        // shared decay mapping).
        self.late_fdn
            .set_decay(0.7 + params.late_decay.clamp(0.0, 1.0) * 0.28);
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // PRE-DELAY remap: shaped nonlinear output feeds back into the
        // generator input (before the late stage) — repeating shapes.
        let fb = self.mx.feedback.clamp(0.0, 1.0) * 0.9;
        let input = (left + right) * 0.5 + self.nl_fb_state * fb;

        // Generate dense reverb
        let diff = self.diffuser_l.tick(input);
        let reverbed = self.fdn.tick(diff);

        // Store in envelope buffer
        self.env_buffer_l.write(reverbed);
        self.env_buffer_r.write(self.diffuser_r.tick(reverbed));

        // Read back with envelope shaping
        let env_len = self.env_length.max(1);
        let position = (self.env_write_count % env_len) as f64 / env_len as f64;
        let gain = self.envelope_gain(position);

        let mut out_l = self.env_buffer_l.read(1) * gain;
        let mut out_r = self.env_buffer_r.read(1) * gain;

        // Swoosh: rising lowpass across the window (400 Hz → 8 kHz).
        if self.shape == EnvelopeShape::Swoosh {
            if self.swoosh_countdown == 0 {
                self.swoosh_countdown = 64;
                let cutoff = 400.0 * (8000.0f64 / 400.0).powf(position.clamp(0.0, 1.0));
                self.swoosh_lp.set_freq(cutoff, self.sample_rate);
            }
            self.swoosh_countdown -= 1;
            out_l = self.swoosh_lp.tick(out_l);
            out_r = self.swoosh_lp.tick(out_r);
        }

        // ── Chop: amplitude modulation on the decay (BigSky MX) ─────
        // Raised-cosine tremolo; depth 0 is bit-transparent.
        let chop_depth = self.mx.chop_depth.clamp(0.0, 1.0);
        if chop_depth > 1e-9 {
            let trem =
                1.0 - chop_depth * (0.5 - 0.5 * (self.chop_phase * std::f64::consts::TAU).cos());
            out_l *= trem;
            out_r *= trem;
            self.chop_phase += self.mx.chop_rate_hz.clamp(0.05, 20.0) / self.sample_rate;
            if self.chop_phase >= 1.0 {
                self.chop_phase -= 1.0;
            }
        }

        // ── Late stage: separate conventional tail after the burst ──
        // interpretation: Late Level blends in a second, unshaped
        // reverb tank; Late Speed sets how fast it swells in after the
        // note (one-pole onset, 0 = ~500 ms swell, 1 = immediate);
        // Late Decay is its own decay time.
        let late_level = self.mx.late_level.clamp(0.0, 1.0);
        if late_level > 1e-9 {
            // Swell envelope on the tank INPUT (the tail then rings out
            // per late_decay): rises while input energy is present, at
            // the late_speed-controlled rate.
            let speed = self.mx.late_speed.clamp(0.0, 1.0);
            let attack_s = 0.5 - 0.49 * speed; // 500 ms .. 10 ms
            let coeff = 1.0 - (-1.0 / (attack_s * self.sample_rate)).exp();
            let drive = (input.abs() * 4.0).min(1.0);
            self.late_env += (drive - self.late_env) * coeff.min(1.0);
            let late = self.late_fdn.tick(diff * self.late_env);
            out_l += late * late_level;
            out_r += late * late_level;
        }

        self.env_write_count = self.env_write_count.wrapping_add(1);
        self.nl_fb_state = ((out_l + out_r) * 0.5).clamp(-2.0, 2.0);

        (out_l, out_r)
    }
}
