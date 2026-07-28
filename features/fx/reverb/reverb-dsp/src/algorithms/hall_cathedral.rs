//! Cathedral hall reverb — very large space with extreme diffusion.
//!
//! Compared to the Concert Hall variant:
//!   - Much longer delay lines (massive space)
//!   - More diffusion stages (stone walls scatter heavily)
//!   - Longer pre-delay gap between ER and late field
//!   - Less HF damping (hard surfaces preserve brightness)
//!   - More early reflection taps (complex geometry)
//!   - Stronger cross-coupling (omnidirectional sound field)

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap};
use crate::primitives::one_pole::Lp1;

const FDN_MOD_AP_COUNT: usize = 8;

pub struct HallCathedral {
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

    cross_feed: f64,
    sample_rate: f64,
    size: f64,
}

impl HallCathedral {
    pub fn new(sample_rate: f64) -> Self {
        let max_er = (sample_rate * 0.25) as usize; // 250ms max ER (cathedral is huge)

        let mod_ap_l = std::array::from_fn(|_| ModulatedAllpass::new());
        let mod_ap_r = std::array::from_fn(|_| ModulatedAllpass::new());

        let mut tone_lp_l = Lp1::new();
        tone_lp_l.set_freq(18000.0, sample_rate); // Brighter than concert hall
        let mut tone_lp_r = Lp1::new();
        tone_lp_r.set_freq(18000.0, sample_rate);

        let mut cathedral = Self {
            er_l: MultitapDelay::new(max_er),
            er_r: MultitapDelay::new(max_er),
            er_level: 0.3,
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 1.5),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 1.5),
            fdn_l: Self::make_fdn(sample_rate, 1.5, false),
            fdn_r: Self::make_fdn(sample_rate, 1.5, true),
            mod_ap_l,
            mod_ap_r,
            tone_lp_l,
            tone_lp_r,
            cross_feed: 0.25, // Stronger cross-feed (omnidirectional)
            sample_rate,
            size: 1.5,
        };

        cathedral.setup_er_taps(1.5);
        cathedral.setup_mod_allpass(0.25);
        cathedral
    }

    fn make_fdn(sample_rate: f64, size: f64, offset: bool) -> Fdn {
        // Much longer delays than concert hall — cathedral scale
        let base = if !offset {
            [2113, 2557, 3049, 3631, 4241, 4871, 5483, 6143]
        } else {
            [2203, 2663, 3163, 3767, 4397, 5039, 5591, 6277]
        };
        let scale = sample_rate / 48000.0 * size.max(0.5);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| ((d as f64 * scale) as usize).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.92); // Long decay — stone walls
        fdn.set_damping(12000.0, sample_rate); // Less HF absorption than carpet/wood
        fdn
    }

    fn setup_er_taps(&mut self, size: f64) {
        let scale = self.sample_rate / 48000.0 * size.max(0.5);
        // Cathedral ER: sparse early, then increasingly dense
        // Long initial gap (sound travels far to first wall)
        let taps_l = [
            Tap {
                delay_samples: (347.0 * scale) as usize,
                gain: 0.80,
            },
            Tap {
                delay_samples: (631.0 * scale) as usize,
                gain: 0.68,
            },
            Tap {
                delay_samples: (887.0 * scale) as usize,
                gain: 0.58,
            },
            Tap {
                delay_samples: (1153.0 * scale) as usize,
                gain: 0.48,
            },
            Tap {
                delay_samples: (1471.0 * scale) as usize,
                gain: 0.39,
            },
            Tap {
                delay_samples: (1789.0 * scale) as usize,
                gain: 0.31,
            },
            Tap {
                delay_samples: (2111.0 * scale) as usize,
                gain: 0.24,
            },
            Tap {
                delay_samples: (2503.0 * scale) as usize,
                gain: 0.18,
            },
            Tap {
                delay_samples: (2897.0 * scale) as usize,
                gain: 0.13,
            },
            Tap {
                delay_samples: (3307.0 * scale) as usize,
                gain: 0.09,
            },
        ];
        let taps_r = [
            Tap {
                delay_samples: (389.0 * scale) as usize,
                gain: 0.80,
            },
            Tap {
                delay_samples: (701.0 * scale) as usize,
                gain: 0.68,
            },
            Tap {
                delay_samples: (953.0 * scale) as usize,
                gain: 0.58,
            },
            Tap {
                delay_samples: (1231.0 * scale) as usize,
                gain: 0.48,
            },
            Tap {
                delay_samples: (1559.0 * scale) as usize,
                gain: 0.39,
            },
            Tap {
                delay_samples: (1877.0 * scale) as usize,
                gain: 0.31,
            },
            Tap {
                delay_samples: (2237.0 * scale) as usize,
                gain: 0.24,
            },
            Tap {
                delay_samples: (2633.0 * scale) as usize,
                gain: 0.18,
            },
            Tap {
                delay_samples: (3041.0 * scale) as usize,
                gain: 0.13,
            },
            Tap {
                delay_samples: (3461.0 * scale) as usize,
                gain: 0.09,
            },
        ];
        self.er_l.set_taps(&taps_l);
        self.er_r.set_taps(&taps_r);
    }

    fn setup_mod_allpass(&mut self, modulation: f64) {
        // Longer mod AP delays for cathedral scale
        let base_delays = [191, 241, 293, 353, 421, 491, 569, 647];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.5);

        #[allow(clippy::needless_range_loop)]
        for i in 0..FDN_MOD_AP_COUNT {
            let delay = ((base_delays[i] as f64) * scale) as usize;
            self.mod_ap_l[i].sample_delay = delay.max(4);
            self.mod_ap_l[i].feedback = 0.45;
            self.mod_ap_l[i].set_modulation(
                0.25 + i as f64 * 0.12,
                modulation * self.sample_rate * 0.0006,
                self.sample_rate,
            );
            self.mod_ap_l[i].set_phase(i as f64 / FDN_MOD_AP_COUNT as f64);

            let delay_r = ((base_delays[i] as f64 + 19.0) * scale) as usize;
            self.mod_ap_r[i].sample_delay = delay_r.max(4);
            self.mod_ap_r[i].feedback = 0.45;
            self.mod_ap_r[i].set_modulation(
                0.3 + i as f64 * 0.1,
                modulation * self.sample_rate * 0.0006,
                self.sample_rate,
            );
            self.mod_ap_r[i].set_phase((i as f64 + 0.5) / FDN_MOD_AP_COUNT as f64);
        }
    }

    fn rebuild_fdns(&mut self) {
        self.fdn_l = Self::make_fdn(self.sample_rate, self.size, false);
        self.fdn_r = Self::make_fdn(self.sample_rate, self.size, true);
    }
}

impl ReverbAlgorithm for HallCathedral {
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
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Size — cathedral ranges from large church to massive basilica
        let new_size = 1.0 + params.size * 2.5; // 1.0x to 3.5x
        if (new_size - self.size).abs() > 0.01 {
            self.size = new_size;
            self.rebuild_fdns();
            self.setup_er_taps(new_size);
            self.setup_mod_allpass(params.modulation);
        }

        // Exact per-line T60 decay (Jot shelf): decay knob maps to a
        // midband T60, the Low End multiplier stretches/tames the DC
        // target and damping + the high multiplier set the Nyquist
        // target. decay pinned at 1.0 (freeze) holds ~forever.
        let t60 = if params.decay >= 0.999 {
            1.0e6
        } else {
            0.8 * 50.0f64.powf(params.decay)
        };
        let t60_dc = (t60 * params.low_decay_mult.max(0.05)).max(0.05);
        let hf_ratio = ((0.15 + 0.85 * (1.0 - params.damping))
            * params.high_decay_mult.max(0.05))
        .clamp(0.02, 1.5);
        let t60_ny = (t60 * hf_ratio).max(0.02);
        self.fdn_l.set_t60(t60_dc, t60_ny, self.sample_rate);
        self.fdn_r.set_t60(t60_dc, t60_ny, self.sample_rate);

        // In-loop allpasses (zita-style ±): hall density builds with
        // every recirculation instead of only at the input diffuser.
        self.fdn_l.set_loop_allpass(0.55);
        self.fdn_r.set_loop_allpass(0.55);

        // Artifact-free tail animation: slow orthogonal rotation of the
        // feedback mix (no decay error, no pitch wobble).
        self.fdn_l
            .set_rotation(0.35 + params.modulation * 0.6, params.modulation * 0.2, self.sample_rate);
        self.fdn_r
            .set_rotation((0.35 + params.modulation * 0.6) * 1.13, params.modulation * 0.2, self.sample_rate);

        // Diffusion — more stages, cathedral scatters heavily
        let stages = (params.diffusion * 12.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.55 + params.diffusion * 0.25);
        self.diffuser_r.set_feedback(0.55 + params.diffusion * 0.25);

        // Modulation
        self.setup_mod_allpass(params.modulation);
        let diff_mod_depth = params.modulation * 8.0;
        self.diffuser_l
            .set_modulation(0.4, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.4, diff_mod_depth, self.sample_rate);

        // Tone
        let tone_freq = 4000.0 + (1.0 + params.tone) * 0.5 * 14000.0;
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        // Extra A → ER level
        self.er_level = params.extra_a * 0.5;

        // Extra B → cross-feed (stereo field)
        self.cross_feed = 0.1 + params.extra_b * 0.35;
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let er_l = self.er_l.tick(left) * self.er_level;
        let er_r = self.er_r.tick(right) * self.er_level;

        let diff_l = self.diffuser_l.tick(left);
        let diff_r = self.diffuser_r.tick(right);

        // Cross-feed injection (stronger than concert hall)
        let fdn_in_l = diff_l + er_r * self.cross_feed;
        let fdn_in_r = diff_r + er_l * self.cross_feed;

        let mut late_l = self.fdn_l.tick(fdn_in_l);
        let mut late_r = self.fdn_r.tick(fdn_in_r);

        for i in 0..FDN_MOD_AP_COUNT {
            late_l = self.mod_ap_l[i].tick(late_l);
            late_r = self.mod_ap_r[i].tick(late_r);
        }

        late_l = self.tone_lp_l.tick(late_l);
        late_r = self.tone_lp_r.tick(late_r);

        (er_l + late_l, er_r + late_r)
    }
}
