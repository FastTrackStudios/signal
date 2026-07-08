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

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass::Allpass;
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

pub struct PlateProgenitor {
    // Input bandwidth
    bandwidth: Lp1,
    // Input diffusion (4 series allpass)
    input_diffuser: [Allpass; 4],

    // Tank A — 4 AP + 2 delays
    tank_a_ap1: ModulatedAllpass,
    tank_a_ap2: Allpass,
    tank_a_delay1: DelayLine,
    tank_a_damp_lp: Lp1, // HF damping
    tank_a_damp_hp: Lp1, // LF shelf (bass decay)
    tank_a_ap3: ModulatedAllpass,
    tank_a_ap4: Allpass,
    tank_a_delay2: DelayLine,

    // Tank B — 4 AP + 2 delays
    tank_b_ap1: ModulatedAllpass,
    tank_b_ap2: Allpass,
    tank_b_delay1: DelayLine,
    tank_b_damp_lp: Lp1,
    tank_b_damp_hp: Lp1,
    tank_b_ap3: ModulatedAllpass,
    tank_b_ap4: Allpass,
    tank_b_delay2: DelayLine,

    // DC blockers on the loop cross-feeds.
    dc_a: DcBlocker,
    dc_b: DcBlocker,
    decay: f64,
    bass_mult: f64, // Bass decay multiplier
    s: f64,
    sample_rate: f64,
}

impl PlateProgenitor {
    pub fn new(sample_rate: f64) -> Self {
        let s = sample_rate / 29761.0;

        // Input diffuser delays
        let id = [
            (156.0 * s) as usize,
            (113.0 * s) as usize,
            (341.0 * s) as usize,
            (251.0 * s) as usize,
        ];

        // Tank A delays — Progenitor uses longer, more varied delays
        let ta_ap1_len = (617.0 * s) as usize;
        let ta_ap2_len = (439.0 * s) as usize;
        let ta_d1_len = (4597.0 * s) as usize;
        let ta_ap3_len = (1559.0 * s) as usize;
        let ta_ap4_len = (887.0 * s) as usize;
        let ta_d2_len = (3823.0 * s) as usize;

        // Tank B delays — offset for stereo decorrelation
        let tb_ap1_len = (773.0 * s) as usize;
        let tb_ap2_len = (521.0 * s) as usize;
        let tb_d1_len = (4357.0 * s) as usize;
        let tb_ap3_len = (1801.0 * s) as usize;
        let tb_ap4_len = (1019.0 * s) as usize;
        let tb_d2_len = (3467.0 * s) as usize;

        // Tank A AP1 (modulated, negative coeff — Dattorro convention)
        let mut tank_a_ap1 = ModulatedAllpass::new();
        tank_a_ap1.sample_delay = ta_ap1_len;
        tank_a_ap1.feedback = -0.7;
        tank_a_ap1.set_modulation(0.7, 14.0 * s, sample_rate);
        tank_a_ap1.set_phase(0.0);

        let mut tank_a_ap2 = Allpass::new(ta_ap2_len);
        tank_a_ap2.set_delay(ta_ap2_len);
        tank_a_ap2.set_feedback(0.5);

        // Tank A AP3 (modulated — extra modulation point)
        let mut tank_a_ap3 = ModulatedAllpass::new();
        tank_a_ap3.sample_delay = ta_ap3_len;
        tank_a_ap3.feedback = 0.5;
        tank_a_ap3.set_modulation(1.1, 10.0 * s, sample_rate);
        tank_a_ap3.set_phase(0.25);

        let mut tank_a_ap4 = Allpass::new(ta_ap4_len);
        tank_a_ap4.set_delay(ta_ap4_len);
        tank_a_ap4.set_feedback(0.45);

        // Tank B AP1 (modulated)
        let mut tank_b_ap1 = ModulatedAllpass::new();
        tank_b_ap1.sample_delay = tb_ap1_len;
        tank_b_ap1.feedback = -0.7;
        tank_b_ap1.set_modulation(0.8, 14.0 * s, sample_rate);
        tank_b_ap1.set_phase(0.5);

        let mut tank_b_ap2 = Allpass::new(tb_ap2_len);
        tank_b_ap2.set_delay(tb_ap2_len);
        tank_b_ap2.set_feedback(0.5);

        // Tank B AP3 (modulated)
        let mut tank_b_ap3 = ModulatedAllpass::new();
        tank_b_ap3.sample_delay = tb_ap3_len;
        tank_b_ap3.feedback = 0.5;
        tank_b_ap3.set_modulation(1.0, 10.0 * s, sample_rate);
        tank_b_ap3.set_phase(0.75);

        let mut tank_b_ap4 = Allpass::new(tb_ap4_len);
        tank_b_ap4.set_delay(tb_ap4_len);
        tank_b_ap4.set_feedback(0.45);

        // Damping filters
        let mut tank_a_damp_lp = Lp1::new();
        tank_a_damp_lp.set_freq(8000.0, sample_rate);
        let mut tank_a_damp_hp = Lp1::new();
        tank_a_damp_hp.set_freq(200.0, sample_rate);
        let mut tank_b_damp_lp = Lp1::new();
        tank_b_damp_lp.set_freq(8000.0, sample_rate);
        let mut tank_b_damp_hp = Lp1::new();
        tank_b_damp_hp.set_freq(200.0, sample_rate);

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
            tank_a_ap1,
            tank_a_ap2,
            tank_a_delay1: DelayLine::new(ta_d1_len + 1),
            tank_a_damp_lp,
            tank_a_damp_hp,
            tank_a_ap3,
            tank_a_ap4,
            tank_a_delay2: DelayLine::new(ta_d2_len + 1),
            tank_b_ap1,
            tank_b_ap2,
            tank_b_delay1: DelayLine::new(tb_d1_len + 1),
            tank_b_damp_lp,
            tank_b_damp_hp,
            tank_b_ap3,
            tank_b_ap4,
            tank_b_delay2: DelayLine::new(tb_d2_len + 1),
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
        self.tank_a_ap1.reset();
        self.tank_a_ap2.reset();
        self.tank_a_delay1.clear();
        self.tank_a_damp_lp.reset();
        self.tank_a_damp_hp.reset();
        self.tank_a_ap3.reset();
        self.tank_a_ap4.reset();
        self.tank_a_delay2.clear();
        self.tank_b_ap1.reset();
        self.tank_b_ap2.reset();
        self.tank_b_delay1.clear();
        self.tank_b_damp_lp.reset();
        self.tank_b_damp_hp.reset();
        self.tank_b_ap3.reset();
        self.tank_b_ap4.reset();
        self.tank_b_delay2.clear();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay — Progenitor can go longer than basic Dattorro
        self.decay = 0.25 + params.decay * 0.74; // 0.25 to 0.99

        // Damping → tank LP cutoff
        let freq = 2000.0 + (1.0 - params.damping) * 14000.0;
        self.tank_a_damp_lp.set_freq(freq, self.sample_rate);
        self.tank_b_damp_lp.set_freq(freq, self.sample_rate);

        // Bass decay — via HP filter in feedback (extra_a controls bass ratio)
        // Higher extra_a = more bass decay (shorter bass RT60)
        let bass_freq = 80.0 + params.extra_a * 400.0; // 80-480 Hz
        self.tank_a_damp_hp.set_freq(bass_freq, self.sample_rate);
        self.tank_b_damp_hp.set_freq(bass_freq, self.sample_rate);
        self.bass_mult = 1.0 - params.extra_a * 0.3; // 1.0 to 0.7

        // Input bandwidth
        let bw_freq = 4000.0 + (1.0 - params.damping * 0.5) * 12000.0;
        self.bandwidth.set_freq(bw_freq, self.sample_rate);

        // Diffusion — 4 AP stages allow finer control
        let dd1 = 0.5 + params.diffusion * 0.2;
        let dd2 = 0.35 + params.diffusion * 0.15;
        self.tank_a_ap1.feedback = -dd1;
        self.tank_b_ap1.feedback = -dd1;
        self.tank_a_ap2.set_feedback(dd2);
        self.tank_b_ap2.set_feedback(dd2);
        self.tank_a_ap3.feedback = dd2;
        self.tank_b_ap3.feedback = dd2;
        self.tank_a_ap4.set_feedback(dd2 * 0.9);
        self.tank_b_ap4.set_feedback(dd2 * 0.9);

        // Input diffusion
        let id1 = 0.6 + params.diffusion * 0.15;
        let id2 = 0.5 + params.diffusion * 0.125;
        self.input_diffuser[0].set_feedback(id1);
        self.input_diffuser[1].set_feedback(id1);
        self.input_diffuser[2].set_feedback(id2);
        self.input_diffuser[3].set_feedback(id2);

        // Modulation — more modulation points than basic plate
        let mod_depth = params.modulation * 22.0 * self.s;
        self.tank_a_ap1
            .set_modulation(0.7, mod_depth, self.sample_rate);
        self.tank_a_ap3
            .set_modulation(1.1, mod_depth * 0.6, self.sample_rate);
        self.tank_b_ap1
            .set_modulation(0.8, mod_depth, self.sample_rate);
        self.tank_b_ap3
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
        let fb_a = self.dc_a.tick(self.tank_b_delay2.read((3467.0 * s) as usize));
        let fb_b = self.dc_b.tick(self.tank_a_delay2.read((3823.0 * s) as usize));

        // ---- Tank A ----
        // AP1 (modulated) → AP2 → Delay1
        let a1 = self.tank_a_ap1.tick(x + fb_a * self.decay);
        let a2 = self.tank_a_ap2.tick(a1);
        self.tank_a_delay1.write(a2);
        let a_d1 = self.tank_a_delay1.read((4597.0 * s) as usize);

        // Frequency-dependent decay: LP damping + HP bass control
        let a_lp = self.tank_a_damp_lp.tick(a_d1);
        // Bass control: blend between full signal and HP-filtered
        let a_hp_part = a_d1 - self.tank_a_damp_hp.tick(a_d1); // highpassed
        let a_damped = (a_lp * self.bass_mult + a_hp_part * (1.0 - self.bass_mult)) * self.decay;

        // AP3 (modulated) → AP4 → Delay2
        let a3 = self.tank_a_ap3.tick(a_damped);
        let a4 = self.tank_a_ap4.tick(a3);
        self.tank_a_delay2.write(a4);

        // ---- Tank B ----
        let b1 = self.tank_b_ap1.tick(x + fb_b * self.decay);
        let b2 = self.tank_b_ap2.tick(b1);
        self.tank_b_delay1.write(b2);
        let b_d1 = self.tank_b_delay1.read((4357.0 * s) as usize);

        let b_lp = self.tank_b_damp_lp.tick(b_d1);
        let b_hp_part = b_d1 - self.tank_b_damp_hp.tick(b_d1);
        let b_damped = (b_lp * self.bass_mult + b_hp_part * (1.0 - self.bass_mult)) * self.decay;

        let b3 = self.tank_b_ap3.tick(b_damped);
        let b4 = self.tank_b_ap4.tick(b3);
        self.tank_b_delay2.write(b4);

        // ---- Multi-tap output (more taps than basic Dattorro) ----
        // Tapping from all 4 delay lines for maximum density
        let out_l = self.tank_a_delay1.read((241.0 * s) as usize)
            + self.tank_a_delay1.read((3079.0 * s) as usize)
            - self.tank_b_delay1.read((1747.0 * s) as usize)
            + self.tank_b_delay2.read((1979.0 * s) as usize)
            - self.tank_a_delay2.read((953.0 * s) as usize)
            - self.tank_b_delay2.read((2521.0 * s) as usize)
            + self.tank_a_delay2.read((2711.0 * s) as usize);

        let out_r = self.tank_b_delay1.read((317.0 * s) as usize)
            + self.tank_b_delay1.read((3251.0 * s) as usize)
            - self.tank_a_delay1.read((1913.0 * s) as usize)
            + self.tank_a_delay2.read((1571.0 * s) as usize)
            - self.tank_b_delay2.read((811.0 * s) as usize)
            - self.tank_a_delay2.read((2243.0 * s) as usize)
            + self.tank_b_delay2.read((2857.0 * s) as usize);

        (out_l * 0.2, out_r * 0.2)
    }
}
