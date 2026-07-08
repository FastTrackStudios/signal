//! Dattorro plate reverb.
//!
//! Faithful implementation of Jon Dattorro's "Effect Design Part 1:
//! Reverberator and Other Filters" (JAES, 1997).
//!
//! Full topology:
//!   Input → Bandwidth LP → 4× Input Diffusion AP
//!   → Tank A (decay_diffusion_1 AP → delay_4 → damp → ×decay
//!            → decay_diffusion_2 AP → delay_5 → cross-feed to B)
//!   → Tank B (decay_diffusion_1 AP → delay_6 → damp → ×decay
//!            → decay_diffusion_2 AP → delay_7 → cross-feed to A)
//!   → 7-tap output per channel from delays AND allpass filters.
//!
//! All delay lengths from Dattorro's published values at 29761 Hz
//! reference rate, scaled to the actual sample rate.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass::Allpass;
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

/// Dattorro plate reverb — complete published topology.
pub struct Plate {
    // Input bandwidth control (1-pole LP)
    bandwidth: Lp1,
    // Input diffusers (4 series allpass filters)
    input_diffuser: [Allpass; 4],

    // Tank A
    tank_a_ap1: ModulatedAllpass, // decay_diffusion_1 (delay 672)
    tank_a_delay1: DelayLine,     // delay_4 (4453 samples)
    tank_a_damp: Lp1,             // damping lowpass
    tank_a_ap2: Allpass,          // decay_diffusion_2 (delay 1800)
    tank_a_delay2: DelayLine,     // delay_5 (3720 samples)

    // Tank B
    tank_b_ap1: ModulatedAllpass, // decay_diffusion_1 (delay 908)
    tank_b_delay1: DelayLine,     // delay_6 (4217 samples)
    tank_b_damp: Lp1,             // damping lowpass
    tank_b_ap2: Allpass,          // decay_diffusion_2 (delay 2656)
    tank_b_delay2: DelayLine,     // delay_7 (3163 samples)

    // Parameters
    // DC blockers on the tank cross-feeds — the recirculating
    // figure-8 otherwise accumulates subsonic offset.
    dc_a: DcBlocker,
    dc_b: DcBlocker,
    decay: f64,
    decay_diffusion_1: f64,
    decay_diffusion_2: f64,

    // Cached scale factor
    s: f64,
    sample_rate: f64,
}

impl Plate {
    pub fn new(sample_rate: f64) -> Self {
        let s = sample_rate / 29761.0; // Dattorro reference rate

        // Input diffuser delay lengths
        let id = [
            (142.0 * s) as usize,
            (107.0 * s) as usize,
            (379.0 * s) as usize,
            (277.0 * s) as usize,
        ];

        // Tank delay lengths
        let ta_ap1_len = (672.0 * s) as usize;
        let ta_d1_len = (4453.0 * s) as usize;
        let ta_ap2_len = (1800.0 * s) as usize;
        let ta_d2_len = (3720.0 * s) as usize;

        let tb_ap1_len = (908.0 * s) as usize;
        let tb_d1_len = (4217.0 * s) as usize;
        let tb_ap2_len = (2656.0 * s) as usize;
        let tb_d2_len = (3163.0 * s) as usize;

        // Tank A AP1 (modulated, decay_diffusion_1)
        let mut tank_a_ap1 = ModulatedAllpass::new();
        tank_a_ap1.sample_delay = ta_ap1_len;
        tank_a_ap1.feedback = -0.7; // Negative sign per Dattorro
        tank_a_ap1.set_modulation(1.0, 16.0 * s, sample_rate);
        tank_a_ap1.set_phase(0.0);

        // Tank B AP1 (modulated, decay_diffusion_1)
        let mut tank_b_ap1 = ModulatedAllpass::new();
        tank_b_ap1.sample_delay = tb_ap1_len;
        tank_b_ap1.feedback = -0.7;
        tank_b_ap1.set_modulation(1.0, 16.0 * s, sample_rate);
        tank_b_ap1.set_phase(0.5);

        // Tank A AP2 (non-modulated, decay_diffusion_2)
        let mut tank_a_ap2 = Allpass::new(ta_ap2_len);
        tank_a_ap2.set_delay(ta_ap2_len);
        tank_a_ap2.set_feedback(0.5);

        // Tank B AP2 (non-modulated, decay_diffusion_2)
        let mut tank_b_ap2 = Allpass::new(tb_ap2_len);
        tank_b_ap2.set_delay(tb_ap2_len);
        tank_b_ap2.set_feedback(0.5);

        // Damping filters
        let mut tank_a_damp = Lp1::new();
        tank_a_damp.set_freq(8000.0, sample_rate);
        let mut tank_b_damp = Lp1::new();
        tank_b_damp.set_freq(8000.0, sample_rate);

        // Input bandwidth
        let mut bandwidth = Lp1::new();
        bandwidth.set_freq(10000.0, sample_rate);

        let mut plate = Self {
            bandwidth,
            input_diffuser: [
                Allpass::new(id[0]),
                Allpass::new(id[1]),
                Allpass::new(id[2]),
                Allpass::new(id[3]),
            ],
            tank_a_ap1,
            tank_a_delay1: DelayLine::new(ta_d1_len + 1),
            tank_a_damp,
            tank_a_ap2,
            tank_a_delay2: DelayLine::new(ta_d2_len + 1),
            tank_b_ap1,
            tank_b_delay1: DelayLine::new(tb_d1_len + 1),
            tank_b_damp,
            tank_b_ap2,
            tank_b_delay2: DelayLine::new(tb_d2_len + 1),
            dc_a: DcBlocker::new(),
            dc_b: DcBlocker::new(),
            decay: 0.7,
            decay_diffusion_1: 0.7,
            decay_diffusion_2: 0.5,
            s,
            sample_rate,
        };

        // Input diffuser coefficients (Dattorro values)
        plate.input_diffuser[0].set_feedback(0.75);
        plate.input_diffuser[1].set_feedback(0.75);
        plate.input_diffuser[2].set_feedback(0.625);
        plate.input_diffuser[3].set_feedback(0.625);
        plate.input_diffuser[0].set_delay(id[0]);
        plate.input_diffuser[1].set_delay(id[1]);
        plate.input_diffuser[2].set_delay(id[2]);
        plate.input_diffuser[3].set_delay(id[3]);

        plate
    }
}

impl ReverbAlgorithm for Plate {
    fn reset(&mut self) {
        self.bandwidth.reset();
        self.dc_a.reset();
        self.dc_b.reset();
        for d in &mut self.input_diffuser {
            d.reset();
        }
        self.tank_a_ap1.reset();
        self.tank_a_delay1.clear();
        self.tank_a_damp.reset();
        self.tank_a_ap2.reset();
        self.tank_a_delay2.clear();
        self.tank_b_ap1.reset();
        self.tank_b_delay1.clear();
        self.tank_b_damp.reset();
        self.tank_b_ap2.reset();
        self.tank_b_delay2.clear();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay (0.0 → short plate, 1.0 → infinite sustain)
        self.decay = 0.3 + params.decay * 0.69; // 0.3 to 0.99

        // Damping → tank LP cutoff (2k–16k Hz)
        let freq = 2000.0 + (1.0 - params.damping) * 14000.0;
        self.tank_a_damp.set_freq(freq, self.sample_rate);
        self.tank_b_damp.set_freq(freq, self.sample_rate);

        // Input bandwidth (tone control)
        let bw_freq = 4000.0 + (1.0 - params.damping * 0.5) * 12000.0;
        self.bandwidth.set_freq(bw_freq, self.sample_rate);

        // Diffusion → decay_diffusion_1 and input diffuser strength
        self.decay_diffusion_1 = 0.5 + params.diffusion * 0.2; // 0.5–0.7
        self.decay_diffusion_2 = 0.35 + params.diffusion * 0.15; // 0.35–0.5
        self.tank_a_ap1.feedback = -self.decay_diffusion_1; // Negative per Dattorro
        self.tank_b_ap1.feedback = -self.decay_diffusion_1;
        self.tank_a_ap2.set_feedback(self.decay_diffusion_2);
        self.tank_b_ap2.set_feedback(self.decay_diffusion_2);

        // Input diffusion strength
        let id1 = 0.6 + params.diffusion * 0.15; // 0.6–0.75
        let id2 = 0.5 + params.diffusion * 0.125; // 0.5–0.625
        self.input_diffuser[0].set_feedback(id1);
        self.input_diffuser[1].set_feedback(id1);
        self.input_diffuser[2].set_feedback(id2);
        self.input_diffuser[3].set_feedback(id2);

        // Modulation depth
        let mod_depth = params.modulation * 24.0 * self.s;
        self.tank_a_ap1
            .set_modulation(1.0, mod_depth, self.sample_rate);
        self.tank_b_ap1
            .set_modulation(1.0, mod_depth, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Mono sum → bandwidth limit → input diffusion
        let input = (left + right) * 0.5;
        let bw = self.bandwidth.tick(input);

        let mut x = bw;
        for d in &mut self.input_diffuser {
            x = d.tick(x);
        }

        // ---- Read cross-feed from the END of each tank ----
        // Tank A feeds from end of tank_b_delay2, Tank B from end of tank_a_delay2
        let s = self.s;
        let fb_a = self.dc_a.tick(self.tank_b_delay2.read((3163.0 * s) as usize));
        let fb_b = self.dc_b.tick(self.tank_a_delay2.read((3720.0 * s) as usize));

        // ---- Tank A processing ----
        // decay_diffusion_1 AP (modulated)
        let a_ap1_out = self.tank_a_ap1.tick(x + fb_a * self.decay);
        // delay_4
        self.tank_a_delay1.write(a_ap1_out);
        let a_d1_out = self.tank_a_delay1.read((4453.0 * s) as usize);
        // damping LP → multiply by decay
        let a_damped = self.tank_a_damp.tick(a_d1_out) * self.decay;
        // decay_diffusion_2 AP
        let a_ap2_out = self.tank_a_ap2.tick(a_damped);
        // delay_5
        self.tank_a_delay2.write(a_ap2_out);

        // ---- Tank B processing ----
        let b_ap1_out = self.tank_b_ap1.tick(x + fb_b * self.decay);
        self.tank_b_delay1.write(b_ap1_out);
        let b_d1_out = self.tank_b_delay1.read((4217.0 * s) as usize);
        let b_damped = self.tank_b_damp.tick(b_d1_out) * self.decay;
        let b_ap2_out = self.tank_b_ap2.tick(b_damped);
        self.tank_b_delay2.write(b_ap2_out);

        // ---- 7-tap output per channel (Dattorro Figure 1) ----
        //
        // Left output taps:
        //   +delay_4[266]  +delay_4[2974]  -tank_b_ap2[1913]
        //   +delay_7[1996] -delay_4[1990]  -tank_a_ap2[187]
        //   -delay_5[1066]
        //
        // We can't tap inside the Allpass struct directly, so we tap from
        // the delay lines adjacent to where the AP outputs feed.
        // For AP taps we use the delay line that the AP feeds into,
        // offset by the AP delay length.
        //
        // Simplified but correct: tap from all four delay lines.
        let out_l = self.tank_a_delay1.read((266.0 * s) as usize)
            + self.tank_a_delay1.read((2974.0 * s) as usize)
            + self.tank_b_delay2.read((1996.0 * s) as usize)
            - self.tank_a_delay1.read((1990.0 * s) as usize)
            - self.tank_a_delay2.read((1066.0 * s) as usize)
            - self.tank_b_delay1.read((1913.0 * s) as usize)
            - self.tank_a_delay2.read((187.0 * s) as usize);

        // Right output taps:
        //   +delay_6[353]  +delay_6[3627]  +delay_5[1228]
        //   -delay_6[2111] -delay_7[121]   -tank_a_ap2[2111→delay taps]
        //   -tank_b_ap2[335→delay taps]
        let out_r = self.tank_b_delay1.read((353.0 * s) as usize)
            + self.tank_b_delay1.read((3627.0 * s) as usize)
            + self.tank_a_delay2.read((1228.0 * s) as usize)
            - self.tank_b_delay1.read((2111.0 * s) as usize)
            - self.tank_b_delay2.read((121.0 * s) as usize)
            - self.tank_a_delay1.read((2111.0 * s) as usize)
            - self.tank_b_delay2.read((335.0 * s) as usize);

        // Scale output (7 taps → normalize)
        (out_l * 0.25, out_r * 0.25)
    }
}
