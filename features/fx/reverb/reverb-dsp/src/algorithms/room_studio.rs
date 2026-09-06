//! Studio reverb — treated, controlled acoustic space.
//!
//! Modeled after a professionally treated recording studio:
//!   - Moderate delay lines (medium room)
//!   - Controlled HF damping (acoustic treatment absorbs mids/highs)
//!   - Smooth, even ER (diffusers on walls)
//!   - Tight low end (bass trapping)
//!   - Neutral, transparent character
//!   - Quick density buildup (diffused surfaces)

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, ROOM_STUDIO_T60, ReverbAlgorithm, decay_to_t60, t60_shelf_targets};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap, sign_balance};
use crate::primitives::one_pole::Lp1;

const FDN_MOD_AP_COUNT: usize = 8;

/// Studio ER: smooth, even spacing — wall treatment scatters evenly, so
/// the gain decay is more uniform than the raw room's.
///
/// `(delay in samples at 48 kHz, gain)`, scaled by rate and size.
const ER_TAPS_L: [(f64, f64); 10] = [
    (53.0, 0.82),
    (109.0, 0.72),
    (163.0, 0.63),
    (223.0, 0.54),
    (281.0, 0.46),
    (347.0, 0.38),
    (419.0, 0.31),
    (491.0, 0.24),
    (569.0, 0.18),
    (647.0, 0.13),
];

/// The right channel's train, offset from the left for decorrelation.
const ER_TAPS_R: [(f64, f64); 10] = [
    (61.0, 0.82),
    (119.0, 0.72),
    (179.0, 0.63),
    (241.0, 0.54),
    (307.0, 0.46),
    (373.0, 0.38),
    (443.0, 0.31),
    (517.0, 0.24),
    (593.0, 0.18),
    (673.0, 0.13),
];

pub struct RoomStudio {
    er_l: MultitapDelay,
    er_r: MultitapDelay,
    er_level: f64,

    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,

    fdn_l: Fdn,
    fdn_r: Fdn,

    mod_ap_l: [ModulatedAllpass; FDN_MOD_AP_COUNT],
    mod_ap_r: [ModulatedAllpass; FDN_MOD_AP_COUNT],

    tone_lp_l: Lp1,
    tone_lp_r: Lp1,
    // Bass control (high-pass in feedback — bass trapping)
    bass_hp_l: Lp1,
    bass_hp_r: Lp1,

    sample_rate: f64,
    size: f64,
    late_level: f64,
    width: f64,
}

impl RoomStudio {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let max_er = num::f64_to_index(sample_rate * 0.06); // 60ms max ER

        let mod_ap_l = std::array::from_fn(|_| ModulatedAllpass::new());
        let mod_ap_r = std::array::from_fn(|_| ModulatedAllpass::new());

        let mut tone_lp_l = Lp1::new();
        tone_lp_l.set_freq(12000.0, sample_rate);
        let mut tone_lp_r = Lp1::new();
        tone_lp_r.set_freq(12000.0, sample_rate);

        // Bass trapping simulation
        let mut bass_hp_l = Lp1::new();
        bass_hp_l.set_freq(150.0, sample_rate);
        let mut bass_hp_r = Lp1::new();
        bass_hp_r.set_freq(150.0, sample_rate);

        let mut studio = Self {
            er_l: MultitapDelay::new(max_er),
            er_r: MultitapDelay::new(max_er),
            er_level: 0.45,
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.35),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.35),
            fdn_l: Self::make_fdn(sample_rate, 0.4, false),
            fdn_r: Self::make_fdn(sample_rate, 0.4, true),
            mod_ap_l,
            mod_ap_r,
            tone_lp_l,
            tone_lp_r,
            bass_hp_l,
            bass_hp_r,
            sample_rate,
            size: 0.4,
            late_level: 1.0,
            width: 1.0,
        };

        studio.setup_er_taps(0.4);
        studio.setup_mod_allpass(0.08);
        studio
    }

    fn make_fdn(sample_rate: f64, size: f64, offset: bool) -> Fdn {
        // Moderate delays — studio-sized room
        let base = if offset {
            [409, 509, 619, 743, 863, 991, 1123, 1259]
        } else {
            [389, 487, 601, 719, 839, 967, 1097, 1229]
        };
        let scale = sample_rate / 48000.0 * size.max(0.1);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| num::f64_to_index(f64::from(d) * scale).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.6);
        // More damping than chamber — acoustic treatment absorbs
        fdn.set_damping(5000.0, sample_rate);
        fdn
    }

    fn setup_er_taps(&mut self, size: f64) {
        let scale = self.sample_rate / 48000.0 * size.max(0.1);
        let tap = |(delay, gain): (f64, f64)| Tap {
            delay_samples: num::f64_to_index(delay * scale),
            gain,
        };
        let mut taps_l = ER_TAPS_L.map(tap);
        let mut taps_r = ER_TAPS_R.map(tap);
        sign_balance(&mut taps_l);
        sign_balance(&mut taps_r);
        self.er_l.set_taps(&taps_l);
        self.er_r.set_taps(&taps_r);
    }

    fn setup_mod_allpass(&mut self, modulation: f64) {
        let base_delays = [59, 79, 101, 127, 157, 191, 229, 269];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.1);

        for (i, ((ap_l, ap_r), &d)) in self.mod_ap_l
            .iter_mut()
            .zip(self.mod_ap_r.iter_mut())
            .zip(base_delays.iter())
            .enumerate()
        {
            let delay = num::f64_to_index(f64::from(d) * scale);
            ap_l.sample_delay = delay.max(4);
            ap_l.feedback = 0.3;
            ap_l.set_modulation(
                num::count_to_f64(i).mul_add(0.08, 0.25),
                modulation * self.sample_rate * 0.0002,
                self.sample_rate,
            );
            ap_l.set_phase(num::count_to_f64(i) / num::count_to_f64(FDN_MOD_AP_COUNT));

            let delay_r = num::f64_to_index((f64::from(d) + 11.0) * scale);
            ap_r.sample_delay = delay_r.max(4);
            ap_r.feedback = 0.3;
            ap_r.set_modulation(
                num::count_to_f64(i).mul_add(0.07, 0.3),
                modulation * self.sample_rate * 0.0002,
                self.sample_rate,
            );
            ap_r.set_phase((num::count_to_f64(i) + 0.5) / num::count_to_f64(FDN_MOD_AP_COUNT));
        }
    }

    fn rebuild_fdns(&mut self) {
        self.fdn_l = Self::make_fdn(self.sample_rate, self.size, false);
        self.fdn_r = Self::make_fdn(self.sample_rate, self.size, true);
    }
}

impl ReverbAlgorithm for RoomStudio {
    fn reset(&mut self) {
        self.er_l.reset();
        self.er_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.fdn_l.reset();
        self.fdn_r.reset();
        for ap in &mut self.mod_ap_l {
            ap.reset();
        }
        for ap in &mut self.mod_ap_r {
            ap.reset();
        }
        self.tone_lp_l.reset();
        self.tone_lp_r.reset();
        self.bass_hp_l.reset();
        self.bass_hp_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_vintage(&mut self, on: bool) -> bool {
        self.fdn_l.set_vintage_reads(on, self.sample_rate);
        self.fdn_r.set_vintage_reads(on, self.sample_rate);
        true
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Size — small to medium studio
        let new_size = params.size.mul_add(0.85, 0.15); // 0.15x to 1.0x
        if (new_size - self.size).abs() > 0.01 {
            self.size = new_size;
            self.rebuild_fdns();
            self.setup_er_taps(new_size);
            self.setup_mod_allpass(params.modulation);
        }

        // Decay — studios are controlled, shorter decay than live rooms
        // In-loop allpasses and a slow rotation of the feedback mix — the
        // density Hall has always had and this engine did not. It did not
        // matter while the old feedback-gain model capped the tail short:
        // once a treated studio room can actually ring for seconds, the sparse modal
        // structure sings, and an isolated mode stands ~36 dB above its
        // neighbours. Diffusing inside the loop builds density with every
        // recirculation instead of only at the input diffuser.
        self.fdn_l.set_loop_allpass(0.5);
        self.fdn_r.set_loop_allpass(0.5);
        self.fdn_l.set_rotation(
            params.modulation.mul_add(0.6, 0.3),
            params.modulation.mul_add(0.16, 0.04),
            self.sample_rate,
        );
        self.fdn_r.set_rotation(
            params.modulation.mul_add(0.6, 0.3) * 1.11,
            params.modulation.mul_add(0.16, 0.04),
            self.sample_rate,
        );

        // Exact per-line T60 decay (Jot shelf), the same model Hall uses.
        // This replaced a raw feedback gain, which is not a time: it capped
        // the reachable tail well short of a treated studio room and made
        // `decay` mean something different here than in every other engine.
        // Range covers a treated studio room — deliberately short.
        let t60 = decay_to_t60(params.decay, ROOM_STUDIO_T60.0, ROOM_STUDIO_T60.1);
        let (t60_dc, t60_ny) = t60_shelf_targets(
            t60,
            params.low_decay_mult,
            params.high_decay_mult,
            params.damping,
        );
        self.fdn_l.set_t60(t60_dc, t60_ny, self.sample_rate);
        self.fdn_r.set_t60(t60_dc, t60_ny, self.sample_rate);

        // Decay Rate EQ, realized in the feedback path — layered over the
        // shelf. Flat bands cost nothing.
        self.fdn_l
            .set_decay_curve(t60, &params.decay_bands, self.sample_rate);
        self.fdn_r
            .set_decay_curve(t60, &params.decay_bands, self.sample_rate);

        // Damping — acoustic treatment absorbs more consistently
        let damp_freq = (1.0 - params.damping).mul_add(8500.0, 1500.0);
        self.fdn_l.set_damping(damp_freq, self.sample_rate);
        self.fdn_r.set_damping(damp_freq, self.sample_rate);

        // Multi-band decay
        self.fdn_l.set_band_decay(
            params.band_crossover_hz,
            params.low_decay_mult,
            params.high_decay_mult,
            self.sample_rate,
        );
        self.fdn_r.set_band_decay(
            params.band_crossover_hz,
            params.low_decay_mult,
            params.high_decay_mult,
            self.sample_rate,
        );

        // Bass trapping — extra_a controls bass tightness
        let bass_freq = params.extra_a.mul_add(300.0, 80.0); // 80-380 Hz
        self.bass_hp_l.set_freq(bass_freq, self.sample_rate);
        self.bass_hp_r.set_freq(bass_freq, self.sample_rate);

        // Diffusion — studios have diffusers, so density builds fast
        let stages = num::f64_to_index(params.diffusion * 10.0);
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(params.diffusion.mul_add(0.2, 0.55));
        self.diffuser_r.set_feedback(params.diffusion.mul_add(0.2, 0.55));

        // Modulation (very subtle in studio)
        self.setup_mod_allpass(params.modulation);
        let diff_mod_depth = params.modulation * 2.0;
        self.diffuser_l
            .set_modulation(0.3, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.3, diff_mod_depth, self.sample_rate);

        // Tone
        let tone_freq = ((1.0 + params.tone) * 0.5).mul_add(8000.0, 4000.0);
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        // Extra B → stereo width
        self.width = params.extra_b;

        self.er_level = 0.45;
        self.late_level = 1.0;
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let er_l = self.er_l.tick(left) * self.er_level;
        let er_r = self.er_r.tick(right) * self.er_level;

        let diff_l = self.diffuser_l.tick(left);
        let diff_r = self.diffuser_r.tick(right);

        let mut late_l = self.fdn_l.tick(diff_l);
        let mut late_r = self.fdn_r.tick(diff_r);

        for (ap_l, ap_r) in self.mod_ap_l.iter_mut().zip(self.mod_ap_r.iter_mut()) {
            late_l = ap_l.tick(late_l);
            late_r = ap_r.tick(late_r);
        }

        // Bass trapping: subtract low-passed signal to remove bass energy
        let bass_l = self.bass_hp_l.tick(late_l);
        let bass_r = self.bass_hp_r.tick(late_r);
        late_l -= bass_l * 0.3; // Partial bass reduction
        late_r -= bass_r * 0.3;

        // Stereo width
        (late_l, late_r) = audiocore_dsp::stereo::width(late_l, late_r, self.width);

        // Tone
        late_l = self.tone_lp_l.tick(late_l) * self.late_level;
        late_r = self.tone_lp_r.tick(late_r) * self.late_level;

        (er_l + late_l, er_r + late_r)
    }
}
