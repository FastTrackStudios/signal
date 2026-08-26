//! Arena hall reverb — stadium/arena-scale reverb.
//!
//! Characteristics:
//!   - Extremely long delay lines (massive open space)
//!   - Very sparse early reflections (distant walls)
//!   - Slow density buildup (long mean free path)
//!   - Strong low-frequency sustain (large volume of air)
//!   - Moderate HF damping (air absorption over distance)
//!   - Wide stereo field (surround-like from distant surfaces)

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap};
use crate::primitives::one_pole::Lp1;

const FDN_MOD_AP_COUNT: usize = 8;

pub struct HallArena {
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
    // Air absorption filter (distance-based HF roll-off)
    air_lp_l: Lp1,
    air_lp_r: Lp1,

    cross_feed: f64,
    sample_rate: f64,
    size: f64,
}

impl HallArena {
    pub fn new(sample_rate: f64) -> Self {
        let max_er = (sample_rate * 0.4) as usize; // 400ms — sound takes time to reach arena walls

        let mod_ap_l = std::array::from_fn(|_| ModulatedAllpass::new());
        let mod_ap_r = std::array::from_fn(|_| ModulatedAllpass::new());

        let mut tone_lp_l = Lp1::new();
        tone_lp_l.set_freq(14000.0, sample_rate);
        let mut tone_lp_r = Lp1::new();
        tone_lp_r.set_freq(14000.0, sample_rate);

        // Air absorption — HF rolls off with distance
        let mut air_lp_l = Lp1::new();
        air_lp_l.set_freq(8000.0, sample_rate);
        let mut air_lp_r = Lp1::new();
        air_lp_r.set_freq(8000.0, sample_rate);

        let mut arena = Self {
            er_l: MultitapDelay::new(max_er),
            er_r: MultitapDelay::new(max_er),
            er_level: 0.25, // Sparse ER in arena
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 2.0),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 2.0),
            fdn_l: Self::make_fdn(sample_rate, 2.0, false),
            fdn_r: Self::make_fdn(sample_rate, 2.0, true),
            mod_ap_l,
            mod_ap_r,
            tone_lp_l,
            tone_lp_r,
            air_lp_l,
            air_lp_r,
            cross_feed: 0.3, // Strong cross-feed — omnidirectional in arena
            sample_rate,
            size: 2.0,
        };

        arena.setup_er_taps(2.0);
        arena.setup_mod_allpass(0.15);
        arena
    }

    fn make_fdn(sample_rate: f64, size: f64, offset: bool) -> Fdn {
        // Very long delays — arena scale (2-4x hall)
        let base = if !offset {
            [3001, 3631, 4327, 5147, 6011, 6907, 7793, 8731]
        } else {
            [3121, 3779, 4493, 5347, 6247, 7177, 8089, 9059]
        };
        let scale = sample_rate / 48000.0 * size.max(0.5);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| ((d as f64 * scale) as usize).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.90);
        fdn.set_damping(6000.0, sample_rate); // Air absorption
        fdn
    }

    fn setup_er_taps(&mut self, size: f64) {
        let scale = self.sample_rate / 48000.0 * size.max(0.5);
        // Arena ER: very sparse, long initial gap, then widely spaced
        // Simulates distant walls in a large open venue
        let taps_l = [
            Tap {
                delay_samples: (701.0 * scale) as usize,
                gain: 0.65,
            },
            Tap {
                delay_samples: (1301.0 * scale) as usize,
                gain: 0.52,
            },
            Tap {
                delay_samples: (1907.0 * scale) as usize,
                gain: 0.41,
            },
            Tap {
                delay_samples: (2503.0 * scale) as usize,
                gain: 0.32,
            },
            Tap {
                delay_samples: (3109.0 * scale) as usize,
                gain: 0.24,
            },
            Tap {
                delay_samples: (3701.0 * scale) as usize,
                gain: 0.18,
            },
            Tap {
                delay_samples: (4297.0 * scale) as usize,
                gain: 0.13,
            },
            Tap {
                delay_samples: (4903.0 * scale) as usize,
                gain: 0.09,
            },
        ];
        let taps_r = [
            Tap {
                delay_samples: (797.0 * scale) as usize,
                gain: 0.65,
            },
            Tap {
                delay_samples: (1409.0 * scale) as usize,
                gain: 0.52,
            },
            Tap {
                delay_samples: (2017.0 * scale) as usize,
                gain: 0.41,
            },
            Tap {
                delay_samples: (2621.0 * scale) as usize,
                gain: 0.32,
            },
            Tap {
                delay_samples: (3217.0 * scale) as usize,
                gain: 0.24,
            },
            Tap {
                delay_samples: (3823.0 * scale) as usize,
                gain: 0.18,
            },
            Tap {
                delay_samples: (4421.0 * scale) as usize,
                gain: 0.13,
            },
            Tap {
                delay_samples: (5021.0 * scale) as usize,
                gain: 0.09,
            },
        ];
        self.er_l.set_taps(&taps_l);
        self.er_r.set_taps(&taps_r);
    }

    fn setup_mod_allpass(&mut self, modulation: f64) {
        // Longer mod AP delays for arena scale
        let base_delays = [251, 317, 389, 461, 541, 631, 727, 829];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.5);

        #[allow(clippy::needless_range_loop)]
        for i in 0..FDN_MOD_AP_COUNT {
            let delay = ((base_delays[i] as f64) * scale) as usize;
            self.mod_ap_l[i].sample_delay = delay.max(4);
            self.mod_ap_l[i].feedback = 0.4;
            self.mod_ap_l[i].set_modulation(
                0.2 + i as f64 * 0.08,
                modulation * self.sample_rate * 0.0004,
                self.sample_rate,
            );
            self.mod_ap_l[i].set_phase(i as f64 / FDN_MOD_AP_COUNT as f64);

            let delay_r = ((base_delays[i] as f64 + 23.0) * scale) as usize;
            self.mod_ap_r[i].sample_delay = delay_r.max(4);
            self.mod_ap_r[i].feedback = 0.4;
            self.mod_ap_r[i].set_modulation(
                0.25 + i as f64 * 0.07,
                modulation * self.sample_rate * 0.0004,
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

impl ReverbAlgorithm for HallArena {
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
        self.air_lp_l.reset();
        self.air_lp_r.reset();
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
        // Size — arena ranges from large venue to stadium
        let new_size = 1.5 + params.size * 3.5; // 1.5x to 5.0x
        if (new_size - self.size).abs() > 0.02 {
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
            1.0 * 40.0f64.powf(params.decay)
        };
        let t60_dc = (t60 * params.low_decay_mult.max(0.05)).max(0.05);
        let hf_ratio = ((0.15 + 0.85 * (1.0 - params.damping)) * params.high_decay_mult.max(0.05))
            .clamp(0.02, 1.5);
        let t60_ny = (t60 * hf_ratio).max(0.02);
        self.fdn_l.set_t60(t60_dc, t60_ny, self.sample_rate);
        self.fdn_r.set_t60(t60_dc, t60_ny, self.sample_rate);
        // Decay Rate EQ (`fx.reverb.decay-eq`), layered over the T60 shelf.
        self.fdn_l
            .set_decay_curve(t60, &params.decay_bands, self.sample_rate);
        self.fdn_r
            .set_decay_curve(t60, &params.decay_bands, self.sample_rate);

        // In-loop allpasses (zita-style ±): hall density builds with
        // every recirculation instead of only at the input diffuser.
        self.fdn_l.set_loop_allpass(0.6);
        self.fdn_r.set_loop_allpass(0.6);

        // Artifact-free tail animation: slow orthogonal rotation of the
        // feedback mix (no decay error, no pitch wobble).
        self.fdn_l.set_rotation(
            0.3 + params.modulation * 0.5,
            params.modulation * 0.12,
            self.sample_rate,
        );
        self.fdn_r.set_rotation(
            (0.3 + params.modulation * 0.5) * 1.13,
            params.modulation * 0.12,
            self.sample_rate,
        );

        // Arena-scale tail animation: random-walk delay jitter
        // (reverbsc-style) — huge but unchorused.
        self.fdn_l.set_jitter(1.2, self.sample_rate);
        self.fdn_r.set_jitter(1.2, self.sample_rate);

        // Air absorption LP
        let air_freq = 3000.0 + (1.0 - params.damping) * 9000.0;
        self.air_lp_l.set_freq(air_freq, self.sample_rate);
        self.air_lp_r.set_freq(air_freq, self.sample_rate);

        // Diffusion — slow buildup in arena
        let stages = (params.diffusion * 10.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.5 + params.diffusion * 0.2);
        self.diffuser_r.set_feedback(0.5 + params.diffusion * 0.2);

        // Modulation
        self.setup_mod_allpass(params.modulation);
        let diff_mod_depth = params.modulation * 5.0;
        self.diffuser_l
            .set_modulation(0.3, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.3, diff_mod_depth, self.sample_rate);

        // Tone
        let tone_freq = 3000.0 + (1.0 + params.tone) * 0.5 * 11000.0;
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        // Extra A → ER level (sparse in arena)
        self.er_level = params.extra_a * 0.4;

        // Extra B → cross-feed
        self.cross_feed = 0.15 + params.extra_b * 0.35;
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let er_l = self.er_l.tick(left) * self.er_level;
        let er_r = self.er_r.tick(right) * self.er_level;

        let diff_l = self.diffuser_l.tick(left);
        let diff_r = self.diffuser_r.tick(right);

        let fdn_in_l = diff_l + er_r * self.cross_feed;
        let fdn_in_r = diff_r + er_l * self.cross_feed;

        let mut late_l = self.fdn_l.tick(fdn_in_l);
        let mut late_r = self.fdn_r.tick(fdn_in_r);

        for i in 0..FDN_MOD_AP_COUNT {
            late_l = self.mod_ap_l[i].tick(late_l);
            late_r = self.mod_ap_r[i].tick(late_r);
        }

        // Air absorption
        late_l = self.air_lp_l.tick(late_l);
        late_r = self.air_lp_r.tick(late_r);

        // Tone
        late_l = self.tone_lp_l.tick(late_l);
        late_r = self.tone_lp_r.tick(late_r);

        (er_l + late_l, er_r + late_r)
    }
}
