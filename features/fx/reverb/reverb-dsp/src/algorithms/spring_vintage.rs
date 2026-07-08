//! Vintage spring reverb — 3-spring tank with lo-fi character.
//!
//! Same Välimäki/Parker/Abel parametric architecture as the Classic,
//! but modeled after a vintage tube amp spring tank:
//!
//!   - 3 springs (short/medium/long) for richer, denser character
//!   - More allpass sections (chirpier, more "drippy" transients)
//!   - Soft saturation in the feedback path (tube-like warmth)
//!   - Band-limited I/O (lo-fi vintage character)
//!   - Stronger delay modulation (mechanical flutter from loose mounting)
//!   - Higher feedback gains possible (self-oscillation at extremes)
//!
//! The 3 springs create a complex interference pattern that's denser
//! and warmer than the 2-spring Classic tank.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::one_pole::Lp1;
use crate::primitives::spectral_delay::SpectralDelay;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

use std::f64::consts::PI;

/// One spring with dispersion + feedback loop + saturation.
struct VintageSpringUnit {
    dispersion: SpectralDelay,
    delay: DelayLine,
    delay_samples: usize,
    damp: Lp1,
    dc_blocker: DcBlocker,
    loop_gain: f64,
    mod_phase: f64,
    mod_rate: f64,
    mod_depth: f64,
    feedback: f64,
    /// Saturation drive (0.0 = clean, 1.0+ = overdriven).
    drive: f64,
}

impl VintageSpringUnit {
    fn new(
        sample_rate: f64,
        delay_ms: f64,
        max_delay_ms: f64,
        num_sections: usize,
        stretch: usize,
        ap_coeff: f64,
        damp_freq: f64,
        mod_rate: f64,
        mod_depth: f64,
        initial_phase: f64,
    ) -> Self {
        let delay_samples = (sample_rate * delay_ms * 0.001) as usize;
        let max_delay = (sample_rate * max_delay_ms * 0.001) as usize + 48;

        let mut damp = Lp1::new();
        damp.set_freq(damp_freq, sample_rate);

        Self {
            dispersion: SpectralDelay::new(num_sections, stretch, ap_coeff),
            delay: DelayLine::new(max_delay + 1),
            delay_samples,
            damp,
            dc_blocker: DcBlocker::with_cutoff(38.0, 48000.0), // matches the old 0.995 pole
            loop_gain: 0.82,
            mod_phase: initial_phase,
            mod_rate: mod_rate / sample_rate,
            mod_depth,
            feedback: 0.0,
            drive: 0.5,
        }
    }

    fn reset(&mut self) {
        self.dispersion.reset();
        self.delay.clear();
        self.damp.reset();
        self.dc_blocker.reset();
        self.feedback = 0.0;
    }

    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        // Mix input with saturated feedback
        let fb_saturated = soft_clip(self.feedback * (1.0 + self.drive));
        let x = input + fb_saturated;

        // Spectral delay filter — chirp
        let dispersed = self.dispersion.tick(x);

        // Write to feedback delay
        self.delay.write(dispersed);

        // Modulated delay read (more flutter than classic — vintage loose mounting)
        self.mod_phase += self.mod_rate;
        if self.mod_phase > 1.0 {
            self.mod_phase -= 1.0;
        }
        // Use a more complex modulation shape (sum of two sines for irregular flutter)
        let mod_sig = (self.mod_phase * 2.0 * PI).sin() * 0.7
            + (self.mod_phase * 2.0 * PI * 1.47).sin() * 0.3; // Irrational ratio
        let mod_offset = mod_sig * self.mod_depth;
        let read_pos = self.delay_samples as f64 + mod_offset;
        let read_pos = read_pos.max(1.0);
        let read_int = read_pos as usize;
        let frac = read_pos - read_int as f64;

        let s0 = self.delay.read(read_int);
        let s1 = self.delay.read(read_int + 1);
        let delayed = s0 + (s1 - s0) * frac;

        // Damping + saturation in feedback
        let damped = self.damp.tick(delayed);
        let clean = self.dc_blocker.tick(damped);

        self.feedback = clean * self.loop_gain;

        dispersed
    }
}

/// Soft saturation — approximation of tube amp character.
/// Uses tanh-like shape for smooth clipping.
#[inline]
fn soft_clip(x: f64) -> f64 {
    // Fast tanh approximation: x / (1 + |x|)
    // Smoother than hard clip, preserves zero crossing
    x / (1.0 + x.abs())
}

/// Vintage 3-spring reverb tank.
pub struct SpringVintage {
    spring_a: VintageSpringUnit,
    spring_b: VintageSpringUnit,
    spring_c: VintageSpringUnit,
    /// Input band-limiting (vintage character).
    input_lp: Lp1,
    input_hp: Lp1,
    /// Output tone.
    output_lp: Lp1,
    /// Number of active springs (1–3).
    num_active: usize,
    sample_rate: f64,
}

impl SpringVintage {
    pub fn new(sample_rate: f64) -> Self {
        // Three springs with different characteristics for a rich, dense sound.
        // Vintage tanks: short + medium + long spring, more aggressive chirp.

        // Maximum delay: base_c_max = (18+25)*2.2 = 94.6ms + mod headroom
        let max_delay_ms = 110.0;

        // Spring A: short, bright, quick chirp (lead transducer side)
        let spring_a = VintageSpringUnit::new(
            sample_rate,
            25.0, // 25ms — short spring
            max_delay_ms,
            100,    // More sections than classic = chirpier
            4,      // stretch
            0.60,   // Higher coefficient = more chirp
            4500.0, // Darker than classic
            0.9,    // Faster flutter
            5.0,    // More mod depth (loose vintage tank)
            0.0,    // Phase offset
        );

        // Spring B: medium, warm
        let spring_b = VintageSpringUnit::new(
            sample_rate,
            38.0, // 38ms — medium spring
            max_delay_ms,
            120, // Even more sections
            4,
            0.62,   // Slightly more chirp
            3500.0, // Darker
            0.65,
            5.5,
            0.33, // Phase offset for decorrelation
        );

        // Spring C: long, dark, most chirp (distinctive drip)
        let spring_c = VintageSpringUnit::new(
            sample_rate,
            55.0, // 55ms — long spring
            max_delay_ms,
            150, // Maximum chirp
            4,
            0.65,   // Most chirp
            2800.0, // Darkest
            0.45,
            6.0,  // Most modulation
            0.67, // Phase offset
        );

        // Vintage input is band-limited (smaller transducer)
        let mut input_lp = Lp1::new();
        input_lp.set_freq(5000.0, sample_rate);
        let mut input_hp = Lp1::new();
        input_hp.set_freq(120.0, sample_rate);
        let mut output_lp = Lp1::new();
        output_lp.set_freq(4500.0, sample_rate);

        Self {
            spring_a,
            spring_b,
            spring_c,
            input_lp,
            input_hp,
            output_lp,
            num_active: 3,
            sample_rate,
        }
    }
}

impl ReverbAlgorithm for SpringVintage {
    fn reset(&mut self) {
        self.spring_a.reset();
        self.spring_b.reset();
        self.spring_c.reset();
        self.input_lp.reset();
        self.input_hp.reset();
        self.output_lp.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay → loop gain (vintage tanks can go into self-oscillation)
        let gain = 0.45 + params.decay * 0.5; // 0.45 to 0.95
        self.spring_a.loop_gain = gain;
        self.spring_b.loop_gain = gain;
        self.spring_c.loop_gain = gain;

        // Size → spring lengths
        let base_a = 18.0 + params.size * 25.0; // 18ms to 43ms
        let base_b = base_a * 1.5; // 50% longer
        let base_c = base_a * 2.2; // 120% longer
        self.spring_a.delay_samples = (self.sample_rate * base_a * 0.001) as usize;
        self.spring_b.delay_samples = (self.sample_rate * base_b * 0.001) as usize;
        self.spring_c.delay_samples = (self.sample_rate * base_c * 0.001) as usize;

        // Diffusion → chirp intensity (allpass coefficient + section count)
        let ap_a = 0.40 + params.diffusion * 0.30; // 0.40 to 0.70
        let ap_b = ap_a + 0.02;
        let ap_c = ap_a + 0.05; // Long spring is always chirpiest
        self.spring_a.dispersion.coefficient = ap_a;
        self.spring_b.dispersion.coefficient = ap_b;
        self.spring_c.dispersion.coefficient = ap_c;

        let sec_a = 50 + (params.diffusion * 100.0) as usize; // 50-150
        let sec_b = 60 + (params.diffusion * 120.0) as usize; // 60-180
        let sec_c = 80 + (params.diffusion * 140.0) as usize; // 80-220
        self.spring_a.dispersion.active_sections = sec_a;
        self.spring_b.dispersion.active_sections = sec_b;
        self.spring_c.dispersion.active_sections = sec_c;

        // Damping → feedback LP
        let damp_a = 1500.0 + (1.0 - params.damping) * 5500.0; // 1.5k to 7k (darker than classic)
        let damp_b = damp_a * 0.8;
        let damp_c = damp_a * 0.6;
        self.spring_a.damp.set_freq(damp_a, self.sample_rate);
        self.spring_b.damp.set_freq(damp_b, self.sample_rate);
        self.spring_c.damp.set_freq(damp_c, self.sample_rate);

        // Modulation → delay modulation depth (vintage flutter)
        let mod_depth = 2.0 + params.modulation * 10.0; // 2 to 12 samples (more than classic)
        self.spring_a.mod_depth = mod_depth;
        self.spring_b.mod_depth = mod_depth * 1.15;
        self.spring_c.mod_depth = mod_depth * 1.3;

        // Tone → output LP + input LP
        let tone_freq = 2000.0 + (1.0 + params.tone) * 0.5 * 5000.0; // 2k to 7k (lo-fi range)
        self.output_lp.set_freq(tone_freq, self.sample_rate);
        let input_freq = 3000.0 + (1.0 + params.tone) * 0.5 * 4000.0; // 3k to 7k
        self.input_lp.set_freq(input_freq, self.sample_rate);

        // Extra A → saturation drive (tube warmth)
        let drive = params.extra_a * 2.0; // 0.0 to 2.0
        self.spring_a.drive = drive;
        self.spring_b.drive = drive;
        self.spring_c.drive = drive;

        // Extra B → number of active springs
        self.num_active = if params.extra_b < 0.33 {
            1
        } else if params.extra_b < 0.66 {
            2
        } else {
            3
        };
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let mono = (left + right) * 0.5;

        // Vintage band-limited input
        let lp = self.input_lp.tick(mono);
        let hp_removed = self.input_hp.tick(mono);
        let input = lp - hp_removed + mono * 0.15; // Mostly LP, subtract LF, add a touch of full-range

        // Process active springs
        let a_out = self.spring_a.tick(input);
        let b_out = if self.num_active >= 2 {
            self.spring_b.tick(input)
        } else {
            0.0
        };
        let c_out = if self.num_active >= 3 {
            self.spring_c.tick(input)
        } else {
            0.0
        };

        // Pan 3 springs for stereo:
        // A = left-center, B = right-center, C = center (both)
        let (out_l, out_r) = match self.num_active {
            1 => (a_out, a_out),
            2 => (a_out * 0.7 + b_out * 0.3, a_out * 0.3 + b_out * 0.7),
            _ => (
                a_out * 0.55 + b_out * 0.2 + c_out * 0.35,
                a_out * 0.2 + b_out * 0.55 + c_out * 0.35,
            ),
        };

        // Output tone filtering (vintage roll-off)
        let final_l = self.output_lp.tick(out_l);
        let final_r = self.output_lp.tick(out_r);

        (final_l, final_r)
    }
}
