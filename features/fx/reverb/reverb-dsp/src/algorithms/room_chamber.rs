//! Chamber reverb — small, bright, live room.
//!
//! Modeled after a small stone or tile echo chamber:
//!   - Very short delay lines (tight space)
//!   - Minimal HF damping (hard reflective surfaces)
//!   - Dense, quick ER buildup (walls are close)
//!   - Bright, present character
//!   - Fast density saturation (small volume fills quickly)

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap};
use crate::primitives::one_pole::Lp1;

const FDN_MOD_AP_COUNT: usize = 8;

pub struct RoomChamber {
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

    sample_rate: f64,
    size: f64,
    late_level: f64,
}

impl RoomChamber {
    pub fn new(sample_rate: f64) -> Self {
        let max_er = (sample_rate * 0.04) as usize; // 40ms max ER (small space)

        let mod_ap_l = std::array::from_fn(|_| ModulatedAllpass::new());
        let mod_ap_r = std::array::from_fn(|_| ModulatedAllpass::new());

        let mut tone_lp_l = Lp1::new();
        tone_lp_l.set_freq(16000.0, sample_rate); // Bright — hard surfaces
        let mut tone_lp_r = Lp1::new();
        tone_lp_r.set_freq(16000.0, sample_rate);

        let mut chamber = Self {
            er_l: MultitapDelay::new(max_er),
            er_r: MultitapDelay::new(max_er),
            er_level: 0.6, // Prominent ER in small space
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.2),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.2),
            fdn_l: Self::make_fdn(sample_rate, 0.25, false),
            fdn_r: Self::make_fdn(sample_rate, 0.25, true),
            mod_ap_l,
            mod_ap_r,
            tone_lp_l,
            tone_lp_r,
            sample_rate,
            size: 0.25,
            late_level: 1.0,
        };

        chamber.setup_er_taps(0.25);
        chamber.setup_mod_allpass(0.05);
        chamber
    }

    fn make_fdn(sample_rate: f64, size: f64, offset: bool) -> Fdn {
        // Very short delays — small chamber
        let base = if !offset {
            [251, 317, 389, 467, 547, 631, 719, 811]
        } else {
            [269, 337, 409, 487, 569, 653, 743, 839]
        };
        let scale = sample_rate / 48000.0 * size.max(0.05);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| ((d as f64 * scale) as usize).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.65);
        fdn.set_damping(10000.0, sample_rate); // Hard surfaces — less damping
        fdn
    }

    fn setup_er_taps(&mut self, size: f64) {
        let scale = self.sample_rate / 48000.0 * size.max(0.05);
        // Chamber ER: very dense, close together (small room, walls nearby)
        let taps_l = [
            Tap {
                delay_samples: (31.0 * scale) as usize,
                gain: 0.92,
            },
            Tap {
                delay_samples: (59.0 * scale) as usize,
                gain: 0.85,
            },
            Tap {
                delay_samples: (89.0 * scale) as usize,
                gain: 0.78,
            },
            Tap {
                delay_samples: (127.0 * scale) as usize,
                gain: 0.68,
            },
            Tap {
                delay_samples: (167.0 * scale) as usize,
                gain: 0.58,
            },
            Tap {
                delay_samples: (211.0 * scale) as usize,
                gain: 0.48,
            },
            Tap {
                delay_samples: (263.0 * scale) as usize,
                gain: 0.38,
            },
            Tap {
                delay_samples: (317.0 * scale) as usize,
                gain: 0.28,
            },
            Tap {
                delay_samples: (379.0 * scale) as usize,
                gain: 0.20,
            },
            Tap {
                delay_samples: (443.0 * scale) as usize,
                gain: 0.14,
            },
            Tap {
                delay_samples: (509.0 * scale) as usize,
                gain: 0.09,
            },
            Tap {
                delay_samples: (577.0 * scale) as usize,
                gain: 0.05,
            },
        ];
        let taps_r = [
            Tap {
                delay_samples: (37.0 * scale) as usize,
                gain: 0.92,
            },
            Tap {
                delay_samples: (67.0 * scale) as usize,
                gain: 0.85,
            },
            Tap {
                delay_samples: (101.0 * scale) as usize,
                gain: 0.78,
            },
            Tap {
                delay_samples: (139.0 * scale) as usize,
                gain: 0.68,
            },
            Tap {
                delay_samples: (181.0 * scale) as usize,
                gain: 0.58,
            },
            Tap {
                delay_samples: (227.0 * scale) as usize,
                gain: 0.48,
            },
            Tap {
                delay_samples: (277.0 * scale) as usize,
                gain: 0.38,
            },
            Tap {
                delay_samples: (331.0 * scale) as usize,
                gain: 0.28,
            },
            Tap {
                delay_samples: (397.0 * scale) as usize,
                gain: 0.20,
            },
            Tap {
                delay_samples: (461.0 * scale) as usize,
                gain: 0.14,
            },
            Tap {
                delay_samples: (523.0 * scale) as usize,
                gain: 0.09,
            },
            Tap {
                delay_samples: (593.0 * scale) as usize,
                gain: 0.05,
            },
        ];
        // Alternate tap polarity (Moorer-style): all-positive spike trains
        // carry a net-positive area = subsonic thump in the IR spectrum.
        // Alternating signs keeps timing/level, zeroes the DC lobe.
        let mut taps_l = taps_l;
        let mut taps_r = taps_r;
        // Greedy sign balance: flip each tap against the running sum so
        // the net area stays near zero (plain alternation leaves ~0.4 of
        // residual area because the gains decay).
        let mut sum = 0.0;
        for t in taps_l.iter_mut() {
            if sum > 0.0 {
                t.gain = -t.gain;
            }
            sum += t.gain;
        }
        sum = 0.0;
        for t in taps_r.iter_mut() {
            if sum > 0.0 {
                t.gain = -t.gain;
            }
            sum += t.gain;
        }
        self.er_l.set_taps(&taps_l);
        self.er_r.set_taps(&taps_r);
    }

    fn setup_mod_allpass(&mut self, modulation: f64) {
        // Very short mod AP delays — chamber scale
        let base_delays = [41, 53, 67, 83, 101, 127, 151, 179];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.05);

        for i in 0..FDN_MOD_AP_COUNT {
            let delay = ((base_delays[i] as f64) * scale) as usize;
            self.mod_ap_l[i].sample_delay = delay.max(4);
            self.mod_ap_l[i].feedback = 0.3;
            self.mod_ap_l[i].set_modulation(
                0.3 + i as f64 * 0.12,
                modulation * self.sample_rate * 0.0002,
                self.sample_rate,
            );
            self.mod_ap_l[i].set_phase(i as f64 / FDN_MOD_AP_COUNT as f64);

            let delay_r = ((base_delays[i] as f64 + 7.0) * scale) as usize;
            self.mod_ap_r[i].sample_delay = delay_r.max(4);
            self.mod_ap_r[i].feedback = 0.3;
            self.mod_ap_r[i].set_modulation(
                0.35 + i as f64 * 0.1,
                modulation * self.sample_rate * 0.0002,
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

impl ReverbAlgorithm for RoomChamber {
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
        // Size — chamber is small: closet to small room
        let new_size = 0.05 + params.size * 0.5; // 0.05x to 0.55x
        if (new_size - self.size).abs() > 0.005 {
            self.size = new_size;
            self.rebuild_fdns();
            self.setup_er_taps(new_size);
            self.setup_mod_allpass(params.modulation);
        }

        // Decay — chambers decay quickly
        let decay_gain = 0.25 + params.decay * 0.6; // 0.25 to 0.85
        self.fdn_l.set_decay(decay_gain);
        self.fdn_r.set_decay(decay_gain);

        // Damping — hard surfaces = bright
        let damp_freq = 2000.0 + (1.0 - params.damping) * 14000.0;
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

        // Diffusion
        let stages = (params.diffusion * 8.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.5 + params.diffusion * 0.25);
        self.diffuser_r.set_feedback(0.5 + params.diffusion * 0.25);

        // Modulation (subtle in chamber)
        self.setup_mod_allpass(params.modulation);
        let diff_mod_depth = params.modulation * 2.0;
        self.diffuser_l
            .set_modulation(0.5, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.5, diff_mod_depth, self.sample_rate);

        // Tone
        let tone_freq = 5000.0 + (1.0 + params.tone) * 0.5 * 11000.0;
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        // Extra A → ER/late balance
        self.er_level = 0.4 + params.extra_a * 0.6;
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

        for i in 0..FDN_MOD_AP_COUNT {
            late_l = self.mod_ap_l[i].tick(late_l);
            late_r = self.mod_ap_r[i].tick(late_r);
        }

        late_l = self.tone_lp_l.tick(late_l) * self.late_level;
        late_r = self.tone_lp_r.tick(late_r) * self.late_level;

        (er_l + late_l, er_r + late_r)
    }
}
