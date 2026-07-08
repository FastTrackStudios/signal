//! Lexicon 224-style plate reverb.
//!
//! Based on Griesinger's original design as documented by Dattorro and
//! implemented in Freeverb3. Uses a figure-of-eight allpass loop topology:
//!
//!   Input → bandwidth LP → 4× input diffusion AP
//!   → Twin nested allpass loops with cross-coupling:
//!     Loop A: AP1 → Delay → LP → ×decay → AP2 → Delay → cross to B
//!     Loop B: AP1 → Delay → LP → ×decay → AP2 → Delay → cross to A
//!   → Multi-tap output from both loops
//!
//! Key differences from Dattorro plate:
//!   - Nested allpass pairs in each loop half (more diffusion)
//!   - Different delay ratios (brighter, more metallic character)
//!   - Additional modulation points for richer chorus
//!   - Characteristic Lexicon "shimmer" in the high end

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass::Allpass;
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

/// Lexicon 224-style plate.
pub struct PlateLexicon {
    // Input bandwidth control
    bandwidth: Lp1,
    // Input diffusion (4 series allpass)
    input_diffuser: [Allpass; 4],

    // Loop A — nested allpass pair + delays
    loop_a_ap1: ModulatedAllpass,
    loop_a_delay1: DelayLine,
    loop_a_damp1: Lp1,
    loop_a_ap2: ModulatedAllpass,
    loop_a_delay2: DelayLine,

    // Loop B — nested allpass pair + delays
    loop_b_ap1: ModulatedAllpass,
    loop_b_delay1: DelayLine,
    loop_b_damp1: Lp1,
    loop_b_ap2: ModulatedAllpass,
    loop_b_delay2: DelayLine,

    // Parameters
    // DC blockers on the loop cross-feeds.
    dc_a: DcBlocker,
    dc_b: DcBlocker,
    decay: f64,
    s: f64, // sample rate scale factor
    sample_rate: f64,
}

impl PlateLexicon {
    pub fn new(sample_rate: f64) -> Self {
        // Lexicon 224 reference rate ~30000 Hz (slightly different from Dattorro's 29761)
        let s = sample_rate / 30000.0;

        // Input diffuser delays (Lexicon-style shorter cascade)
        let id = [
            (113.0 * s) as usize,
            (162.0 * s) as usize,
            (241.0 * s) as usize,
            (339.0 * s) as usize,
        ];

        // Loop A allpass delays — nested pair
        let la_ap1_len = (547.0 * s) as usize;
        let la_d1_len = (3571.0 * s) as usize;
        let la_ap2_len = (1187.0 * s) as usize;
        let la_d2_len = (2833.0 * s) as usize;

        // Loop B allpass delays — offset for stereo
        let lb_ap1_len = (709.0 * s) as usize;
        let lb_d1_len = (3373.0 * s) as usize;
        let lb_ap2_len = (1493.0 * s) as usize;
        let lb_d2_len = (2999.0 * s) as usize;

        // Loop A AP1 (modulated)
        let mut loop_a_ap1 = ModulatedAllpass::new();
        loop_a_ap1.sample_delay = la_ap1_len;
        loop_a_ap1.feedback = -0.65;
        loop_a_ap1.set_modulation(0.8, 12.0 * s, sample_rate);
        loop_a_ap1.set_phase(0.0);

        // Loop A AP2 (modulated, different rate)
        let mut loop_a_ap2 = ModulatedAllpass::new();
        loop_a_ap2.sample_delay = la_ap2_len;
        loop_a_ap2.feedback = 0.55;
        loop_a_ap2.set_modulation(1.2, 10.0 * s, sample_rate);
        loop_a_ap2.set_phase(0.25);

        // Loop B AP1 (modulated)
        let mut loop_b_ap1 = ModulatedAllpass::new();
        loop_b_ap1.sample_delay = lb_ap1_len;
        loop_b_ap1.feedback = -0.65;
        loop_b_ap1.set_modulation(0.9, 12.0 * s, sample_rate);
        loop_b_ap1.set_phase(0.5);

        // Loop B AP2 (modulated, different rate)
        let mut loop_b_ap2 = ModulatedAllpass::new();
        loop_b_ap2.sample_delay = lb_ap2_len;
        loop_b_ap2.feedback = 0.55;
        loop_b_ap2.set_modulation(1.1, 10.0 * s, sample_rate);
        loop_b_ap2.set_phase(0.75);

        // Damping
        let mut loop_a_damp1 = Lp1::new();
        loop_a_damp1.set_freq(8000.0, sample_rate);
        let mut loop_b_damp1 = Lp1::new();
        loop_b_damp1.set_freq(8000.0, sample_rate);

        // Input bandwidth
        let mut bandwidth = Lp1::new();
        bandwidth.set_freq(12000.0, sample_rate);

        // Input diffusers
        let mut input_diffuser = [
            Allpass::new(id[0]),
            Allpass::new(id[1]),
            Allpass::new(id[2]),
            Allpass::new(id[3]),
        ];
        input_diffuser[0].set_delay(id[0]);
        input_diffuser[0].set_feedback(0.70);
        input_diffuser[1].set_delay(id[1]);
        input_diffuser[1].set_feedback(0.70);
        input_diffuser[2].set_delay(id[2]);
        input_diffuser[2].set_feedback(0.60);
        input_diffuser[3].set_delay(id[3]);
        input_diffuser[3].set_feedback(0.60);

        Self {
            bandwidth,
            input_diffuser,
            loop_a_ap1,
            loop_a_delay1: DelayLine::new(la_d1_len + 1),
            loop_a_damp1,
            loop_a_ap2,
            loop_a_delay2: DelayLine::new(la_d2_len + 1),
            loop_b_ap1,
            loop_b_delay1: DelayLine::new(lb_d1_len + 1),
            loop_b_damp1,
            loop_b_ap2,
            loop_b_delay2: DelayLine::new(lb_d2_len + 1),
            dc_a: DcBlocker::new(),
            dc_b: DcBlocker::new(),
            decay: 0.7,
            s,
            sample_rate,
        }
    }
}

impl ReverbAlgorithm for PlateLexicon {
    fn reset(&mut self) {
        self.bandwidth.reset();
        self.dc_a.reset();
        self.dc_b.reset();
        for d in &mut self.input_diffuser {
            d.reset();
        }
        self.loop_a_ap1.reset();
        self.loop_a_delay1.clear();
        self.loop_a_damp1.reset();
        self.loop_a_ap2.reset();
        self.loop_a_delay2.clear();
        self.loop_b_ap1.reset();
        self.loop_b_delay1.clear();
        self.loop_b_damp1.reset();
        self.loop_b_ap2.reset();
        self.loop_b_delay2.clear();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay
        self.decay = 0.3 + params.decay * 0.69;

        // Damping
        let freq = 2000.0 + (1.0 - params.damping) * 14000.0;
        self.loop_a_damp1.set_freq(freq, self.sample_rate);
        self.loop_b_damp1.set_freq(freq, self.sample_rate);

        // Input bandwidth — Lexicon characteristic: brighter input than Dattorro
        let bw_freq = 6000.0 + (1.0 - params.damping * 0.3) * 14000.0;
        self.bandwidth.set_freq(bw_freq, self.sample_rate);

        // Diffusion — affects both input diffusers and loop AP feedback
        let id1 = 0.55 + params.diffusion * 0.15;
        let id2 = 0.45 + params.diffusion * 0.15;
        self.input_diffuser[0].set_feedback(id1);
        self.input_diffuser[1].set_feedback(id1);
        self.input_diffuser[2].set_feedback(id2);
        self.input_diffuser[3].set_feedback(id2);

        let loop_fb1 = -(0.5 + params.diffusion * 0.2);
        let loop_fb2 = 0.4 + params.diffusion * 0.15;
        self.loop_a_ap1.feedback = loop_fb1;
        self.loop_b_ap1.feedback = loop_fb1;
        self.loop_a_ap2.feedback = loop_fb2;
        self.loop_b_ap2.feedback = loop_fb2;

        // Modulation — Lexicon has more modulation points than Dattorro
        let mod_depth = params.modulation * 20.0 * self.s;
        self.loop_a_ap1
            .set_modulation(0.8, mod_depth, self.sample_rate);
        self.loop_a_ap2
            .set_modulation(1.2, mod_depth * 0.7, self.sample_rate);
        self.loop_b_ap1
            .set_modulation(0.9, mod_depth, self.sample_rate);
        self.loop_b_ap2
            .set_modulation(1.1, mod_depth * 0.7, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let input = (left + right) * 0.5;
        let bw = self.bandwidth.tick(input);

        // Input diffusion
        let mut x = bw;
        for d in &mut self.input_diffuser {
            x = d.tick(x);
        }

        // Read cross-feed from end of each loop
        let s = self.s;
        let fb_a = self
            .dc_a
            .tick(self.loop_b_delay2.read((2999.0 * s) as usize));
        let fb_b = self
            .dc_b
            .tick(self.loop_a_delay2.read((2833.0 * s) as usize));

        // --- Loop A ---
        // AP1 (modulated, negative feedback — characteristic Lexicon)
        let a_ap1 = self.loop_a_ap1.tick(x + fb_a * self.decay);
        // Delay 1
        self.loop_a_delay1.write(a_ap1);
        let a_d1 = self.loop_a_delay1.read((3571.0 * s) as usize);
        // Damping + decay
        let a_damped = self.loop_a_damp1.tick(a_d1) * self.decay;
        // AP2 (modulated — extra modulation point vs Dattorro)
        let a_ap2 = self.loop_a_ap2.tick(a_damped);
        // Delay 2
        self.loop_a_delay2.write(a_ap2);

        // --- Loop B ---
        let b_ap1 = self.loop_b_ap1.tick(x + fb_b * self.decay);
        self.loop_b_delay1.write(b_ap1);
        let b_d1 = self.loop_b_delay1.read((3373.0 * s) as usize);
        let b_damped = self.loop_b_damp1.tick(b_d1) * self.decay;
        let b_ap2 = self.loop_b_ap2.tick(b_damped);
        self.loop_b_delay2.write(b_ap2);

        // Multi-tap output — Lexicon-style decorrelated tapping
        // More taps than Dattorro for smoother stereo field
        let out_l = self.loop_a_delay1.read((213.0 * s) as usize)
            + self.loop_a_delay1.read((2491.0 * s) as usize)
            - self.loop_b_delay1.read((1571.0 * s) as usize)
            + self.loop_b_delay2.read((1667.0 * s) as usize)
            - self.loop_a_delay2.read((887.0 * s) as usize)
            - self.loop_b_delay2.read((2311.0 * s) as usize);

        let out_r = self.loop_b_delay1.read((281.0 * s) as usize)
            + self.loop_b_delay1.read((2719.0 * s) as usize)
            - self.loop_a_delay1.read((1831.0 * s) as usize)
            + self.loop_a_delay2.read((1423.0 * s) as usize)
            - self.loop_b_delay2.read((773.0 * s) as usize)
            - self.loop_a_delay2.read((2143.0 * s) as usize);

        (out_l * 0.22, out_r * 0.22)
    }
}
