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

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass::Allpass;
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

/// Lexicon 224-style plate.
/// One recirculating loop: a nested modulated-allpass pair with a delay
/// and a damping lowpass between them.
struct Loop {
    ap1: ModulatedAllpass,
    delay1: DelayLine,
    damp1: Lp1,
    ap2: ModulatedAllpass,
    delay2: DelayLine,
}

/// One loop's lengths, modulation and phases, already scaled to the host
/// rate where they are lengths.
struct LoopVoicing {
    ap1_len: usize,
    delay1_len: usize,
    ap2_len: usize,
    delay2_len: usize,
    /// Modulation rate of each allpass, in Hz.
    ap1_rate: f64,
    ap2_rate: f64,
    /// Starting LFO phase of each allpass (stereo decorrelation).
    ap1_phase: f64,
    ap2_phase: f64,
}

impl Loop {
    fn new(v: &LoopVoicing, s: f64, sample_rate: f64) -> Self {
        let mut ap1 = ModulatedAllpass::new();
        ap1.sample_delay = v.ap1_len;
        ap1.feedback = -0.65;
        ap1.set_modulation(v.ap1_rate, 12.0 * s, sample_rate);
        ap1.set_phase(v.ap1_phase);

        let mut ap2 = ModulatedAllpass::new();
        ap2.sample_delay = v.ap2_len;
        ap2.feedback = 0.55;
        ap2.set_modulation(v.ap2_rate, 10.0 * s, sample_rate);
        ap2.set_phase(v.ap2_phase);

        let mut damp1 = Lp1::new();
        damp1.set_freq(8000.0, sample_rate);

        Self {
            ap1,
            delay1: DelayLine::new(v.delay1_len.saturating_add(1)),
            damp1,
            ap2,
            delay2: DelayLine::new(v.delay2_len.saturating_add(1)),
        }
    }

    fn reset(&mut self) {
        self.ap1.reset();
        self.delay1.clear();
        self.damp1.reset();
        self.ap2.reset();
        self.delay2.clear();
    }
}

pub struct PlateLexicon {
    // Input bandwidth control
    bandwidth: Lp1,
    // Input diffusion (4 series allpass)
    input_diffuser: [Allpass; 4],

    // The two loops. Same shape, different lengths, modulation rates
    // and phases — a `Loop` each rather than ten `loop_a_*` /
    // `loop_b_*` fields walked in parallel.
    loop_a: Loop,
    loop_b: Loop,

    // Parameters
    // DC blockers on the loop cross-feeds.
    dc_a: DcBlocker,
    dc_b: DcBlocker,
    decay: f64,
    s: f64, // sample rate scale factor
    sample_rate: f64,
}

impl PlateLexicon {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        // Lexicon 224 reference rate ~30000 Hz (slightly different from Dattorro's 29761)
        let s = sample_rate / 30000.0;

        // Input diffuser delays (Lexicon-style shorter cascade)
        let id = [
            num::f64_to_index(113.0 * s),
            num::f64_to_index(162.0 * s),
            num::f64_to_index(241.0 * s),
            num::f64_to_index(339.0 * s),
        ];

        // Loop A — nested pair; Loop B offset for stereo.
        let loop_a = Loop::new(
            &LoopVoicing {
                ap1_len: num::f64_to_index(547.0 * s),
                delay1_len: num::f64_to_index(3571.0 * s),
                ap2_len: num::f64_to_index(1187.0 * s),
                delay2_len: num::f64_to_index(2833.0 * s),
                ap1_rate: 0.8,
                ap2_rate: 1.2,
                ap1_phase: 0.0,
                ap2_phase: 0.25,
            },
            s,
            sample_rate,
        );
        let loop_b = Loop::new(
            &LoopVoicing {
                ap1_len: num::f64_to_index(709.0 * s),
                delay1_len: num::f64_to_index(3373.0 * s),
                ap2_len: num::f64_to_index(1493.0 * s),
                delay2_len: num::f64_to_index(2999.0 * s),
                ap1_rate: 0.9,
                ap2_rate: 1.1,
                ap1_phase: 0.5,
                ap2_phase: 0.75,
            },
            s,
            sample_rate,
        );

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
            loop_a,
            loop_b,
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
        self.loop_a.reset();
        self.loop_b.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay
        self.decay = params.decay.mul_add(0.69, 0.3);

        // Damping
        let freq = (1.0 - params.damping).mul_add(14000.0, 2000.0);
        self.loop_a.damp1.set_freq(freq, self.sample_rate);
        self.loop_b.damp1.set_freq(freq, self.sample_rate);

        // Input bandwidth — Lexicon characteristic: brighter input than Dattorro
        let bw_freq = params.damping.mul_add(-0.3, 1.0).mul_add(14000.0, 6000.0);
        self.bandwidth.set_freq(bw_freq, self.sample_rate);

        // Diffusion — affects both input diffusers and loop AP feedback
        let id1 = params.diffusion.mul_add(0.15, 0.55);
        let id2 = params.diffusion.mul_add(0.15, 0.45);
        self.input_diffuser[0].set_feedback(id1);
        self.input_diffuser[1].set_feedback(id1);
        self.input_diffuser[2].set_feedback(id2);
        self.input_diffuser[3].set_feedback(id2);

        let loop_fb1 = -params.diffusion.mul_add(0.2, 0.5);
        let loop_fb2 = params.diffusion.mul_add(0.15, 0.4);
        self.loop_a.ap1.feedback = loop_fb1;
        self.loop_b.ap1.feedback = loop_fb1;
        self.loop_a.ap2.feedback = loop_fb2;
        self.loop_b.ap2.feedback = loop_fb2;

        // Modulation — Lexicon has more modulation points than Dattorro
        let mod_depth = params.modulation * 20.0 * self.s;
        self.loop_a
            .ap1
            .set_modulation(0.8, mod_depth, self.sample_rate);
        self.loop_a
            .ap2
            .set_modulation(1.2, mod_depth * 0.7, self.sample_rate);
        self.loop_b
            .ap1
            .set_modulation(0.9, mod_depth, self.sample_rate);
        self.loop_b
            .ap2
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
            .tick(self.loop_b.delay2.read(num::f64_to_index(2999.0 * s)));
        let fb_b = self
            .dc_b
            .tick(self.loop_a.delay2.read(num::f64_to_index(2833.0 * s)));

        // --- Loop A ---
        // AP1 (modulated, negative feedback — characteristic Lexicon)
        let a_ap1 = self.loop_a.ap1.tick(x + fb_a * self.decay);
        // Delay 1
        self.loop_a.delay1.write(a_ap1);
        let a_d1 = self.loop_a.delay1.read(num::f64_to_index(3571.0 * s));
        // Damping + decay
        let a_damped = self.loop_a.damp1.tick(a_d1) * self.decay;
        // AP2 (modulated — extra modulation point vs Dattorro)
        let a_ap2 = self.loop_a.ap2.tick(a_damped);
        // Delay 2
        self.loop_a.delay2.write(a_ap2);

        // --- Loop B ---
        let b_ap1 = self.loop_b.ap1.tick(x + fb_b * self.decay);
        self.loop_b.delay1.write(b_ap1);
        let b_d1 = self.loop_b.delay1.read(num::f64_to_index(3373.0 * s));
        let b_damped = self.loop_b.damp1.tick(b_d1) * self.decay;
        let b_ap2 = self.loop_b.ap2.tick(b_damped);
        self.loop_b.delay2.write(b_ap2);

        // Multi-tap output — Lexicon-style decorrelated tapping
        // More taps than Dattorro for smoother stereo field
        let out_l = self.loop_a.delay1.read(num::f64_to_index(213.0 * s))
            + self.loop_a.delay1.read(num::f64_to_index(2491.0 * s))
            - self.loop_b.delay1.read(num::f64_to_index(1571.0 * s))
            + self.loop_b.delay2.read(num::f64_to_index(1667.0 * s))
            - self.loop_a.delay2.read(num::f64_to_index(887.0 * s))
            - self.loop_b.delay2.read(num::f64_to_index(2311.0 * s));

        let out_r = self.loop_b.delay1.read(num::f64_to_index(281.0 * s))
            + self.loop_b.delay1.read(num::f64_to_index(2719.0 * s))
            - self.loop_a.delay1.read(num::f64_to_index(1831.0 * s))
            + self.loop_a.delay2.read(num::f64_to_index(1423.0 * s))
            - self.loop_b.delay2.read(num::f64_to_index(773.0 * s))
            - self.loop_a.delay2.read(num::f64_to_index(2143.0 * s));

        (out_l * 0.22, out_r * 0.22)
    }
}
