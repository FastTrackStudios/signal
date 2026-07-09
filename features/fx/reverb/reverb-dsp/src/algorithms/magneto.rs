//! Magneto reverb — multi-head tape echo with diffusion crossover.
//!
//! Based on Strymon BigSky Magneto: simulates a multi-head tape
//! machine where the echoes are progressively diffused, blurring
//! the boundary between delay and reverb.

use crate::algorithm::{AlgorithmParams, MagnetoParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::delay_line::DelayLine;

/// Number of virtual tape heads.
const NUM_HEADS: usize = 4;

pub struct Magneto {
    // Main tape delay line (shared by all heads)
    tape_l: DelayLine,
    tape_r: DelayLine,
    // Per-head delay times
    head_delays: [usize; NUM_HEADS],
    head_gains: [f64; NUM_HEADS],
    // Progressive diffusion per head (more diffusion on later heads)
    head_diffusers: [AllpassDiffuser; NUM_HEADS],
    // Feedback path
    feedback: f64,
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
    pub fn new(sample_rate: f64) -> Self {
        let max_delay = (sample_rate * 1.5) as usize; // 1.5s max tape

        let head_diffusers = std::array::from_fn(|i| {
            let mut d = AllpassDiffuser::with_defaults(sample_rate, 0.3 + i as f64 * 0.2);
            d.set_active_stages(2 + i * 2); // Progressive diffusion
            d.set_feedback(0.5);
            d.set_modulation(0.5, 4.0, sample_rate);
            d
        });

        let base_delay = (sample_rate * 0.15) as usize;
        let mut magneto = Self {
            tape_l: DelayLine::new(max_delay + 1),
            tape_r: DelayLine::new(max_delay + 1),
            head_delays: [base_delay, base_delay * 2, base_delay * 3, base_delay * 4],
            head_gains: [0.8, 0.6, 0.4, 0.25],
            head_diffusers,
            feedback: 0.4,
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

    /// Soft tape saturation.
    #[inline]
    fn saturate(x: f64, amount: f64) -> f64 {
        if amount < 0.001 {
            return x;
        }
        let driven = x * (1.0 + amount * 2.0);
        driven / (1.0 + driven.abs())
    }
}

impl ReverbAlgorithm for Magneto {
    fn reset(&mut self) {
        self.tape_l.clear();
        self.tape_r.clear();
        for d in &mut self.head_diffusers {
            d.reset();
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
        // Size -> head spacing
        let base = (0.05 + params.size * 0.35) * self.sample_rate;
        for i in 0..NUM_HEADS {
            self.head_delays[i] = ((base * (i + 1) as f64) as usize)
                .min(self.tape_l.len() - 1)
                .max(1);
        }

        // Decay -> feedback
        self.feedback = 0.2 + params.decay * 0.6;

        // Diffusion -> how much each head is diffused
        for (i, diff) in self.head_diffusers.iter_mut().enumerate() {
            let stages = ((params.diffusion * (2.0 + i as f64 * 2.0)) as usize).min(8);
            diff.set_active_stages(stages);
            diff.set_feedback(0.4 + params.diffusion * 0.3);
        }

        // Damping
        let freq = 2000.0 + (1.0 - params.damping) * 8000.0;
        self.fb_damp_l.set_freq(freq, self.sample_rate);
        self.fb_damp_r.set_freq(freq, self.sample_rate);

        // Modulation
        for (i, diff) in self.head_diffusers.iter_mut().enumerate() {
            diff.set_modulation(
                0.3 + i as f64 * 0.2,
                params.modulation * 8.0,
                self.sample_rate,
            );
        }

        // Saturation (extra_a)
        self.saturation = params.extra_a;
    }

    fn set_magneto_params(&mut self, params: &MagnetoParams) -> bool {
        self.ping_pong = params.ping_pong;
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Write input + feedback to tape
        let in_l = Self::saturate(left + self.fb_state_l * self.feedback, self.saturation);
        let in_r = Self::saturate(right + self.fb_state_r * self.feedback, self.saturation);
        self.tape_l.write(in_l);
        self.tape_r.write(in_r);

        // Read from each head with progressive diffusion
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for i in 0..NUM_HEADS {
            let raw_l = self.tape_l.read(self.head_delays[i]);
            let raw_r = self.tape_r.read(self.head_delays[i]);

            // Later heads get more diffusion (blurring delay→reverb)
            let diff_l = self.head_diffusers[i].tick(raw_l);
            // Re-use same diffuser for R (slightly different phase from L input)
            let diff_r = self.head_diffusers[i].tick(raw_r);

            if self.ping_pong {
                // Alternate taps hard L/R: mono-sum the head, then pan
                // it fully to one side (√2 keeps perceived level ≈ the
                // centered dual-channel sum).
                let mono = (diff_l + diff_r) * 0.5 * std::f64::consts::SQRT_2;
                if i % 2 == 0 {
                    out_l += mono * self.head_gains[i];
                } else {
                    out_r += mono * self.head_gains[i];
                }
            } else {
                out_l += diff_l * self.head_gains[i];
                out_r += diff_r * self.head_gains[i];
            }
        }

        // Feedback from last head
        self.fb_state_l = self.fb_damp_l.tick(out_l);
        self.fb_state_r = self.fb_damp_r.tick(out_r);

        (out_l * 0.5, out_r * 0.5)
    }
}
