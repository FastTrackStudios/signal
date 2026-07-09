//! Swell reverb — envelope-controlled reverb buildup.
//!
//! Based on Strymon BigSky Swell: the reverb gradually builds
//! behind the dry signal using an envelope follower to control
//! the reverb level. Creates pad-like textures that swell up
//! during sustained notes and fade during silence.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use audiocore_dsp::envelope::EnvelopeFollower;

pub struct Swell {
    // Reverb core
    fdn_l: Fdn,
    fdn_r: Fdn,
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    // Envelope follower for swell control
    env_follower: EnvelopeFollower,
    // Swell state
    swell_level: f64,
    swell_rate: f64, // How fast the reverb builds
    swell_target: f64,
    sample_rate: f64,
}

impl Swell {
    pub fn new(sample_rate: f64) -> Self {
        let mut env = EnvelopeFollower::new(0.0);
        env.set_times_ms(50.0, 500.0, sample_rate); // Slow attack, medium release

        Self {
            fdn_l: Self::make_fdn(sample_rate, false),
            fdn_r: Self::make_fdn(sample_rate, true),
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.8),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.8),
            env_follower: env,
            swell_level: 0.0,
            swell_rate: 0.0001,
            swell_target: 0.0,
            sample_rate,
        }
    }

    fn make_fdn(sample_rate: f64, offset: bool) -> Fdn {
        let base = if !offset {
            [1201, 1499, 1801, 2099, 2399, 2699, 2999, 3301]
        } else {
            [1279, 1567, 1873, 2179, 2473, 2777, 3079, 3389]
        };
        let scale = sample_rate / 48000.0;
        let delays: Vec<usize> = base.iter().map(|&d| (d as f64 * scale) as usize).collect();
        Fdn::new(&delays, MixMatrix::Householder)
    }
}

impl ReverbAlgorithm for Swell {
    fn reset(&mut self) {
        self.fdn_l.reset();
        self.fdn_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.env_follower.reset(0.0);
        self.swell_level = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay
        let decay = 0.5 + params.decay * 0.48;
        self.fdn_l.set_decay(decay);
        self.fdn_r.set_decay(decay);

        // Damping
        let damp_coeff = params.damping * 0.6;
        self.fdn_l.set_damping_coeff(damp_coeff);
        self.fdn_r.set_damping_coeff(damp_coeff);

        // Swell rate (extra_a: slow → fast build)
        self.swell_rate = 0.00001 + params.extra_a * 0.0005;

        // Envelope follower timing
        let attack_ms = 20.0 + (1.0 - params.extra_a) * 200.0;
        let release_ms = 100.0 + params.extra_b * 2000.0;
        self.env_follower
            .set_times_ms(attack_ms, release_ms, self.sample_rate);

        // Diffusion
        let stages = (params.diffusion * 10.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.5 + params.diffusion * 0.25);
        self.diffuser_r.set_feedback(0.5 + params.diffusion * 0.25);

        // Modulation
        self.diffuser_l
            .set_modulation(0.5, params.modulation * 10.0, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.5, params.modulation * 10.0, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Track input level
        let input_level = (left.abs() + right.abs()) * 0.5;
        let env = self.env_follower.tick(input_level);

        // Swell: build up reverb level when signal is present
        self.swell_target = env.min(1.0);
        if self.swell_level < self.swell_target {
            self.swell_level += self.swell_rate;
            if self.swell_level > self.swell_target {
                self.swell_level = self.swell_target;
            }
        } else {
            self.swell_level -= self.swell_rate * 0.5; // Slower release
            if self.swell_level < 0.0 {
                self.swell_level = 0.0;
            }
        }

        // Feed input into reverb
        let diff_l = self.diffuser_l.tick(left);
        let diff_r = self.diffuser_r.tick(right);

        let wet_l = self.fdn_l.tick(diff_l);
        let wet_r = self.fdn_r.tick(diff_r);

        // Apply swell envelope to output
        (wet_l * self.swell_level, wet_r * self.swell_level)
    }
}
