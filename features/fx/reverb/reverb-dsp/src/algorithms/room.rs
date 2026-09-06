//! Room reverb — small-to-medium acoustic space simulation.
//!
//! Architecture designed for natural room character:
//!   1. Early reflections (stereo multi-tap, image-source inspired geometry)
//!   2. Input diffusion (allpass cascade)
//!   3. Late reverb (8-line FDN with Householder mixing, per-line damping,
//!      modulated allpass in feedback path, size-scalable delay lengths)
//!   4. Output tone filtering
//!
//! Compared to Hall:
//!   - Shorter delay lines (small spaces)
//!   - More aggressive HF damping (room surfaces absorb more)
//!   - Tighter ER spacing (closer walls)
//!   - Less modulation (less chorus in tail)
//!   - Faster density buildup

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, ROOM_T60, ReverbAlgorithm, decay_to_t60, t60_shelf_targets};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::barr_loop::BarrLoop;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap, sign_balance};
use crate::primitives::one_pole::Lp1;

/// Number of modulated AP stages in the FDN feedback path.
const FDN_MOD_AP_COUNT: usize = 8;

/// Image-source inspired ER pattern for a medium rectangular room
/// (~5m x 4m x 3m). First-order wall reflections arrive first, then
/// corner and ceiling reflections with increasing density. The L and R
/// trains are offset for stereo decorrelation.
///
/// `(delay in samples at 48 kHz, gain)`, scaled by rate and size.
const ER_TAPS_L: [(f64, f64); 10] = [
    (67.0, 0.90), // near wall
    (131.0, 0.82), // side wall
    (197.0, 0.74), // far wall
    (281.0, 0.62), // wall-wall
    (353.0, 0.52),
    (443.0, 0.43),
    (557.0, 0.34), // floor/ceiling and higher-order
    (677.0, 0.26),
    (811.0, 0.19),
    (971.0, 0.13),
];

/// The right channel's train, offset from the left for decorrelation.
const ER_TAPS_R: [(f64, f64); 10] = [
    (79.0, 0.90),
    (149.0, 0.82),
    (223.0, 0.74),
    (307.0, 0.62),
    (389.0, 0.52),
    (479.0, 0.43),
    (593.0, 0.34),
    (719.0, 0.26),
    (859.0, 0.19),
    (1019.0, 0.13),
];

pub struct Room {
    // Early reflections (stereo)
    er_l: MultitapDelay,
    er_r: MultitapDelay,
    er_level: f64,

    // Input diffusion
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    /// Classic-voice late core: the Keith Barr single-loop ring
    /// (sparser, ringier, early-'80s). Swapped in by `set_vintage`.
    barr: BarrLoop,
    vintage: bool,
    /// Cached decay for the Barr ring's T60 mapping.
    last_decay: f64,

    // Late reverb — 8-line FDN per side
    fdn_l: Fdn,
    fdn_r: Fdn,

    // Modulated allpass in FDN feedback path (subtle chorus)
    mod_ap_l: [ModulatedAllpass; FDN_MOD_AP_COUNT],
    mod_ap_r: [ModulatedAllpass; FDN_MOD_AP_COUNT],

    // Tone control (output filtering)
    tone_lp_l: Lp1,
    tone_lp_r: Lp1,
    // High-frequency rolloff in late tail
    hf_damp_l: Lp1,
    hf_damp_r: Lp1,

    sample_rate: f64,
    size: f64,
    late_level: f64,
    width: f64,
}

impl Room {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let max_er = num::f64_to_index(sample_rate * 0.08); // 80ms max ER (rooms are smaller)

        let mod_ap_l = std::array::from_fn(|_| ModulatedAllpass::new());
        let mod_ap_r = std::array::from_fn(|_| ModulatedAllpass::new());

        let mut tone_lp_l = Lp1::new();
        tone_lp_l.set_freq(14000.0, sample_rate);
        let mut tone_lp_r = Lp1::new();
        tone_lp_r.set_freq(14000.0, sample_rate);

        let mut hf_damp_l = Lp1::new();
        hf_damp_l.set_freq(12000.0, sample_rate);
        let mut hf_damp_r = Lp1::new();
        hf_damp_r.set_freq(12000.0, sample_rate);

        let mut room = Self {
            er_l: MultitapDelay::new(max_er),
            er_r: MultitapDelay::new(max_er),
            er_level: 0.5,
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.4),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.4),
            barr: BarrLoop::new(sample_rate),
            vintage: false,
            last_decay: 0.5,
            fdn_l: Self::make_fdn(sample_rate, 0.5, false),
            fdn_r: Self::make_fdn(sample_rate, 0.5, true),
            mod_ap_l,
            mod_ap_r,
            tone_lp_l,
            tone_lp_r,
            hf_damp_l,
            hf_damp_r,
            sample_rate,
            size: 0.5,
            late_level: 1.0,
            width: 1.0,
        };

        room.setup_er_taps(0.5);
        room.setup_mod_allpass(0.1);
        room
    }

    fn make_fdn(sample_rate: f64, size: f64, offset: bool) -> Fdn {
        // Shorter prime-ish delay lengths than Hall — sized for rooms
        let base = if offset {
            [467, 587, 709, 853, 977, 1123, 1259, 1409]
        } else {
            [443, 557, 677, 811, 941, 1087, 1213, 1361]
        };
        let scale = sample_rate / 48000.0 * size.max(0.1);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| num::f64_to_index(f64::from(d) * scale).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.7);
        fdn.set_damping(6000.0, sample_rate); // Rooms absorb more HF than halls
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
        // Shorter modulated allpass delays than Hall — room scale
        let base_delays = [71, 97, 127, 163, 199, 239, 277, 317];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.1);

        for (i, ((d, ap_l), ap_r)) in base_delays
            .iter()
            .zip(self.mod_ap_l.iter_mut())
            .zip(self.mod_ap_r.iter_mut())
            .enumerate()
        {
            let delay = num::f64_to_index(f64::from(*d) * scale);
            ap_l.sample_delay = delay.max(4);
            ap_l.feedback = 0.35; // Slightly less than Hall
            ap_l.set_modulation(
                num::count_to_f64(i).mul_add(0.1, 0.2),                   // Slower rates than Hall
                modulation * self.sample_rate * 0.0003, // Less depth than Hall
                self.sample_rate,
            );
            ap_l.set_phase(num::count_to_f64(i) / num::count_to_f64(FDN_MOD_AP_COUNT));

            let delay_r = num::f64_to_index((f64::from(*d) + 13.0) * scale);
            ap_r.sample_delay = delay_r.max(4);
            ap_r.feedback = 0.35;
            ap_r.set_modulation(
                num::count_to_f64(i).mul_add(0.08, 0.25),
                modulation * self.sample_rate * 0.0003,
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

impl ReverbAlgorithm for Room {
    fn reset(&mut self) {
        self.barr.reset();
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
        self.hf_damp_l.reset();
        self.hf_damp_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_vintage(&mut self, on: bool) -> bool {
        self.vintage = on;
        self.fdn_l.set_vintage_reads(on, self.sample_rate);
        self.fdn_r.set_vintage_reads(on, self.sample_rate);
        true
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Keep the Classic-voice Barr ring in sync.
        self.last_decay = params.decay;
        let barr_t60 = if params.decay >= 0.999 {
            1.0e6
        } else {
            0.3 * 40.0f64.powf(params.decay)
        };
        self.barr.set_t60(barr_t60);
        self.barr
            .set_damping((1.0 - params.damping).mul_add(8000.0, 1500.0));

        // Size → scale all delay lengths (0.1x closet to 1.5x large studio)
        let new_size = params.size.mul_add(1.4, 0.1);
        if (new_size - self.size).abs() > 0.01 {
            self.size = new_size;
            self.rebuild_fdns();
            self.setup_er_taps(new_size);
            self.setup_mod_allpass(params.modulation);
        }

        // In-loop allpasses and a slow rotation of the feedback mix — the
        // density Hall has always had and this engine did not. It did not
        // matter while the old feedback-gain model capped the tail short:
        // once a real room can actually ring for seconds, the sparse modal
        // structure sings, and an isolated mode stands ~36 dB above its
        // neighbours. Diffusing inside the loop builds density with every
        // recirculation instead of only at the input diffuser.
        self.fdn_l.set_loop_allpass(0.6);
        self.fdn_r.set_loop_allpass(0.6);
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
        // the reachable tail well short of a real room, from a tight booth to a large live space and made
        // `decay` mean something different here than in every other engine.
        // Range covers a real room, from a tight booth to a large live space.
        let t60 = decay_to_t60(params.decay, ROOM_T60.0, ROOM_T60.1);
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

        // Damping → FDN feedback LP frequency
        // Rooms have more HF absorption than halls (soft furnishings, carpet)
        let damp_freq = (1.0 - params.damping).mul_add(11200.0, 800.0); // 800 Hz–12k Hz
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

        // Additional HF damping on the late tail
        let hf_freq = (1.0 - params.damping).mul_add(8000.0, 4000.0); // 4k–12k
        self.hf_damp_l.set_freq(hf_freq, self.sample_rate);
        self.hf_damp_r.set_freq(hf_freq, self.sample_rate);

        // Diffusion → input diffuser stages and feedback
        let stages = num::f64_to_index(params.diffusion * 8.0);
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(params.diffusion.mul_add(0.2, 0.5));
        self.diffuser_r.set_feedback(params.diffusion.mul_add(0.2, 0.5));

        // Modulation → modulated AP in feedback path (subtle for rooms)
        self.setup_mod_allpass(params.modulation);

        // Input diffuser modulation (very subtle)
        let diff_mod_depth = params.modulation * 3.0;
        self.diffuser_l
            .set_modulation(0.4, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.4, diff_mod_depth, self.sample_rate);

        // Tone → output lowpass
        let tone_freq = ((1.0 + params.tone) * 0.5).mul_add(10000.0, 4000.0); // 4k–14k
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        // Extra A → ER/late balance
        self.er_level = params.extra_a.mul_add(0.7, 0.3);
        self.late_level = 1.0;

        // Extra B → stereo width (0 = mono late, 1 = full stereo)
        self.width = params.extra_b;
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Early reflections (stereo)
        let er_l = self.er_l.tick(left) * self.er_level;
        let er_r = self.er_r.tick(right) * self.er_level;

        // Input diffusion
        let diff_l = self.diffuser_l.tick(left);
        let diff_r = self.diffuser_r.tick(right);

        // Late core: the FDN pair (MX) or the Barr single-loop ring
        // (Classic voice — sparser, ringier, one modulated allpass
        // keeping the ring alive).
        let (mut late_l, mut late_r) = if self.vintage {
            self.barr.tick((diff_l + diff_r) * 0.5)
        } else {
            (self.fdn_l.tick(diff_l), self.fdn_r.tick(diff_r))
        };

        // Modulated allpass in feedback path (subtle chorus in tail)
        for (ap_l, ap_r) in self.mod_ap_l.iter_mut().zip(self.mod_ap_r.iter_mut()) {
            late_l = ap_l.tick(late_l);
            late_r = ap_r.tick(late_r);
        }

        // HF damping on late tail
        late_l = self.hf_damp_l.tick(late_l);
        late_r = self.hf_damp_r.tick(late_r);

        // Stereo width control (mono-to-stereo blend on late reverb)
        (late_l, late_r) = audiocore_dsp::stereo::width(late_l, late_r, self.width);

        // Tone filter
        late_l = self.tone_lp_l.tick(late_l) * self.late_level;
        late_r = self.tone_lp_r.tick(late_r) * self.late_level;

        (er_l + late_l, er_r + late_r)
    }
}
