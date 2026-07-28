//! Chorale reverb — vocal choir synthesis via formant-filtered pitch shifting.
//!
//! Based on Strymon BigSky Chorale: pitch-shifted reverb feedback
//! filtered through formant resonances to create vocal/choral textures.
//! Combines shimmer architecture with a formant filter bank.

use crate::algorithm::{ChoraleResonance, ChoraleVowel, AlgorithmParams, ChoirVoice, ChoraleParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::one_pole::OnePoleHp;
use audiocore_dsp::grain_pitch::GrainPitchShifter;

/// Vowel formant frequencies for "ah", "ee", "oh", "oo"
const VOWEL_FORMANTS: [[f64; 3]; 4] = [
    [800.0, 1200.0, 2500.0], // "ah"
    [300.0, 2300.0, 3000.0], // "ee"
    [500.0, 1000.0, 2500.0], // "oh"
    [300.0, 800.0, 2300.0],  // "oo"
];

pub struct Chorale {
    // Reverb core
    fdn_l: Fdn,
    fdn_r: Fdn,
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    // Pitch shifter
    shifter_l: GrainPitchShifter,
    shifter_r: GrainPitchShifter,
    // Formant filter bank (3 resonant peaks per channel)
    formants_l: [Biquad; 3],
    formants_r: [Biquad; 3],
    // Feedback — per-channel damping (a shared filter would smear L/R
    // state together) and DC blocking on the pitch-shifted loop.
    fb_damp_l: Lp1,
    fb_damp_r: Lp1,
    fb_dc_l: DcBlocker,
    fb_dc_r: DcBlocker,
    fb_l: f64,
    fb_r: f64,
    // Subsonic cleanup on the wet output — the grain-window amplitude
    // modulation in the pitch loop leaves ~1.5% of IR energy below
    // 20 Hz without it (IR-metric verified).
    out_hp_l: OnePoleHp,
    out_hp_r: OnePoleHp,
    chorale_amount: f64,
    vowel_mix: f64, // 0.0 = "ah", 1.0 = "oo"
    // Legacy choir level from extra_a (used when mx.choir_level is None).
    legacy_amount: f64,
    // BigSky MX Chorale params (choir level / voice range / per-voice
    // randomization).
    mx: ChoraleParams,
    // Randomization state: independent smoothed random walks per
    // channel drive shifter-speed vibrato and formant drift.
    // // interpretation: "more Mod = more distinct singers" — we
    // decorrelate the two channel voices' pitch + formant centers.
    rng: u32,
    rand_speed: [f64; 2],  // smoothed speed offset per channel
    rand_formant: [f64; 2], // smoothed formant-scale offset per channel
    rand_target_speed: [f64; 2],
    rand_target_formant: [f64; 2],
    ctrl_countdown: usize,
    /// Vowel-program morph phase (combination programs sweep slowly
    /// between their two vowels; Random walks the space).
    vowel_phase: f64,
    vowel_walk_target: f64,
    walk_countdown: usize,
    sample_rate: f64,
}

/// Control-rate block for the randomization (samples).
const CTRL_BLOCK: usize = 64;

impl Chorale {
    pub fn new(sample_rate: f64) -> Self {
        let grain = (sample_rate * 0.06) as usize;

        let mut chorale = Self {
            fdn_l: Self::make_fdn(sample_rate, false),
            fdn_r: Self::make_fdn(sample_rate, true),
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.7),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.7),
            shifter_l: GrainPitchShifter::new(grain),
            shifter_r: GrainPitchShifter::new(grain),
            formants_l: [Biquad::new(), Biquad::new(), Biquad::new()],
            formants_r: [Biquad::new(), Biquad::new(), Biquad::new()],
            fb_damp_l: Lp1::new(),
            fb_damp_r: Lp1::new(),
            fb_dc_l: DcBlocker::new(),
            fb_dc_r: DcBlocker::new(),
            fb_l: 0.0,
            fb_r: 0.0,
            out_hp_l: OnePoleHp::new(24.0, sample_rate),
            out_hp_r: OnePoleHp::new(24.0, sample_rate),
            chorale_amount: 0.5,
            vowel_mix: 0.0,
            legacy_amount: 0.5 * 0.6,
            mx: ChoraleParams::default(),
            rng: 0x9E37_79B9,
            rand_speed: [0.0; 2],
            rand_formant: [0.0; 2],
            rand_target_speed: [0.0; 2],
            rand_target_formant: [0.0; 2],
            ctrl_countdown: 0,
            vowel_phase: 0.0,
            vowel_walk_target: 0.5,
            walk_countdown: 0,
            sample_rate,
        };

        chorale.shifter_l.set_speed(2.0);
        chorale.shifter_r.set_speed(2.0);
        chorale.shifter_l.set_grain_ms(60.0, sample_rate);
        chorale.shifter_r.set_grain_ms(60.0, sample_rate);
        chorale.fb_damp_l.set_freq(5000.0, sample_rate);
        chorale.fb_damp_r.set_freq(5000.0, sample_rate);
        chorale.set_vowel(0.0, sample_rate);

        chorale
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

    fn set_vowel(&mut self, mix: f64, sample_rate: f64) {
        // Interpolate between vowels
        let idx = (mix * 3.0).min(2.999);
        let lo = idx as usize;
        let hi = (lo + 1).min(3);
        let frac = idx - lo as f64;

        // Choir Voice range: Baritone shifts the formant centers DOWN
        // (larger vocal tract, low chorale range — the pedal's second
        // range). Tenor = 1.0 keeps the legacy voicing.
        let voice_scale = match self.mx.voice {
            ChoirVoice::Tenor => 1.0,
            ChoirVoice::Baritone => 0.78,
        };

        #[allow(clippy::needless_range_loop)]
        for i in 0..3 {
            let base = VOWEL_FORMANTS[lo][i] * (1.0 - frac) + VOWEL_FORMANTS[hi][i] * frac;
            // Per-channel formant drift from the mod randomization.
            let freq_l = base * voice_scale * (1.0 + self.rand_formant[0]);
            let freq_r = base * voice_scale * (1.0 + self.rand_formant[1]);
            // +12 dB / Q 5 peaks inside the feedback loop pushed loop gain
            // near unity at the formant centers — a 33 dB isolated tail
            // mode (metallic ringing). Mild (8 dB / Q 3) keeps the vowel
            // color with the mode down (IR-metric verified); Medium/High
            // raise intensity but stay under that ceiling.
            let (q, gain_db) = self.mx.resonance.q_gain();
            self.formants_l[i].set(FilterType::Peak { gain_db }, freq_l, q, sample_rate);
            self.formants_r[i].set(FilterType::Peak { gain_db }, freq_r, q, sample_rate);
        }
    }

    /// Simple LCG in [-1, 1].
    #[inline]
    fn rand_bipolar(&mut self) -> f64 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.rng >> 8) as f64 / ((u32::MAX >> 8) as f64) * 2.0 - 1.0
    }

    /// Control-rate vowel-program update: static programs pin
    /// `vowel_mix`; combination programs sweep slowly (~0.07 Hz)
    /// between their two vowels; Random walks the whole space.
    /// Formant-space positions: ah = 0.0, ee = 1/3, oh = 2/3, oo = 1.0.
    fn update_vowel_program(&mut self) {
        let Some(program) = self.mx.vowel else {
            return; // legacy continuous morph via extra_b
        };
        let dt = CTRL_BLOCK as f64 / self.sample_rate;
        let sweep = |phase: f64, a: f64, b: f64| -> f64 {
            let x = 0.5 - 0.5 * (phase * std::f64::consts::TAU).cos();
            a + (b - a) * x
        };
        let target = match program {
            ChoraleVowel::Aahh => 0.0,
            ChoraleVowel::Oh => 2.0 / 3.0,
            ChoraleVowel::Ooo => 1.0,
            ChoraleVowel::Aahhoo => {
                self.vowel_phase = (self.vowel_phase + 0.07 * dt).fract();
                sweep(self.vowel_phase, 0.0, 1.0)
            }
            ChoraleVowel::Aahhoh => {
                self.vowel_phase = (self.vowel_phase + 0.07 * dt).fract();
                sweep(self.vowel_phase, 0.0, 2.0 / 3.0)
            }
            ChoraleVowel::Ooohoh => {
                self.vowel_phase = (self.vowel_phase + 0.07 * dt).fract();
                sweep(self.vowel_phase, 1.0, 2.0 / 3.0)
            }
            ChoraleVowel::Random => {
                // New random vowel target every ~1.2 s, glide toward it.
                self.vowel_phase += dt;
                if self.vowel_phase >= 1.2 {
                    self.vowel_phase = 0.0;
                    self.vowel_walk_target = (self.rand_bipolar() + 1.0) * 0.5;
                }
                self.vowel_mix + (self.vowel_walk_target - self.vowel_mix) * 0.06
            }
        };
        if (target - self.vowel_mix).abs() > 1e-4 {
            self.vowel_mix = target;
            self.set_vowel(target, self.sample_rate);
        }
    }

    /// Control-rate randomization update: smoothed random walks on the
    /// pitch-shifter speed (vibrato) and formant centers, independent
    /// per channel so the two "singers" drift apart as mod rises.
    fn update_randomization(&mut self) {
        let amount = self.mx.mod_amount.clamp(0.0, 1.0);
        if amount <= 1e-9 {
            return;
        }
        // New walk targets every ~80 ms.
        if self.walk_countdown == 0 {
            for ch in 0..2 {
                self.rand_target_speed[ch] = self.rand_bipolar() * 0.02 * amount;
                self.rand_target_formant[ch] = self.rand_bipolar() * 0.06 * amount;
            }
            self.walk_countdown = ((0.08 * self.sample_rate) as usize / CTRL_BLOCK).max(1);
        }
        self.walk_countdown -= 1;

        // One-pole toward the targets (smooth, click-free).
        for ch in 0..2 {
            self.rand_speed[ch] += (self.rand_target_speed[ch] - self.rand_speed[ch]) * 0.08;
            self.rand_formant[ch] +=
                (self.rand_target_formant[ch] - self.rand_formant[ch]) * 0.08;
        }

        self.shifter_l.set_speed(2.0 * (1.0 + self.rand_speed[0]));
        self.shifter_r.set_speed(2.0 * (1.0 + self.rand_speed[1]));
        self.set_vowel(self.vowel_mix, self.sample_rate);
    }
}

impl ReverbAlgorithm for Chorale {
    fn reset(&mut self) {
        self.fdn_l.reset();
        self.fdn_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.shifter_l.reset();
        self.shifter_r.reset();
        for f in &mut self.formants_l {
            f.reset();
        }
        for f in &mut self.formants_r {
            f.reset();
        }
        self.fb_damp_l.reset();
        self.fb_damp_r.reset();
        self.fb_dc_l.reset();
        self.fb_dc_r.reset();
        self.out_hp_l.reset();
        self.out_hp_r.reset();
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.ctrl_countdown = 0;
        self.walk_countdown = 0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        let mx = self.mx;
        *self = Self::new(sample_rate);
        self.set_chorale_params(&mx);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay
        let decay = 0.4 + params.decay * 0.55;
        self.fdn_l.set_decay(decay);
        self.fdn_r.set_decay(decay);

        // Damping
        let damp_coeff = params.damping * 0.5;
        self.fdn_l.set_damping_coeff(damp_coeff);
        self.fdn_r.set_damping_coeff(damp_coeff);

        // Chorale amount (extra_a) — legacy mapping, overridable by
        // the MX Choir level param.
        self.legacy_amount = params.extra_a * 0.6;
        self.chorale_amount = self
            .mx
            .choir_level
            .map(|v| v.clamp(0.0, 1.0) * 0.6)
            .unwrap_or(self.legacy_amount);

        // Vowel selection (extra_b: 0=ah, 0.33=ee, 0.66=oh, 1.0=oo)
        self.vowel_mix = params.extra_b;
        self.set_vowel(params.extra_b, self.sample_rate);

        // Diffusion
        let stages = (params.diffusion * 8.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);

        // Modulation
        self.diffuser_l
            .set_modulation(0.6, params.modulation * 10.0, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.6, params.modulation * 10.0, self.sample_rate);
    }

    fn set_chorale_params(&mut self, params: &ChoraleParams) -> bool {
        let voice_changed = params.voice != self.mx.voice;
        let mod_off = params.mod_amount <= 1e-9 && self.mx.mod_amount > 1e-9;
        self.mx = *params;
        self.chorale_amount = self
            .mx
            .choir_level
            .map(|v| v.clamp(0.0, 1.0) * 0.6)
            .unwrap_or(self.legacy_amount);
        if mod_off {
            // Land the randomized offsets back on neutral.
            self.rand_speed = [0.0; 2];
            self.rand_formant = [0.0; 2];
            self.rand_target_speed = [0.0; 2];
            self.rand_target_formant = [0.0; 2];
            self.shifter_l.set_speed(2.0);
            self.shifter_r.set_speed(2.0);
        }
        if voice_changed || mod_off {
            self.set_vowel(self.vowel_mix, self.sample_rate);
        }
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Control-rate updates: vowel program morph + per-voice
        // randomization (Mod).
        if self.ctrl_countdown == 0 {
            self.update_vowel_program();
            if self.mx.mod_amount > 1e-9 {
                self.update_randomization();
            }
            self.ctrl_countdown = CTRL_BLOCK;
        }
        self.ctrl_countdown -= 1;

        // Mix input with formant-filtered pitch-shifted feedback
        let in_l = left + self.fb_l * self.chorale_amount;
        let in_r = right + self.fb_r * self.chorale_amount;

        // Diffuse
        let diff_l = self.diffuser_l.tick(in_l);
        let diff_r = self.diffuser_r.tick(in_r);

        // FDN
        let wet_l = self.fdn_l.tick(diff_l);
        let wet_r = self.fdn_r.tick(diff_r);

        // Pitch shift
        let shifted_l = self.shifter_l.tick(wet_l);
        let shifted_r = self.shifter_r.tick(wet_r);

        // Formant filtering
        let mut vocal_l = shifted_l;
        let mut vocal_r = shifted_r;
        for f in &mut self.formants_l {
            vocal_l = f.tick(vocal_l, 0);
        }
        for f in &mut self.formants_r {
            vocal_r = f.tick(vocal_r, 1);
        }

        // Block DC, damp, and store feedback
        self.fb_l = self.fb_damp_l.tick(self.fb_dc_l.tick(vocal_l));
        self.fb_r = self.fb_damp_r.tick(self.fb_dc_r.tick(vocal_r));

        (self.out_hp_l.tick(wet_l), self.out_hp_r.tick(wet_r))
    }
}
