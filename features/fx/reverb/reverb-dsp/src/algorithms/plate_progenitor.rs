//! Progenitor-style plate reverb.
//!
//! Based on Dattorro's Progenitor reverb (evolution of the 1997 plate),
//! using allpass feedback delay networks for denser, richer late field.
//!
//! Key differences from the basic Dattorro plate:
//!   - 4 allpass delays per tank half (vs 2) for more diffusion
//!   - Additional modulation on inner allpass stages
//!   - Frequency-dependent decay (bass and treble decay separately)
//!   - More output taps for smoother stereo image
//!   - Higher density suitable for longer decay times
//!
//! Topology per tank half:
//!   Input → AP1 (mod) → AP2 → Delay1 → LF damp → HF damp → ×decay
//!   → AP3 (mod) → AP4 → Delay2 → cross-feed to other half

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass::Allpass;
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

/// One Progenitor tank: four allpasses (two modulated) around two
/// delays, with separate HF-damping and LF-shelf filters.
struct Tank {
    ap1: ModulatedAllpass,
    ap2: Allpass,
    delay1: DelayLine,
    /// HF damping.
    hf_damping: Lp1,
    /// LF shelf (bass decay).
    bass_split: Lp1,
    ap3: ModulatedAllpass,
    ap4: Allpass,
    delay2: DelayLine,
}

/// One tank's lengths, modulation rates and phases. Lengths are already
/// scaled to the host rate.
struct TankVoicing {
    ap1_len: usize,
    ap2_len: usize,
    delay1_len: usize,
    ap3_len: usize,
    ap4_len: usize,
    delay2_len: usize,
    /// Modulation rate of the two modulated allpasses, in Hz.
    ap1_rate: f64,
    ap3_rate: f64,
    /// Their starting LFO phases (stereo decorrelation).
    ap1_phase: f64,
    ap3_phase: f64,
}

impl Tank {
    fn new(v: &TankVoicing, s: f64, sample_rate: f64) -> Self {
        // AP1 takes the negative coefficient, per the Dattorro convention.
        let mut ap1 = ModulatedAllpass::new();
        ap1.sample_delay = v.ap1_len;
        ap1.feedback = -0.7;
        ap1.set_modulation(v.ap1_rate, 14.0 * s, sample_rate);
        ap1.set_phase(v.ap1_phase);

        let mut ap2 = Allpass::new(v.ap2_len);
        ap2.set_delay(v.ap2_len);
        ap2.set_feedback(0.5);

        // AP3 is the extra modulation point Progenitor adds.
        let mut ap3 = ModulatedAllpass::new();
        ap3.sample_delay = v.ap3_len;
        ap3.feedback = 0.5;
        ap3.set_modulation(v.ap3_rate, 10.0 * s, sample_rate);
        ap3.set_phase(v.ap3_phase);

        let mut ap4 = Allpass::new(v.ap4_len);
        ap4.set_delay(v.ap4_len);
        ap4.set_feedback(0.45);

        let mut hf_damping = Lp1::new();
        hf_damping.set_freq(8000.0, sample_rate);
        let mut bass_split = Lp1::new();
        bass_split.set_freq(200.0, sample_rate);

        Self {
            ap1,
            ap2,
            delay1: DelayLine::new(v.delay1_len.saturating_add(1)),
            hf_damping,
            bass_split,
            ap3,
            ap4,
            delay2: DelayLine::new(v.delay2_len.saturating_add(1)),
        }
    }

    fn reset(&mut self) {
        self.ap1.reset();
        self.ap2.reset();
        self.delay1.clear();
        self.hf_damping.reset();
        self.bass_split.reset();
        self.ap3.reset();
        self.ap4.reset();
        self.delay2.clear();
    }
}

pub struct PlateProgenitor {
    // Input bandwidth
    bandwidth: Lp1,
    // Input diffusion (4 series allpass)
    input_diffuser: [Allpass; 4],

    // The two tanks. Same shape, different lengths, modulation rates
    // and phases — a `Tank` each rather than sixteen `tank_a_*` /
    // `tank_b_*` fields walked in parallel.
    tank_a: Tank,
    tank_b: Tank,

    // DC blockers on the loop cross-feeds.
    dc_a: DcBlocker,
    dc_b: DcBlocker,
    decay: f64,
    bass_mult: f64, // Bass decay multiplier
    s: f64,
    sample_rate: f64,
}

impl PlateProgenitor {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let s = sample_rate / 29761.0;

        // Input diffuser delays
        let id = [
            num::f64_to_index(156.0 * s),
            num::f64_to_index(113.0 * s),
            num::f64_to_index(341.0 * s),
            num::f64_to_index(251.0 * s),
        ];

        // Progenitor uses longer, more varied delays than Dattorro;
        // tank B is offset for stereo decorrelation.
        let tank_a = Tank::new(
            &TankVoicing {
                ap1_len: num::f64_to_index(617.0 * s),
                ap2_len: num::f64_to_index(439.0 * s),
                delay1_len: num::f64_to_index(4597.0 * s),
                ap3_len: num::f64_to_index(1559.0 * s),
                ap4_len: num::f64_to_index(887.0 * s),
                delay2_len: num::f64_to_index(3823.0 * s),
                ap1_rate: 0.7,
                ap3_rate: 1.1,
                ap1_phase: 0.0,
                ap3_phase: 0.25,
            },
            s,
            sample_rate,
        );
        let tank_b = Tank::new(
            &TankVoicing {
                ap1_len: num::f64_to_index(773.0 * s),
                ap2_len: num::f64_to_index(521.0 * s),
                delay1_len: num::f64_to_index(4357.0 * s),
                ap3_len: num::f64_to_index(1801.0 * s),
                ap4_len: num::f64_to_index(1019.0 * s),
                delay2_len: num::f64_to_index(3467.0 * s),
                ap1_rate: 0.8,
                ap3_rate: 1.0,
                ap1_phase: 0.5,
                ap3_phase: 0.75,
            },
            s,
            sample_rate,
        );

        // Input bandwidth
        let mut bandwidth = Lp1::new();
        bandwidth.set_freq(10000.0, sample_rate);

        // Build input diffusers
        let mut input_diffuser = [
            Allpass::new(id[0]),
            Allpass::new(id[1]),
            Allpass::new(id[2]),
            Allpass::new(id[3]),
        ];
        input_diffuser[0].set_delay(id[0]);
        input_diffuser[0].set_feedback(0.75);
        input_diffuser[1].set_delay(id[1]);
        input_diffuser[1].set_feedback(0.75);
        input_diffuser[2].set_delay(id[2]);
        input_diffuser[2].set_feedback(0.625);
        input_diffuser[3].set_delay(id[3]);
        input_diffuser[3].set_feedback(0.625);

        Self {
            bandwidth,
            input_diffuser,
            tank_a,
            tank_b,
            dc_a: DcBlocker::new(),
            dc_b: DcBlocker::new(),
            decay: 0.7,
            bass_mult: 1.0,
            s,
            sample_rate,
        }
    }
}

impl ReverbAlgorithm for PlateProgenitor {
    fn reset(&mut self) {
        self.bandwidth.reset();
        self.dc_a.reset();
        self.dc_b.reset();
        for d in &mut self.input_diffuser {
            d.reset();
        }
        self.tank_a.reset();
        self.tank_b.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay — Progenitor can go longer than basic Dattorro
        self.decay = params.decay.mul_add(0.74, 0.25); // 0.25 to 0.99

        // Damping → tank LP cutoff
        let freq = (1.0 - params.damping).mul_add(14000.0, 2000.0);
        self.tank_a.hf_damping.set_freq(freq, self.sample_rate);
        self.tank_b.hf_damping.set_freq(freq, self.sample_rate);

        // Bass decay — via HP filter in feedback (extra_a controls bass ratio)
        // Higher extra_a = more bass decay (shorter bass RT60)
        let bass_freq = params.extra_a.mul_add(400.0, 80.0); // 80-480 Hz
        self.tank_a.bass_split.set_freq(bass_freq, self.sample_rate);
        self.tank_b.bass_split.set_freq(bass_freq, self.sample_rate);
        self.bass_mult = params.extra_a.mul_add(-0.3, 1.0); // 1.0 to 0.7

        // Input bandwidth
        let bw_freq = params.damping.mul_add(-0.5, 1.0).mul_add(12000.0, 4000.0);
        self.bandwidth.set_freq(bw_freq, self.sample_rate);

        // Diffusion — 4 AP stages allow finer control
        let dd1 = params.diffusion.mul_add(0.2, 0.5);
        let dd2 = params.diffusion.mul_add(0.15, 0.35);
        self.tank_a.ap1.feedback = -dd1;
        self.tank_b.ap1.feedback = -dd1;
        self.tank_a.ap2.set_feedback(dd2);
        self.tank_b.ap2.set_feedback(dd2);
        self.tank_a.ap3.feedback = dd2;
        self.tank_b.ap3.feedback = dd2;
        self.tank_a.ap4.set_feedback(dd2 * 0.9);
        self.tank_b.ap4.set_feedback(dd2 * 0.9);

        // Input diffusion
        let id1 = params.diffusion.mul_add(0.15, 0.6);
        let id2 = params.diffusion.mul_add(0.125, 0.5);
        self.input_diffuser[0].set_feedback(id1);
        self.input_diffuser[1].set_feedback(id1);
        self.input_diffuser[2].set_feedback(id2);
        self.input_diffuser[3].set_feedback(id2);

        // Modulation — more modulation points than basic plate
        let mod_depth = params.modulation * 22.0 * self.s;
        self.tank_a.ap1
            .set_modulation(0.7, mod_depth, self.sample_rate);
        self.tank_a.ap3
            .set_modulation(1.1, mod_depth * 0.6, self.sample_rate);
        self.tank_b.ap1
            .set_modulation(0.8, mod_depth, self.sample_rate);
        self.tank_b.ap3
            .set_modulation(1.0, mod_depth * 0.6, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let input = (left + right) * 0.5;
        let bw = self.bandwidth.tick(input);

        // Input diffusion cascade
        let mut x = bw;
        for d in &mut self.input_diffuser {
            x = d.tick(x);
        }

        let s = self.s;

        // Cross-feed from end of each tank
        let fb_a = self
            .dc_a
            .tick(self.tank_b.delay2.read(num::f64_to_index(3467.0 * s)));
        let fb_b = self
            .dc_b
            .tick(self.tank_a.delay2.read(num::f64_to_index(3823.0 * s)));

        // ---- Tank A ----
        // AP1 (modulated) → AP2 → Delay1
        let a1 = self.tank_a.ap1.tick(x + fb_a * self.decay);
        let a2 = self.tank_a.ap2.tick(a1);
        self.tank_a.delay1.write(a2);
        let a_d1 = self.tank_a.delay1.read(num::f64_to_index(4597.0 * s));

        // Frequency-dependent decay: LP damping + HP bass control
        let a_lp = self.tank_a.hf_damping.tick(a_d1);
        // Bass control: blend between full signal and HP-filtered
        let a_hp_part = a_d1 - self.tank_a.bass_split.tick(a_d1); // highpassed
        let a_damped = (a_lp * self.bass_mult + a_hp_part * (1.0 - self.bass_mult)) * self.decay;

        // AP3 (modulated) → AP4 → Delay2
        let a3 = self.tank_a.ap3.tick(a_damped);
        let a4 = self.tank_a.ap4.tick(a3);
        self.tank_a.delay2.write(a4);

        // ---- Tank B ----
        let b1 = self.tank_b.ap1.tick(x + fb_b * self.decay);
        let b2 = self.tank_b.ap2.tick(b1);
        self.tank_b.delay1.write(b2);
        let b_d1 = self.tank_b.delay1.read(num::f64_to_index(4357.0 * s));

        let b_lp = self.tank_b.hf_damping.tick(b_d1);
        let b_hp_part = b_d1 - self.tank_b.bass_split.tick(b_d1);
        let b_damped = (b_lp * self.bass_mult + b_hp_part * (1.0 - self.bass_mult)) * self.decay;

        let b3 = self.tank_b.ap3.tick(b_damped);
        let b4 = self.tank_b.ap4.tick(b3);
        self.tank_b.delay2.write(b4);

        // ---- Multi-tap output (more taps than basic Dattorro) ----
        // Tapping from all 4 delay lines for maximum density
        let out_l = self.tank_a.delay1.read(num::f64_to_index(241.0 * s))
            + self.tank_a.delay1.read(num::f64_to_index(3079.0 * s))
            - self.tank_b.delay1.read(num::f64_to_index(1747.0 * s))
            + self.tank_b.delay2.read(num::f64_to_index(1979.0 * s))
            - self.tank_a.delay2.read(num::f64_to_index(953.0 * s))
            - self.tank_b.delay2.read(num::f64_to_index(2521.0 * s))
            + self.tank_a.delay2.read(num::f64_to_index(2711.0 * s));

        let out_r = self.tank_b.delay1.read(num::f64_to_index(317.0 * s))
            + self.tank_b.delay1.read(num::f64_to_index(3251.0 * s))
            - self.tank_a.delay1.read(num::f64_to_index(1913.0 * s))
            + self.tank_a.delay2.read(num::f64_to_index(1571.0 * s))
            - self.tank_b.delay2.read(num::f64_to_index(811.0 * s))
            - self.tank_a.delay2.read(num::f64_to_index(2243.0 * s))
            + self.tank_b.delay2.read(num::f64_to_index(2857.0 * s));

        (out_l * 0.2, out_r * 0.2)
    }
}
