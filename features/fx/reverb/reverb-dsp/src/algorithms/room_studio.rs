//! Studio reverb — treated, controlled acoustic space.
//!
//! Modeled after a professionally treated recording studio:
//!   - Moderate delay lines (medium room)
//!   - Controlled HF damping (acoustic treatment absorbs mids/highs)
//!   - Smooth, even ER (diffusers on walls)
//!   - Tight low end (bass trapping)
//!   - Neutral, transparent character
//!   - Quick density buildup (diffused surfaces)

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap};
use crate::primitives::one_pole::Lp1;

const FDN_MOD_AP_COUNT: usize = 8;

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
    pub fn new(sample_rate: f64) -> Self {
        let max_er = (sample_rate * 0.06) as usize; // 60ms max ER

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
        let base = if !offset {
            [389, 487, 601, 719, 839, 967, 1097, 1229]
        } else {
            [409, 509, 619, 743, 863, 991, 1123, 1259]
        };
        let scale = sample_rate / 48000.0 * size.max(0.1);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| ((d as f64 * scale) as usize).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.6);
        // More damping than chamber — acoustic treatment absorbs
        fdn.set_damping(5000.0, sample_rate);
        fdn
    }

    fn setup_er_taps(&mut self, size: f64) {
        let scale = self.sample_rate / 48000.0 * size.max(0.1);
        // Studio ER: smooth, even spacing (wall diffusers scatter evenly)
        // More uniform gain decay than raw room (treatment controls reflections)
        let taps_l = [
            Tap {
                delay_samples: (53.0 * scale) as usize,
                gain: 0.82,
            },
            Tap {
                delay_samples: (109.0 * scale) as usize,
                gain: 0.72,
            },
            Tap {
                delay_samples: (163.0 * scale) as usize,
                gain: 0.63,
            },
            Tap {
                delay_samples: (223.0 * scale) as usize,
                gain: 0.54,
            },
            Tap {
                delay_samples: (281.0 * scale) as usize,
                gain: 0.46,
            },
            Tap {
                delay_samples: (347.0 * scale) as usize,
                gain: 0.38,
            },
            Tap {
                delay_samples: (419.0 * scale) as usize,
                gain: 0.31,
            },
            Tap {
                delay_samples: (491.0 * scale) as usize,
                gain: 0.24,
            },
            Tap {
                delay_samples: (569.0 * scale) as usize,
                gain: 0.18,
            },
            Tap {
                delay_samples: (647.0 * scale) as usize,
                gain: 0.13,
            },
        ];
        let taps_r = [
            Tap {
                delay_samples: (61.0 * scale) as usize,
                gain: 0.82,
            },
            Tap {
                delay_samples: (119.0 * scale) as usize,
                gain: 0.72,
            },
            Tap {
                delay_samples: (179.0 * scale) as usize,
                gain: 0.63,
            },
            Tap {
                delay_samples: (241.0 * scale) as usize,
                gain: 0.54,
            },
            Tap {
                delay_samples: (307.0 * scale) as usize,
                gain: 0.46,
            },
            Tap {
                delay_samples: (373.0 * scale) as usize,
                gain: 0.38,
            },
            Tap {
                delay_samples: (443.0 * scale) as usize,
                gain: 0.31,
            },
            Tap {
                delay_samples: (517.0 * scale) as usize,
                gain: 0.24,
            },
            Tap {
                delay_samples: (593.0 * scale) as usize,
                gain: 0.18,
            },
            Tap {
                delay_samples: (673.0 * scale) as usize,
                gain: 0.13,
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
        let base_delays = [59, 79, 101, 127, 157, 191, 229, 269];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.1);

        #[allow(clippy::needless_range_loop)]
        for i in 0..FDN_MOD_AP_COUNT {
            let delay = ((base_delays[i] as f64) * scale) as usize;
            self.mod_ap_l[i].sample_delay = delay.max(4);
            self.mod_ap_l[i].feedback = 0.3;
            self.mod_ap_l[i].set_modulation(
                0.25 + i as f64 * 0.08,
                modulation * self.sample_rate * 0.0002,
                self.sample_rate,
            );
            self.mod_ap_l[i].set_phase(i as f64 / FDN_MOD_AP_COUNT as f64);

            let delay_r = ((base_delays[i] as f64 + 11.0) * scale) as usize;
            self.mod_ap_r[i].sample_delay = delay_r.max(4);
            self.mod_ap_r[i].feedback = 0.3;
            self.mod_ap_r[i].set_modulation(
                0.3 + i as f64 * 0.07,
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
        let new_size = 0.15 + params.size * 0.85; // 0.15x to 1.0x
        if (new_size - self.size).abs() > 0.01 {
            self.size = new_size;
            self.rebuild_fdns();
            self.setup_er_taps(new_size);
            self.setup_mod_allpass(params.modulation);
        }

        // Decay — studios are controlled, shorter decay than live rooms
        let decay_gain = 0.2 + params.decay * 0.65; // 0.2 to 0.85
        self.fdn_l.set_decay(decay_gain);
        self.fdn_r.set_decay(decay_gain);

        // Damping — acoustic treatment absorbs more consistently
        let damp_freq = 1500.0 + (1.0 - params.damping) * 8500.0;
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
        let bass_freq = 80.0 + params.extra_a * 300.0; // 80-380 Hz
        self.bass_hp_l.set_freq(bass_freq, self.sample_rate);
        self.bass_hp_r.set_freq(bass_freq, self.sample_rate);

        // Diffusion — studios have diffusers, so density builds fast
        let stages = (params.diffusion * 10.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.55 + params.diffusion * 0.2);
        self.diffuser_r.set_feedback(0.55 + params.diffusion * 0.2);

        // Modulation (very subtle in studio)
        self.setup_mod_allpass(params.modulation);
        let diff_mod_depth = params.modulation * 2.0;
        self.diffuser_l
            .set_modulation(0.3, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.3, diff_mod_depth, self.sample_rate);

        // Tone
        let tone_freq = 4000.0 + (1.0 + params.tone) * 0.5 * 8000.0;
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

        for i in 0..FDN_MOD_AP_COUNT {
            late_l = self.mod_ap_l[i].tick(late_l);
            late_r = self.mod_ap_r[i].tick(late_r);
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
