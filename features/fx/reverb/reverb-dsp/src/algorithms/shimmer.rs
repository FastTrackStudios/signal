//! Shimmer reverb — pitch-shifted feedback reverb.
//!
//! Based on CloudSeedCore + Strymon BigSky MX Shimmer:
//! A reverb tank with pitch-shifted signal fed back into the input,
//! creating evolving harmonic tails. Two independent pitch voices
//! (Shift 1 + Shift 2, each −oct..+oct) share a single Amount level,
//! and the voices' source is selectable (BigSky "Feedback" mode):
//! Input (one-pass shimmer, no laddering), Regenerative (shift inside
//! the loop → octave ladders), or both summed.

use crate::algorithm::{
    AlgorithmParams, ReverbAlgorithm, ShimmerFeedbackMode, ShimmerParams,
};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::grain_pitch::GrainPitchShifter;
use audiocore_dsp::one_pole::OnePoleHp;

pub struct Shimmer {
    // Reverb tank
    fdn_l: Fdn,
    fdn_r: Fdn,
    // Input diffusion
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    // Pitch voices (voice 1 always active, voice 2 optional)
    shifter1_l: GrainPitchShifter,
    shifter1_r: GrainPitchShifter,
    shifter2_l: GrainPitchShifter,
    shifter2_r: GrainPitchShifter,
    // Feedback damping
    fb_damp_l: Lp1,
    fb_damp_r: Lp1,
    // DC blockers — pitch-shifted feedback accumulates subsonic offset
    fb_dc_l: DcBlocker,
    fb_dc_r: DcBlocker,
    // Feedback state (shifted signal, injected next tick)
    fb_l: f64,
    fb_r: f64,
    // Subsonic cleanup on the wet output — the grain-window amplitude
    // modulation in the pitch loop leaves ~1.5% of IR energy below
    // 20 Hz without it (IR-metric verified).
    out_hp_l: OnePoleHp,
    out_hp_r: OnePoleHp,
    // Shimmer amount (how much pitch-shifted signal feeds back)
    shimmer_amount: f64,
    decay: f64,
    // MX param overlay (voice intervals / amount / feedback mode).
    mx: ShimmerParams,
    // Legacy coarse voice-1 speed from extra_b (used when
    // `mx.shift1_semitones` is None).
    legacy_speed: f64,
    // Legacy amount from extra_a (used when `mx.amount` is None).
    legacy_amount: f64,
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
            shifter1_l: GrainPitchShifter::new(grain_samples),
            shifter1_r: GrainPitchShifter::new(grain_samples),
            shifter2_l: GrainPitchShifter::new(grain_samples),
            shifter2_r: GrainPitchShifter::new(grain_samples),
            fb_damp_l: Lp1::new(),
            fb_damp_r: Lp1::new(),
            fb_dc_l: DcBlocker::new(),
            fb_dc_r: DcBlocker::new(),
            fb_l: 0.0,
            fb_r: 0.0,
            out_hp_l: OnePoleHp::new(24.0, sample_rate),
            out_hp_r: OnePoleHp::new(24.0, sample_rate),
            shimmer_amount: 0.5,
            decay: 0.8,
            mx: ShimmerParams::default(),
            legacy_speed: 2.0,
            legacy_amount: 0.5 * 0.7,
            sample_rate,
        };

        for s in [
            &mut shimmer.shifter1_l,
            &mut shimmer.shifter1_r,
            &mut shimmer.shifter2_l,
            &mut shimmer.shifter2_r,
        ] {
            s.set_speed(2.0); // Octave up
            s.set_grain_ms(50.0, sample_rate);
        }
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

    /// Re-derive shifter speeds / amount from the MX overlay + legacy
    /// mappings.
    fn apply_voice_config(&mut self) {
        let speed1 = match self.mx.shift1_semitones {
            Some(st) => semitones_to_speed(st),
            None => self.legacy_speed,
        };
        self.shifter1_l.set_speed(speed1);
        self.shifter1_r.set_speed(speed1);

        if self.mx.voice2 {
            let speed2 = semitones_to_speed(self.mx.shift2_semitones.unwrap_or(12.0));
            self.shifter2_l.set_speed(speed2);
            self.shifter2_r.set_speed(speed2);
        }

        self.shimmer_amount = self.mx.amount.map(|a| a.clamp(0.0, 1.0)).unwrap_or(self.legacy_amount);
    }
}

#[inline]
fn semitones_to_speed(st: f64) -> f64 {
    2f64.powf(st.clamp(-12.0, 12.0) / 12.0)
}

impl ReverbAlgorithm for Shimmer {
    fn reset(&mut self) {
        self.fdn_l.reset();
        self.fdn_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.shifter1_l.reset();
        self.shifter1_r.reset();
        self.shifter2_l.reset();
        self.shifter2_r.reset();
        self.fb_damp_l.reset();
        self.fb_damp_r.reset();
        self.fb_dc_l.reset();
        self.fb_dc_r.reset();
        self.out_hp_l.reset();
        self.out_hp_r.reset();
        self.fb_l = 0.0;
        self.fb_r = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        let mx = self.mx;
        *self = Self::new(sample_rate);
        self.mx = mx;
        self.apply_voice_config();
    }

    fn set_vintage(&mut self, on: bool) -> bool {
        self.fdn_l.set_vintage_reads(on, self.sample_rate);
        self.fdn_r.set_vintage_reads(on, self.sample_rate);
        true
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

        // Shimmer amount (extra_a) — legacy mapping, overridable by
        // the MX `amount` param.
        self.legacy_amount = params.extra_a * 0.7;

        // Legacy coarse pitch voice selection (extra_b)
        // 0.0 = octave up, 0.5 = fifth up, 1.0 = octave down
        self.legacy_speed = if params.extra_b < 0.33 {
            2.0 // Octave up
        } else if params.extra_b < 0.66 {
            1.5 // Fifth up
        } else {
            0.5 // Octave down
        };
        self.apply_voice_config();

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

    fn set_shimmer_params(&mut self, params: &ShimmerParams) -> bool {
        self.mx = *params;
        self.apply_voice_config();
        true
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

        // Pitch-voice source per the MX feedback mode. Regenerative
        // uses THIS tick's wet (legacy behavior, zero extra latency);
        // Input takes the dry input, InputPlusRegen sums both.
        let (src_l, src_r) = match self.mx.feedback_mode {
            ShimmerFeedbackMode::Regenerative => (wet_l, wet_r),
            ShimmerFeedbackMode::Input => (left, right),
            ShimmerFeedbackMode::InputPlusRegen => (left + wet_l, right + wet_r),
        };

        // Pitch shift for the injection path (both voices, shared level)
        let mut shifted_l = self.shifter1_l.tick(src_l);
        let mut shifted_r = self.shifter1_r.tick(src_r);
        if self.mx.voice2 {
            shifted_l += self.shifter2_l.tick(src_l);
            shifted_r += self.shifter2_r.tick(src_r);
        }

        // Block DC, damp, and store for next iteration
        self.fb_l = self.fb_damp_l.tick(self.fb_dc_l.tick(shifted_l));
        self.fb_r = self.fb_damp_r.tick(self.fb_dc_r.tick(shifted_r));

        (self.out_hp_l.tick(wet_l), self.out_hp_r.tick(wet_r))
    }
}
