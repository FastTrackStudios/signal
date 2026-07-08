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

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::multitap_delay::{MultitapDelay, Tap};
use crate::primitives::one_pole::Lp1;

/// Number of modulated AP stages in the FDN feedback path.
const FDN_MOD_AP_COUNT: usize = 8;

pub struct Room {
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
    pub fn new(sample_rate: f64) -> Self {
        let max_er = (sample_rate * 0.08) as usize; // 80ms max ER (rooms are smaller)

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
        let base = if !offset {
            [443, 557, 677, 811, 941, 1087, 1213, 1361]
        } else {
            [467, 587, 709, 853, 977, 1123, 1259, 1409]
        };
        let scale = sample_rate / 48000.0 * size.max(0.1);
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| ((d as f64 * scale) as usize).max(4))
            .collect();
        let mut fdn = Fdn::new(&delays, MixMatrix::Householder);
        fdn.set_decay(0.7);
        fdn.set_damping(6000.0, sample_rate); // Rooms absorb more HF than halls
        fdn
    }

    fn setup_er_taps(&mut self, size: f64) {
        let scale = self.sample_rate / 48000.0 * size.max(0.1);

        // Image-source inspired ER pattern for a medium rectangular room
        // (~5m × 4m × 3m). First-order wall reflections arrive first,
        // followed by corner and ceiling reflections with increasing density.
        // L/R taps offset for stereo decorrelation.
        let taps_l = [
            // First-order wall reflections (direct path ~1-3ms)
            Tap {
                delay_samples: (67.0 * scale) as usize,
                gain: 0.90,
            }, // near wall
            Tap {
                delay_samples: (131.0 * scale) as usize,
                gain: 0.82,
            }, // side wall
            Tap {
                delay_samples: (197.0 * scale) as usize,
                gain: 0.74,
            }, // far wall
            // Second-order (wall-wall) reflections
            Tap {
                delay_samples: (281.0 * scale) as usize,
                gain: 0.62,
            },
            Tap {
                delay_samples: (353.0 * scale) as usize,
                gain: 0.52,
            },
            Tap {
                delay_samples: (443.0 * scale) as usize,
                gain: 0.43,
            },
            // Floor/ceiling + higher-order
            Tap {
                delay_samples: (557.0 * scale) as usize,
                gain: 0.34,
            },
            Tap {
                delay_samples: (677.0 * scale) as usize,
                gain: 0.26,
            },
            Tap {
                delay_samples: (811.0 * scale) as usize,
                gain: 0.19,
            },
            Tap {
                delay_samples: (971.0 * scale) as usize,
                gain: 0.13,
            },
        ];
        let taps_r = [
            Tap {
                delay_samples: (79.0 * scale) as usize,
                gain: 0.90,
            },
            Tap {
                delay_samples: (149.0 * scale) as usize,
                gain: 0.82,
            },
            Tap {
                delay_samples: (223.0 * scale) as usize,
                gain: 0.74,
            },
            Tap {
                delay_samples: (307.0 * scale) as usize,
                gain: 0.62,
            },
            Tap {
                delay_samples: (389.0 * scale) as usize,
                gain: 0.52,
            },
            Tap {
                delay_samples: (479.0 * scale) as usize,
                gain: 0.43,
            },
            Tap {
                delay_samples: (593.0 * scale) as usize,
                gain: 0.34,
            },
            Tap {
                delay_samples: (719.0 * scale) as usize,
                gain: 0.26,
            },
            Tap {
                delay_samples: (859.0 * scale) as usize,
                gain: 0.19,
            },
            Tap {
                delay_samples: (1019.0 * scale) as usize,
                gain: 0.13,
            },
        ];
        // Sign-balance the taps (Moorer-style): an all-positive spike
        // train has a strong net-positive area — a subsonic thump in the
        // IR spectrum. Balancing signs keeps timing/level, kills the DC lobe.
        let mut taps_l = taps_l;
        let mut taps_r = taps_r;
        // Greedy: flip each tap against the running sum so the net area
        // stays near zero (plain alternation leaves ~0.4 of
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
        // Shorter modulated allpass delays than Hall — room scale
        let base_delays = [71, 97, 127, 163, 199, 239, 277, 317];
        let scale = self.sample_rate / 48000.0 * self.size.max(0.1);

        for i in 0..FDN_MOD_AP_COUNT {
            let delay = ((base_delays[i] as f64) * scale) as usize;
            self.mod_ap_l[i].sample_delay = delay.max(4);
            self.mod_ap_l[i].feedback = 0.35; // Slightly less than Hall
            self.mod_ap_l[i].set_modulation(
                0.2 + i as f64 * 0.1,                   // Slower rates than Hall
                modulation * self.sample_rate * 0.0003, // Less depth than Hall
                self.sample_rate,
            );
            self.mod_ap_l[i].set_phase(i as f64 / FDN_MOD_AP_COUNT as f64);

            let delay_r = ((base_delays[i] as f64 + 13.0) * scale) as usize;
            self.mod_ap_r[i].sample_delay = delay_r.max(4);
            self.mod_ap_r[i].feedback = 0.35;
            self.mod_ap_r[i].set_modulation(
                0.25 + i as f64 * 0.08,
                modulation * self.sample_rate * 0.0003,
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

impl ReverbAlgorithm for Room {
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
        self.hf_damp_l.reset();
        self.hf_damp_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Size → scale all delay lengths (0.1x closet to 1.5x large studio)
        let new_size = 0.1 + params.size * 1.4;
        if (new_size - self.size).abs() > 0.01 {
            self.size = new_size;
            self.rebuild_fdns();
            self.setup_er_taps(new_size);
            self.setup_mod_allpass(params.modulation);
        }

        // Decay (0.0 → ~0.2s, 1.0 → ~5s) — rooms decay faster than halls
        let decay_gain = 0.3 + params.decay * 0.65; // 0.3 to 0.95
        self.fdn_l.set_decay(decay_gain);
        self.fdn_r.set_decay(decay_gain);

        // Damping → FDN feedback LP frequency
        // Rooms have more HF absorption than halls (soft furnishings, carpet)
        let damp_freq = 800.0 + (1.0 - params.damping) * 11200.0; // 800 Hz–12k Hz
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
        let hf_freq = 4000.0 + (1.0 - params.damping) * 8000.0; // 4k–12k
        self.hf_damp_l.set_freq(hf_freq, self.sample_rate);
        self.hf_damp_r.set_freq(hf_freq, self.sample_rate);

        // Diffusion → input diffuser stages and feedback
        let stages = (params.diffusion * 8.0) as usize;
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);
        self.diffuser_l.set_feedback(0.5 + params.diffusion * 0.2);
        self.diffuser_r.set_feedback(0.5 + params.diffusion * 0.2);

        // Modulation → modulated AP in feedback path (subtle for rooms)
        self.setup_mod_allpass(params.modulation);

        // Input diffuser modulation (very subtle)
        let diff_mod_depth = params.modulation * 3.0;
        self.diffuser_l
            .set_modulation(0.4, diff_mod_depth, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.4, diff_mod_depth, self.sample_rate);

        // Tone → output lowpass
        let tone_freq = 4000.0 + (1.0 + params.tone) * 0.5 * 10000.0; // 4k–14k
        self.tone_lp_l.set_freq(tone_freq, self.sample_rate);
        self.tone_lp_r.set_freq(tone_freq, self.sample_rate);

        // Extra A → ER/late balance
        self.er_level = 0.3 + params.extra_a * 0.7;
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

        // FDN late reverb (independent L/R for stereo)
        let mut late_l = self.fdn_l.tick(diff_l);
        let mut late_r = self.fdn_r.tick(diff_r);

        // Modulated allpass in feedback path (subtle chorus in tail)
        for i in 0..FDN_MOD_AP_COUNT {
            late_l = self.mod_ap_l[i].tick(late_l);
            late_r = self.mod_ap_r[i].tick(late_r);
        }

        // HF damping on late tail
        late_l = self.hf_damp_l.tick(late_l);
        late_r = self.hf_damp_r.tick(late_r);

        // Stereo width control (mono-to-stereo blend on late reverb)
        let mid = (late_l + late_r) * 0.5;
        let side = (late_l - late_r) * 0.5;
        late_l = mid + side * self.width;
        late_r = mid - side * self.width;

        // Tone filter
        late_l = self.tone_lp_l.tick(late_l) * self.late_level;
        late_r = self.tone_lp_r.tick(late_r) * self.late_level;

        (er_l + late_l, er_r + late_r)
    }
}
