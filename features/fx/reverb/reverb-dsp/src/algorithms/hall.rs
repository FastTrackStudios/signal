//! Hall reverb — large space with early reflections and dense late tail.
//!
//! Architecture based on Griesinger/Costello hall designs:
//!   1. Early reflections (stereo multi-tap delay)
//!   2. Input diffusion (allpass cascade)
//!   3. Late reverb (8-line FDN with modulated delays, per-line damping,
//!      Householder mixing, size-scalable delay lengths)
//!   4. Stereo cross-coupling between L/R FDN tanks
//!
//! The size parameter scales all delay lengths simultaneously.
//! Modulation is applied in the FDN feedback path for chorus-like detuning.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap};
use crate::primitives::one_pole::Lp1;

/// Number of modulated AP stages in the FDN feedback path.
const FDN_MOD_AP_COUNT: usize = 8;

pub struct Hall {
    // Early reflections (stereo)
    er_l: MultitapDelay,
    er_r: MultitapDelay,
    er_level: f64,

    // Input diffusion
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,

    // Late reverb — 8-line FDN per side
    fdn_l: Fdn,
    fdn_r: Fdn,

    // Modulated allpass in FDN feedback path (chorus detuning)
    mod_ap_l: [ModulatedAllpass; FDN_MOD_AP_COUNT],
    mod_ap_r: [ModulatedAllpass; FDN_MOD_AP_COUNT],

    // Tone control (output filtering)
    tone_lp_l: Lp1,
    tone_lp_r: Lp1,

    // Cross-feed between L/R tanks
    cross_feed: f64,

    sample_rate: f64,
    size: f64,
}

impl Hall {
    pub fn new(sample_rate: f64) -> Self {
        let max_er = (sample_rate * 0.15) as usize; // 150ms max ER

        let mod_ap_l = std::array::from_fn(|_| ModulatedAllpass::new());
        let mod_ap_r = std::array::from_fn(|_| ModulatedAllpass::new());

        let mut tone_lp_l = Lp1::new();
        tone_lp_l.set_freq(16000.0, sample_rate);
        let mut tone_lp_r = Lp1::new();
        tone_lp_r.set_freq(16000.0, sample_rate);

        let mut hall = Self {
            er_l: MultitapDelay::new(max_er),
            er_r: MultitapDelay::new(max_er),
            er_level: 0.4,
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 1.0),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 1.0),
            fdn_l: Self::make_fdn(sample_rate, 1.0, false),
            fdn_r: Self::make_fdn(sample_rate, 1.0, true),
            mod_ap_l,
            mod_ap_r,
            tone_lp_l,
            tone_lp_r,
            cross_feed: 0.15,
            sample_rate,
            size: 1.0,
        };

        hall.setup_er_taps(1.0);
        hall.setup_mod_allpass(0.2);
        hall
    }

    fn make_fdn(sample_rate: f64, size: f64, offset: bool) -> Fdn {
        // Prime-ish delay lengths for maximum density, longer than room
        let base = if !offset {
            [1549, 1877, 2237, 2663, 3109, 3571, 4019, 4507]
        } else {
            [1607, 1949, 2311, 2741, 3191, 3637, 4091, 4583]
        };
        let scale = sample_rate / 48000.0 * size.max(0.2);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| ((d as f64 * scale) as usize).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.85);
        fdn.set_damping(8000.0, sample_rate);
        fdn
    }

    fn setup_er_taps(&mut self, size: f64) {
        let scale = self.sample_rate / 48000.0 * size.max(0.2);
        // Hall ER pattern — based on a large concert hall geometry
        // More taps at longer delays for increasing density
        let taps_l = [
            Tap {
                delay_samples: (197.0 * scale) as usize,
                gain: 0.85,
            },
            Tap {
                delay_samples: (373.0 * scale) as usize,
                gain: 0.72,
            },
            Tap {
                delay_samples: (521.0 * scale) as usize,
                gain: 0.60,
            },
            Tap {
                delay_samples: (743.0 * scale) as usize,
                gain: 0.48,
            },
            Tap {
                delay_samples: (977.0 * scale) as usize,
                gain: 0.37,
            },
            Tap {
                delay_samples: (1259.0 * scale) as usize,
                gain: 0.28,
            },
            Tap {
                delay_samples: (1571.0 * scale) as usize,
                gain: 0.20,
            },
            Tap {
                delay_samples: (1889.0 * scale) as usize,
                gain: 0.14,
            },
        ];
        let taps_r = [
            Tap {
                delay_samples: (229.0 * scale) as usize,
                gain: 0.85,
            },
            Tap {
                delay_samples: (409.0 * scale) as usize,
                gain: 0.72,
            },
            Tap {
                delay_samples: (577.0 * scale) as usize,
                gain: 0.60,
            },
            Tap {
                delay_samples: (811.0 * scale) as usize,
                gain: 0.48,
            },
            Tap {
                delay_samples: (1051.0 * scale) as usize,
                gain: 0.37,
            },
            Tap {
                delay_samples: (1327.0 * scale) as usize,
                gain: 0.28,
            },
            Tap {
                delay_samples: (1637.0 * scale) as usize,
                gain: 0.20,
            },
            Tap {
                delay_samples: (1979.0 * scale) as usize,
                gain: 0.14,
            },
        ];
        self.er_l.set_taps(&taps_l);
        self.er_r.set_taps(&taps_r);
    }

    fn setup_mod_allpass(&mut self, modulation: f64) {
        // Modulated allpass in FDN feedback path for chorus detuning
        let base_delays = [137, 173, 211, 257, 307, 359, 419, 479];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.2);

        for i in 0..FDN_MOD_AP_COUNT {
            let delay = ((base_delays[i] as f64) * scale) as usize;
            self.mod_ap_l[i].sample_delay = delay.max(4);
            self.mod_ap_l[i].feedback = 0.4;
            self.mod_ap_l[i].set_modulation(
                0.3 + i as f64 * 0.15,
                modulation * self.sample_rate * 0.0005,
                self.sample_rate,
            );
            self.mod_ap_l[i].set_phase(i as f64 / FDN_MOD_AP_COUNT as f64);

            let delay_r = ((base_delays[i] as f64 + 17.0) * scale) as usize;
            self.mod_ap_r[i].sample_delay = delay_r.max(4);
            self.mod_ap_r[i].feedback = 0.4;
            self.mod_ap_r[i].set_modulation(
                0.35 + i as f64 * 0.12,
                modulation * self.sample_rate * 0.0005,
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

impl ReverbAlgorithm for Hall {
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
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Size → scale all delay lengths
        let new_size = 0.3 + params.size * 2.0; // 0.3x to 2.3x
        if (new_size - self.size).abs() > 0.01 {
            self.size = new_size;
            self.rebuild_fdns();
            self.setup_er_taps(new_size);
            self.setup_mod_allpass(params.modulation);
        }

        // Decay (0.0 → ~0.5s, 1.0 → ~30s)
        let decay_gain = 0.5 + params.decay * 0.48; // 0.5 to 0.98
        self.fdn_l.set_decay(decay_gain);
        self.fdn_r.set_decay(decay_gain);

        // Damping → FDN feedback LP frequency
        let damp_freq = 1000.0 + (1.0 - params.damping) * 15000.0; // 1k–16k
        self.fdn_l.set_damping(damp_freq, self.sample_rate);
        self.fdn_r.set_damping(damp_freq, self.sample_rate);

        // Multi-band decay
        self.fdn_l.set_band_decay(params.band_crossover_hz, params.low_decay_mult, params.high_decay_mult, self.sample_rate);
        self.fdn_r.set_band_decay(params.band_crossover_hz, params.low_decay_mult, params.high_decay_mult, self.sample_rate);

        // Diffusion → input diffuser stages and feedback
        let stages = (params.diffusion * 10.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.5 + params.diffusion * 0.25);
        self.diffuser_r.set_feedback(0.5 + params.diffusion * 0.25);

        // Modulation → modulated AP in feedback path
        self.setup_mod_allpass(params.modulation);

        // Input diffuser modulation (subtle)
        let diff_mod_depth = params.modulation * 6.0;
        self.diffuser_l
            .set_modulation(0.5, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.5, diff_mod_depth, self.sample_rate);

        // Tone → output lowpass
        let tone_freq = 4000.0 + (1.0 + params.tone) * 0.5 * 12000.0; // 4k–16k
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        // Extra A → ER/late balance
        self.er_level = params.extra_a * 0.6;

        // Extra B → cross-feed amount (stereo width of late tail)
        self.cross_feed = params.extra_b * 0.3;
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Early reflections (stereo)
        let er_l = self.er_l.tick(left) * self.er_level;
        let er_r = self.er_r.tick(right) * self.er_level;

        // Input diffusion
        let diff_l = self.diffuser_l.tick(left);
        let diff_r = self.diffuser_r.tick(right);

        // Cross-feed injection
        let fdn_in_l = diff_l + er_r * self.cross_feed;
        let fdn_in_r = diff_r + er_l * self.cross_feed;

        // FDN late reverb
        let mut late_l = self.fdn_l.tick(fdn_in_l);
        let mut late_r = self.fdn_r.tick(fdn_in_r);

        // Modulated allpass in feedback path (chorus detuning in tail)
        for i in 0..FDN_MOD_AP_COUNT {
            late_l = self.mod_ap_l[i].tick(late_l);
            late_r = self.mod_ap_r[i].tick(late_r);
        }

        // Tone filter
        late_l = self.tone_lp_l.tick(late_l);
        late_r = self.tone_lp_r.tick(late_r);

        (er_l + late_l, er_r + late_r)
    }
}
