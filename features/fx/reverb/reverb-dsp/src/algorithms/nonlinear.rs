//! Non-Linear reverb — physics-defying decay shapes.
//!
//! Based on Strymon BigSky Non-Linear: applies envelope shaping
//! to a reverb tail, creating reverse, gated, swell, and ramp effects.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use audiocore_dsp::delay_line::DelayLine;

/// Envelope shape for the reverb tail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvelopeShape {
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

    /// Compute envelope gain for current position.
    #[inline]
    fn envelope_gain(&self, position: f64) -> f64 {
        match self.shape {
            EnvelopeShape::Reverse => {
                // Ramp up linearly then cut
                position
            }
            EnvelopeShape::Gate => {
                // Full level, then hard cut at end
                if position < 0.9 {
                    1.0
                } else {
                    (1.0 - position) * 10.0
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
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Size -> envelope length
        self.env_length = ((0.1 + params.size * 1.9) * self.sample_rate) as usize;

        // Shape selection (extra_a: 0=reverse, 0.33=gate, 0.66=swoosh, 1.0=ramp)
        self.shape = if params.extra_a < 0.25 {
            EnvelopeShape::Reverse
        } else if params.extra_a < 0.5 {
            EnvelopeShape::Gate
        } else if params.extra_a < 0.75 {
            EnvelopeShape::Swoosh
        } else {
            EnvelopeShape::Ramp
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

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let input = (left + right) * 0.5;

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

        let out_l = self.env_buffer_l.read(1) * gain;
        let out_r = self.env_buffer_r.read(1) * gain;

        self.env_write_count = self.env_write_count.wrapping_add(1);

        (out_l, out_r)
    }
}
