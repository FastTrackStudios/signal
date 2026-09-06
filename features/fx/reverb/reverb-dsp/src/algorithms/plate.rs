//! Dattorro plate reverb.
//!
//! Faithful implementation of Jon Dattorro's "Effect Design Part 1:
//! Reverberator and Other Filters" (JAES, 1997).
//!
//! Full topology:
//!   Input → Bandwidth LP → 4× Input Diffusion AP
//!   → Tank A (`decay_diffusion_1` AP → `delay_4` → damp → ×decay
//!            → `decay_diffusion_2` AP → `delay_5` → cross-feed to B)
//!   → Tank B (`decay_diffusion_1` AP → `delay_6` → damp → ×decay
//!            → `decay_diffusion_2` AP → `delay_7` → cross-feed to A)
//!   → 7-tap output per channel from delays AND allpass filters.
//!
//! All delay lengths from Dattorro's published values at 29761 Hz
//! reference rate, scaled to the actual sample rate.

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, PLATE_DECAY_APPLICATIONS, PLATE_LOOP_SECONDS, PLATE_T60, ReverbAlgorithm, dattorro_gain_for_t60, decay_to_t60};
use crate::primitives::allpass::Allpass;
use crate::primitives::modulated_allpass::ModulatedAllpass;
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

/// Dattorro plate reverb — complete published topology.
/// Steel-plate dispersion: a cascade of identical first-order
/// allpasses with negative coefficient. Group delay at DC is
/// (1−a)/(1+a) per stage vs (1+a)/(1−a) at Nyquist — lows lag, highs
/// arrive first, the plate's signature DOWNWARD chirp on transients
/// (bending waves travel faster at high frequency in stiff plates).
struct Dispersion {
    state: Vec<f64>,
    coeff: f64,
}

impl Dispersion {
    fn new(stages: usize, coeff: f64) -> Self {
        Self {
            state: vec![0.0; stages],
            coeff,
        }
    }

    #[inline]
    fn tick(&mut self, mut x: f64) -> f64 {
        let a = self.coeff;
        for st in &mut self.state {
            // First-order allpass H(z) = (a + z⁻¹)/(1 + a·z⁻¹).
            let y = a.mul_add(x, *st);
            *st = a.mul_add(-y, x);
            x = y;
        }
        x
    }

    fn reset(&mut self) {
        self.state.fill(0.0);
    }
}

/// One half of the Dattorro figure-8: allpass, delay, damping lowpass,
/// second allpass, second delay.
struct Tank {
    /// `decay_diffusion_1`, modulated.
    ap1: ModulatedAllpass,
    delay1: DelayLine,
    /// Damping lowpass.
    damp: Lp1,
    /// `decay_diffusion_2`, static.
    ap2: Allpass,
    delay2: DelayLine,
}

/// The four Dattorro delay lengths of one tank, already scaled to the
/// host rate.
struct TankLengths {
    ap1: usize,
    delay1: usize,
    ap2: usize,
    delay2: usize,
}

impl Tank {
    /// `phase` staggers the two tanks' allpass modulation.
    fn new(lengths: &TankLengths, phase: f64, s: f64, sample_rate: f64) -> Self {
        let mut ap1 = ModulatedAllpass::new();
        ap1.sample_delay = lengths.ap1;
        ap1.feedback = -0.7; // Negative sign per Dattorro
        ap1.set_modulation(1.0, 16.0 * s, sample_rate);
        ap1.set_phase(phase);

        let mut ap2 = Allpass::new(lengths.ap2);
        ap2.set_delay(lengths.ap2);
        ap2.set_feedback(0.5);

        let mut damp = Lp1::new();
        damp.set_freq(8000.0, sample_rate);

        Self {
            ap1,
            delay1: DelayLine::new(lengths.delay1.saturating_add(1)),
            damp,
            ap2,
            delay2: DelayLine::new(lengths.delay2.saturating_add(1)),
        }
    }

    fn reset(&mut self) {
        self.ap1.reset();
        self.delay1.clear();
        self.damp.reset();
        self.ap2.reset();
        self.delay2.clear();
    }
}

pub struct Plate {
    // Input bandwidth control (1-pole LP)
    bandwidth: Lp1,
    // Input diffusers (4 series allpass filters)
    input_diffuser: [Allpass; 4],

    // The figure-8's two tanks. Same shape, different lengths and
    // modulation phase — a `Tank` each rather than ten `tank_a_*` /
    // `tank_b_*` fields walked in parallel.
    tank_a: Tank,
    tank_b: Tank,

    // Parameters
    // DC blockers on the tank cross-feeds — the recirculating
    // figure-8 otherwise accumulates subsonic offset.
    dc_a: DcBlocker,
    dc_b: DcBlocker,
    decay: f64,
    decay_diffusion_1: f64,
    decay_diffusion_2: f64,
    /// Input dispersion (96 stages, a = −0.55 ≈ 6 ms LF-vs-HF spread).
    dispersion: Dispersion,
    /// Decoupled low-frequency decay: one-pole crossover per tank with
    /// its own feedback multiplier (`low_decay_mult` from Low End).
    lf_split_a: Lp1,
    lf_split_b: Lp1,
    low_decay_mult: f64,

    // Cached scale factor
    s: f64,
    sample_rate: f64,
}

impl Plate {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let s = sample_rate / 29761.0; // Dattorro reference rate

        // Input diffuser delay lengths
        let id = [
            num::f64_to_index(142.0 * s),
            num::f64_to_index(107.0 * s),
            num::f64_to_index(379.0 * s),
            num::f64_to_index(277.0 * s),
        ];

        // Tank delay lengths
        let tank_a = Tank::new(
            &TankLengths {
                ap1: num::f64_to_index(672.0 * s),
                delay1: num::f64_to_index(4453.0 * s),
                ap2: num::f64_to_index(1800.0 * s),
                delay2: num::f64_to_index(3720.0 * s),
            },
            0.0,
            s,
            sample_rate,
        );
        let tank_b = Tank::new(
            &TankLengths {
                ap1: num::f64_to_index(908.0 * s),
                delay1: num::f64_to_index(4217.0 * s),
                ap2: num::f64_to_index(2656.0 * s),
                delay2: num::f64_to_index(3163.0 * s),
            },
            0.5,
            s,
            sample_rate,
        );

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
            tank_a,
            tank_b,
            dc_a: DcBlocker::new(),
            dc_b: DcBlocker::new(),
            decay: 0.7,
            decay_diffusion_1: 0.7,
            decay_diffusion_2: 0.5,
            dispersion: Dispersion::new(96, -0.55),
            lf_split_a: Lp1::new(),
            lf_split_b: Lp1::new(),
            low_decay_mult: 1.0,
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
        self.dispersion.reset();
        self.lf_split_a.reset();
        self.lf_split_b.reset();
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
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay (0.0 → short plate, 1.0 → infinite sustain)
        // Tank gain from a target reverberation time, so `decay` means the
        // same number of seconds here as it does in Hall, Room and Random.
        // Previously this was a bare gain ramp (0.3..0.99) with no relation
        // to time, so `decay_time` did nothing on a plate at all.
        let t60 = decay_to_t60(params.decay, PLATE_T60.0, PLATE_T60.1);
        self.decay =
            dattorro_gain_for_t60(t60, PLATE_LOOP_SECONDS, PLATE_DECAY_APPLICATIONS);

        // Damping → tank LP cutoff (2k–16k Hz)
        let freq = (1.0 - params.damping).mul_add(14000.0, 2000.0);
        self.tank_a.damp.set_freq(freq, self.sample_rate);
        self.tank_b.damp.set_freq(freq, self.sample_rate);

        // Decoupled LF decay (real plates: low decay can run longer or
        // shorter than the mids — ValhallaPlate's core lesson). The Low
        // End param scales the sub-crossover feedback independently.
        self.low_decay_mult = params.low_decay_mult.clamp(0.25, 1.4);
        self.lf_split_a
            .set_freq(params.band_crossover_hz.max(80.0), self.sample_rate);
        self.lf_split_b
            .set_freq(params.band_crossover_hz.max(80.0), self.sample_rate);

        // Input bandwidth (tone control)
        let bw_freq = params.damping.mul_add(-0.5, 1.0).mul_add(12000.0, 4000.0);
        self.bandwidth.set_freq(bw_freq, self.sample_rate);

        // Diffusion → decay_diffusion_1 and input diffuser strength
        self.decay_diffusion_1 = params.diffusion.mul_add(0.2, 0.5); // 0.5–0.7
        self.decay_diffusion_2 = params.diffusion.mul_add(0.15, 0.35); // 0.35–0.5
        self.tank_a.ap1.feedback = -self.decay_diffusion_1; // Negative per Dattorro
        self.tank_b.ap1.feedback = -self.decay_diffusion_1;
        self.tank_a.ap2.set_feedback(self.decay_diffusion_2);
        self.tank_b.ap2.set_feedback(self.decay_diffusion_2);

        // Input diffusion strength
        let id1 = params.diffusion.mul_add(0.15, 0.6); // 0.6–0.75
        let id2 = params.diffusion.mul_add(0.125, 0.5); // 0.5–0.625
        self.input_diffuser[0].set_feedback(id1);
        self.input_diffuser[1].set_feedback(id1);
        self.input_diffuser[2].set_feedback(id2);
        self.input_diffuser[3].set_feedback(id2);

        // Modulation depth
        let mod_depth = params.modulation * 24.0 * self.s;
        self.tank_a.ap1
            .set_modulation(1.0, mod_depth, self.sample_rate);
        self.tank_b.ap1
            .set_modulation(1.0, mod_depth, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Mono sum → bandwidth limit → input diffusion
        let input = (left + right) * 0.5;
        let bw = self.bandwidth.tick(input);

        // Plate dispersion: downward-chirped transients before the
        // diffusers (highs reach the pickups first on real steel).
        let mut x = self.dispersion.tick(bw);
        for d in &mut self.input_diffuser {
            x = d.tick(x);
        }

        // ---- Read cross-feed from the END of each tank ----
        // Tank A feeds from end of tank_b.delay2, Tank B from end of tank_a.delay2
        let s = self.s;
        let fb_a = self
            .dc_a
            .tick(self.tank_b.delay2.read(num::f64_to_index(3163.0 * s)));
        let fb_b = self
            .dc_b
            .tick(self.tank_a.delay2.read(num::f64_to_index(3720.0 * s)));

        // ---- Tank A processing ----
        // decay_diffusion_1 AP (modulated)
        let fb_a = {
            let low = self.lf_split_a.tick(fb_a);
            low * (self.decay * self.low_decay_mult).min(0.997) + (fb_a - low) * self.decay
        };
        let a_ap1_out = self.tank_a.ap1.tick(x + fb_a);
        // delay_4
        self.tank_a.delay1.write(a_ap1_out);
        let a_d1_out = self.tank_a.delay1.read(num::f64_to_index(4453.0 * s));
        // damping LP → multiply by decay
        let a_damped = self.tank_a.damp.tick(a_d1_out) * self.decay;
        // decay_diffusion_2 AP
        let a_ap2_out = self.tank_a.ap2.tick(a_damped);
        // delay_5
        self.tank_a.delay2.write(a_ap2_out);

        // ---- Tank B processing ----
        let fb_b = {
            let low = self.lf_split_b.tick(fb_b);
            low * (self.decay * self.low_decay_mult).min(0.997) + (fb_b - low) * self.decay
        };
        let b_ap1_out = self.tank_b.ap1.tick(x + fb_b);
        self.tank_b.delay1.write(b_ap1_out);
        let b_d1_out = self.tank_b.delay1.read(num::f64_to_index(4217.0 * s));
        let b_damped = self.tank_b.damp.tick(b_d1_out) * self.decay;
        let b_ap2_out = self.tank_b.ap2.tick(b_damped);
        self.tank_b.delay2.write(b_ap2_out);

        // ---- 7-tap output per channel — Dattorro 1997, Table 2 ----
        //
        //   yL = a[266] + a[2974] - b[1913] + c[1996]
        //      - d[1990] - e[187]  - f[1066]
        //   yR = d[353] + d[3627] - e[1228] + f[2673]
        //      - a[2111] - b[335]  - c[121]
        //
        // where a = delay_6 (tank B delay 1), b = AP 2656 (tank B ap2,
        // tapped mid-buffer), c = delay_7 (tank B delay 2), d = delay_4
        // (tank A delay 1), e = AP 1800 (tank A ap2), f = delay_5
        // (tank A delay 2). Earlier revisions approximated the allpass
        // taps with adjacent delay lines and had drifted tank/sign
        // assignments; Allpass::tap restores the published matrix.
        let out_l = self.tank_b.delay1.read(num::f64_to_index(266.0 * s))
            + self.tank_b.delay1.read(num::f64_to_index(2974.0 * s))
            - self.tank_b.ap2.tap(num::f64_to_index(1913.0 * s))
            + self.tank_b.delay2.read(num::f64_to_index(1996.0 * s))
            - self.tank_a.delay1.read(num::f64_to_index(1990.0 * s))
            - self.tank_a.ap2.tap(num::f64_to_index(187.0 * s))
            - self.tank_a.delay2.read(num::f64_to_index(1066.0 * s));

        let out_r = self.tank_a.delay1.read(num::f64_to_index(353.0 * s))
            + self.tank_a.delay1.read(num::f64_to_index(3627.0 * s))
            - self.tank_a.ap2.tap(num::f64_to_index(1228.0 * s))
            + self.tank_a.delay2.read(num::f64_to_index(2673.0 * s))
            - self.tank_b.delay1.read(num::f64_to_index(2111.0 * s))
            - self.tank_b.ap2.tap(num::f64_to_index(335.0 * s))
            - self.tank_b.delay2.read(num::f64_to_index(121.0 * s));

        // Paper output scale is 0.6; 0.25 preserves this port's level
        // relative to the other algorithms (chain-level normalization).
        (out_l * 0.25, out_r * 0.25)
    }
}
