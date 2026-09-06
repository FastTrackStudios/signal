//! Magneto reverb — multi-head tape echo with diffusion crossover.
//!
//! Based on Strymon `BigSky` Magneto: simulates a multi-head tape
//! machine where the echoes are progressively diffused, blurring
//! the boundary between delay and reverb.

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, MagnetoParams, MagnetoSpacing, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::delay_line::DelayLine;

/// Maximum virtual tape heads (menu: 1 / 2 / 3 / 4 / 6).
const NUM_HEADS: usize = 6;
/// Uneven spacing: irregular head positions as fractions of the
/// last-head delay ("more complex, less overtly rhythmic").
const UNEVEN_POS: [f64; 6] = [0.14, 0.27, 0.43, 0.58, 0.81, 1.0];
/// Per-head output gains (progressively quieter).
const HEAD_GAINS: [f64; 6] = [0.8, 0.6, 0.45, 0.35, 0.28, 0.22];

/// One virtual tape head. Three parallel `[_; NUM_HEADS]` arrays before,
/// all walked by the same index.
struct Head {
    /// Read position on the shared tape, in samples.
    delay: usize,
    /// Output gain (later heads are progressively quieter).
    gain: f64,
    /// Per-head diffusion — later heads blur delay into reverb.
    diffuser: AllpassDiffuser,
}

pub struct Magneto {
    // Main tape delay line (shared by all heads)
    tape_l: DelayLine,
    tape_r: DelayLine,
    // Virtual tape heads, in order along the tape.
    heads: [Head; NUM_HEADS],
    // Feedback path (driven by the PRE-DELAY knob remap).
    feedback: f64,
    /// Active head count (1/2/3/4/6).
    active_heads: usize,
    spacing: MagnetoSpacing,
    /// Last-head delay in seconds (DECAY knob remap, up to 1.5 s).
    last_delay_s: f64,
    fb_damp_l: Lp1,
    fb_damp_r: Lp1,
    fb_state_l: f64,
    fb_state_r: f64,
    // Tape saturation
    saturation: f64,
    // BigSky MX Ping Pong: heads alternate hard L/R (odd heads left,
    // even heads right) — width + center clarity.
    ping_pong: bool,
    sample_rate: f64,
}

impl Magneto {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let max_delay = num::f64_to_index(sample_rate * 1.5); // 1.5s max tape

        let base_delay = num::f64_to_index(sample_rate * 0.15);
        let heads = std::array::from_fn(|i| {
            let mut diffuser =
                AllpassDiffuser::with_defaults(sample_rate, num::count_to_f64(i).mul_add(0.2, 0.3));
            diffuser.set_active_stages(2usize.saturating_add(i.saturating_mul(2))); // Progressive diffusion
            diffuser.set_feedback(0.5);
            diffuser.set_modulation(0.5, 4.0, sample_rate);
            Head {
                delay: base_delay.saturating_mul(i.saturating_add(1)),
                gain: HEAD_GAINS.get(i).copied().unwrap_or(0.0),
                diffuser,
            }
        });
        let mut magneto = Self {
            tape_l: DelayLine::new(max_delay.saturating_add(1)),
            tape_r: DelayLine::new(max_delay.saturating_add(1)),
            heads,
            feedback: 0.4,
            active_heads: 4,
            spacing: MagnetoSpacing::Even,
            last_delay_s: 0.6,
            fb_damp_l: Lp1::new(),
            fb_damp_r: Lp1::new(),
            fb_state_l: 0.0,
            fb_state_r: 0.0,
            saturation: 0.3,
            ping_pong: false,
            sample_rate,
        };

        magneto.fb_damp_l.set_freq(4000.0, sample_rate);
        magneto.fb_damp_r.set_freq(4000.0, sample_rate);

        magneto
    }

    /// Lay the active heads along the tape per the spacing mode; the
    /// last active head always lands at `last_delay_s`.
    fn reposition_heads(&mut self) {
        let n = self.active_heads.clamp(1, NUM_HEADS);
        let last = (self.last_delay_s * self.sample_rate)
            .min(num::count_to_f64(self.tape_l.len().saturating_sub(1)))
            .max(1.0);
        for (i, head) in self.heads.iter_mut().enumerate() {
            let frac = match self.spacing {
                MagnetoSpacing::Even => num::count_to_f64(i.saturating_add(1)) / num::count_to_f64(n),
                MagnetoSpacing::Uneven => {
                    // Take the last n entries of the irregular grid so
                    // the final head stays at the full delay time.
                    let idx = NUM_HEADS
                        .saturating_sub(n)
                        .saturating_add(i.min(n.saturating_sub(1)))
                        .min(NUM_HEADS.saturating_sub(1));
                    UNEVEN_POS.get(idx).copied().unwrap_or(1.0)
                }
            };
            head.delay = num::f64_to_index(last * frac.min(1.0)).max(1);
        }
    }

    /// Soft tape saturation.
    #[inline]
    fn saturate(x: f64, amount: f64) -> f64 {
        if amount < 0.001 {
            return x;
        }
        let driven = x * amount.mul_add(2.0, 1.0);
        driven / (1.0 + driven.abs())
    }
}

impl ReverbAlgorithm for Magneto {
    fn reset(&mut self) {
        self.tape_l.clear();
        self.tape_r.clear();
        for head in &mut self.heads {
            head.diffuser.reset();
        }
        self.fb_damp_l.reset();
        self.fb_damp_r.reset();
        self.fb_state_l = 0.0;
        self.fb_state_r = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Knob remap (manual): DECAY -> delay time of the LAST head, up
        // to 1500 ms. Feedback comes from MagnetoParams (the PRE-DELAY
        // remap), not from decay.
        self.last_delay_s = params.decay.mul_add(1.4, 0.1);
        self.reposition_heads();

        // Diffusion -> how much each head is diffused
        for (i, head) in self.heads.iter_mut().enumerate() {
            let diff = &mut head.diffuser;
            let stages = num::f64_to_index(params.diffusion * num::count_to_f64(i).mul_add(2.0, 2.0)).min(8);
            diff.set_active_stages(stages);
            diff.set_feedback(params.diffusion.mul_add(0.3, 0.4));
        }

        // Damping
        let freq = (1.0 - params.damping).mul_add(8000.0, 2000.0);
        self.fb_damp_l.set_freq(freq, self.sample_rate);
        self.fb_damp_r.set_freq(freq, self.sample_rate);

        // Modulation
        for (i, head) in self.heads.iter_mut().enumerate() {
            head.diffuser.set_modulation(
                num::count_to_f64(i).mul_add(0.2, 0.3),
                params.modulation * 8.0,
                self.sample_rate,
            );
        }

        // Saturation (extra_a)
        self.saturation = params.extra_a;
    }

    fn set_magneto_params(&mut self, params: &MagnetoParams) -> bool {
        self.ping_pong = params.ping_pong;
        self.active_heads = params.heads.count();
        self.spacing = params.spacing;
        self.feedback = params.feedback.clamp(0.0, 1.0) * 0.85;
        self.reposition_heads();
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Write input + feedback to tape
        let in_l = Self::saturate(self.fb_state_l.mul_add(self.feedback, left), self.saturation);
        let in_r = Self::saturate(self.fb_state_r.mul_add(self.feedback, right), self.saturation);
        self.tape_l.write(in_l);
        self.tape_r.write(in_r);

        // Read from each active head with progressive diffusion
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        let mut fb_l = 0.0;
        let mut fb_r = 0.0;
        let n = self.active_heads.clamp(1, NUM_HEADS);
        for (i, head) in self.heads.iter_mut().enumerate().take(n) {
            let raw_l = self.tape_l.read(head.delay);
            let raw_r = self.tape_r.read(head.delay);

            // Later heads get more diffusion (blurring delay→reverb)
            let diff_l = head.diffuser.tick(raw_l);
            // Re-use same diffuser for R (slightly different phase from L input)
            let diff_r = head.diffuser.tick(raw_r);

            if self.ping_pong {
                // Alternate taps hard L/R: mono-sum the head, then pan
                // it fully to one side (√2 keeps perceived level ≈ the
                // centered dual-channel sum).
                let mono = (diff_l + diff_r) * 0.5 * std::f64::consts::SQRT_2;
                if i % 2 == 0 {
                    out_l += mono * head.gain;
                } else {
                    out_r += mono * head.gain;
                }
            } else {
                out_l += diff_l * head.gain;
                out_r += diff_r * head.gain;
            }

            // Feedback tap: last head with Even spacing, last TWO
            // heads with Uneven (the manual's spacing side-effect).
            let takes_fb =
                i.saturating_add(1) == n || (self.spacing == MagnetoSpacing::Uneven && n >= 2 && i.saturating_add(2) == n);
            if takes_fb {
                fb_l += diff_l;
                fb_r += diff_r;
            }
        }

        self.fb_state_l = self.fb_damp_l.tick(fb_l);
        self.fb_state_r = self.fb_damp_r.tick(fb_r);

        (out_l * 0.5, out_r * 0.5)
    }
}
