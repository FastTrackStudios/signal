//! Shimmer reverb — pitch-shifted feedback reverb.
//!
//! Based on CloudSeedCore + Strymon BigSky Shimmer concept:
//! A reverb tank with pitch-shifted signal fed back into the input,
//! creating evolving harmonic tails. Two independent pitch voices
//! (e.g., octave up + fifth up) can be blended.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::one_pole::Lp1;
use crate::primitives::pitch_shift::PitchShifter;

pub struct Shimmer {
    // Reverb tank
    fdn_l: Fdn,
    fdn_r: Fdn,
    // Input diffusion
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    // Pitch shifters in the feedback path
    shifter_l: PitchShifter,
    shifter_r: PitchShifter,
    // Feedback damping
    fb_damp_l: Lp1,
    fb_damp_r: Lp1,
    // Feedback state
    fb_l: f64,
    fb_r: f64,
    // Shimmer amount (how much pitch-shifted signal feeds back)
    shimmer_amount: f64,
    decay: f64,
    sample_rate: f64,
}

impl Shimmer {
    pub fn new(sample_rate: f64) -> Self {
        let grain_samples = (sample_rate * 0.05) as usize; // 50ms grains

        let mut shimmer = Self {
            fdn_l: Self::make_fdn(sample_rate, false),
            fdn_r: Self::make_fdn(sample_rate, true),
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.7),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.7),
            shifter_l: PitchShifter::new(grain_samples),
            shifter_r: PitchShifter::new(grain_samples),
            fb_damp_l: Lp1::new(),
            fb_damp_r: Lp1::new(),
            fb_l: 0.0,
            fb_r: 0.0,
            shimmer_amount: 0.5,
            decay: 0.8,
            sample_rate,
        };

        shimmer.shifter_l.set_speed(2.0); // Octave up
        shimmer.shifter_r.set_speed(2.0);
        shimmer.shifter_l.set_grain_ms(50.0, sample_rate);
        shimmer.shifter_r.set_grain_ms(50.0, sample_rate);
        shimmer.fb_damp_l.set_freq(6000.0, sample_rate);
        shimmer.fb_damp_r.set_freq(6000.0, sample_rate);

        shimmer
    }

    fn make_fdn(sample_rate: f64, offset: bool) -> Fdn {
        let base = if !offset {
            [1049, 1327, 1559, 1801, 2069, 2297, 2557, 2803]
        } else {
            [1117, 1381, 1613, 1873, 2131, 2371, 2617, 2879]
        };
        let scale = sample_rate / 48000.0;
        let delays: Vec<usize> = base.iter().map(|&d| (d as f64 * scale) as usize).collect();
        Fdn::new(&delays, MixMatrix::Householder)
    }
}

impl ReverbAlgorithm for Shimmer {
    fn reset(&mut self) {
        self.fdn_l.reset();
        self.fdn_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.shifter_l.reset();
        self.shifter_r.reset();
        self.fb_damp_l.reset();
        self.fb_damp_r.reset();
        self.fb_l = 0.0;
        self.fb_r = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay
        self.decay = 0.4 + params.decay * 0.55;
        self.fdn_l.set_decay(self.decay);
        self.fdn_r.set_decay(self.decay);

        // Damping
        let damp_coeff = params.damping * 0.5;
        self.fdn_l.set_damping_coeff(damp_coeff);
        self.fdn_r.set_damping_coeff(damp_coeff);

        // Shimmer amount (extra_a)
        self.shimmer_amount = params.extra_a * 0.7;

        // Pitch voice selection (extra_b)
        // 0.0 = octave up, 0.5 = fifth up, 1.0 = octave down
        let speed = if params.extra_b < 0.33 {
            2.0 // Octave up
        } else if params.extra_b < 0.66 {
            1.5 // Fifth up
        } else {
            0.5 // Octave down
        };
        self.shifter_l.set_speed(speed);
        self.shifter_r.set_speed(speed);

        // Modulation
        self.diffuser_l
            .set_modulation(0.8, params.modulation * 10.0, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.8, params.modulation * 10.0, self.sample_rate);

        // Diffusion
        let stages = (params.diffusion * 8.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);

        // Feedback damping
        let freq = 3000.0 + (1.0 - params.damping) * 8000.0;
        self.fb_damp_l.set_freq(freq, self.sample_rate);
        self.fb_damp_r.set_freq(freq, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Mix input with pitch-shifted feedback
        let in_l = left + self.fb_l * self.shimmer_amount;
        let in_r = right + self.fb_r * self.shimmer_amount;

        // Diffuse
        let diff_l = self.diffuser_l.tick(in_l);
        let diff_r = self.diffuser_r.tick(in_r);

        // FDN reverb
        let wet_l = self.fdn_l.tick(diff_l);
        let wet_r = self.fdn_r.tick(diff_r);

        // Pitch shift the reverb output for feedback
        let shifted_l = self.shifter_l.tick(wet_l);
        let shifted_r = self.shifter_r.tick(wet_r);

        // Damp and store for next iteration
        self.fb_l = self.fb_damp_l.tick(shifted_l);
        self.fb_r = self.fb_damp_r.tick(shifted_r);

        (wet_l, wet_r)
    }
}
