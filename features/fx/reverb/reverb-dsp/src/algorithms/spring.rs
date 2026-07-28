//! Classic spring reverb — Välimäki/Parker/Abel parametric model.
//!
//! Based on "Parametric Spring Reverberation Effect" (JAES, 2010) and
//! "Efficient Dispersion Generation Structures" (EURASIP, 2011).
//!
//! Models a 2-spring Accutronics-style tank with:
//!   - Spectral delay filter (stretched allpass cascade) for dispersion chirp
//!   - Feedback delay loop with modulated delay for echo smearing
//!   - Lowpass in feedback for frequency-dependent decay
//!   - Parallel springs with different lengths/chirp for stereo width
//!
//! Signal flow per spring:
//!
//! ```text
//!   Input → (+) → [Spectral Delay Filter] → Output tap
//!            ↑                              ↓
//!            +-- [× gain] ← [LP] ← [Modulated Delay] ←+
//! ```
//!
//! The spectral delay filter creates frequency-dependent group delay:
//! low frequencies arrive first, highs arrive later → characteristic chirp.
//! Each trip around the feedback loop applies dispersion again, making
//! successive echoes progressively more "chirpy" and diffuse.

use crate::algorithm::{SpringDwell, SpringParams, AlgorithmParams, ReverbAlgorithm};
use crate::primitives::one_pole::Lp1;
use crate::primitives::spectral_delay::SpectralDelay;
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;

use std::f64::consts::PI;

/// One physical spring with dispersion + feedback loop.
struct SpringUnit {
    /// Spectral delay filter — the chirp generator.
    dispersion: SpectralDelay,
    /// Feedback delay line (echo spacing).
    delay: DelayLine,
    delay_samples: usize,
    /// Lowpass in feedback path (frequency-dependent decay).
    damp: Lp1,
    /// Chirp EQ: low-mid resonance (~183 Hz, BW ~146 Hz per the
    /// DAFx-11 measured model) emphasizing the chirp body.
    chirp_eq: audiocore_dsp::biquad::Biquad,
    /// DC blocker to prevent DC buildup in feedback loop.
    dc_blocker: DcBlocker,
    /// Feedback loop gain.
    loop_gain: f64,
    /// Delay modulation state (correlated noise for echo smearing).
    mod_phase: f64,
    mod_rate: f64,
    mod_depth: f64,
    /// Feedback state.
    feedback: f64,
}

impl SpringUnit {
    #[allow(clippy::too_many_arguments)]
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
    ) -> Self {
        let delay_samples = (sample_rate * delay_ms * 0.001) as usize;
        // Allocate for maximum possible delay + modulation headroom
        let max_delay = (sample_rate * max_delay_ms * 0.001) as usize + 32;

        let mut damp = Lp1::new();
        damp.set_freq(damp_freq, sample_rate);
        let mut chirp_eq = audiocore_dsp::biquad::Biquad::new();
        chirp_eq.set(
            audiocore_dsp::biquad::FilterType::Peak { gain_db: 4.5 },
            183.0,
            183.0 / 146.0, // Q from the measured bandwidth
            sample_rate,
        );

        Self {
            dispersion: SpectralDelay::new(num_sections, stretch, ap_coeff),
            delay: DelayLine::new(max_delay + 1),
            delay_samples,
            damp,
            dc_blocker: DcBlocker::with_cutoff(38.0, 48000.0), // matches the old 0.995 pole
            loop_gain: 0.8,
            chirp_eq,
            mod_phase: 0.0,
            mod_rate: mod_rate / sample_rate,
            mod_depth,
            feedback: 0.0,
        }
    }

    fn reset(&mut self) {
        self.chirp_eq.reset();
        self.dispersion.reset();
        self.delay.clear();
        self.damp.reset();
        self.dc_blocker.reset();
        self.feedback = 0.0;
        self.mod_phase = 0.0;
    }

    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        // Mix input with feedback from delay loop
        let x = input + self.feedback;

        // Spectral delay filter — creates the chirp
        let dispersed = self.dispersion.tick(x);

        // Write to feedback delay line
        self.delay.write(dispersed);

        // Modulated read from delay line (smears successive echoes)
        self.mod_phase += self.mod_rate;
        if self.mod_phase > 1.0 {
            self.mod_phase -= 1.0;
        }
        let mod_offset = (self.mod_phase * 2.0 * PI).sin() * self.mod_depth;
        let read_pos = self.delay_samples as f64 + mod_offset;
        let read_int = read_pos as usize;
        let frac = read_pos - read_int as f64;

        // Linear interpolation between two delay line samples
        let s0 = self.delay.read(read_int);
        let s1 = self.delay.read(read_int + 1);
        let delayed = s0 + (s1 - s0) * frac;

        // Frequency-dependent decay (lowpass in feedback)
        let damped = self.damp.tick(delayed);

        // DC blocker prevents runaway DC in the loop
        let clean = self.dc_blocker.tick(damped);

        // Store feedback for next sample. NEGATIVE loop gain: measured
        // spring models (Gamper/Parker/Välimäki DAFx-11, g_lf ≈ −0.8)
        // fit alternating-polarity echoes — the flip each pass is part
        // of the "drip" character.
        self.feedback = clean * -self.loop_gain;

        // Output: dispersed signal through the chirp EQ (low-mid
        // resonance emphasizing the chirp body).
        self.chirp_eq.tick(dispersed, 0)
    }
}

/// Classic 2-spring reverb tank.
pub struct Spring {
    spring_a: SpringUnit,
    spring_b: SpringUnit,
    /// Third spring (mid length/character) for the 3-spring tank.
    spring_c: SpringUnit,
    /// Active springs 1–3 (manual "Number of Springs").
    num_springs: usize,
    /// Preamp drive stage (manual "Dwell").
    dwell: SpringDwell,
    /// Input lowpass (band-limiting before springs).
    input_lp: Lp1,
    /// Output tone control.
    tone_lp: Lp1,
    sample_rate: f64,
}

impl Spring {
    pub fn new(sample_rate: f64) -> Self {
        // Maximum delay for any parameter setting:
        // delay_a_max = 20 + 35 = 55ms, delay_b_max = 55 * 1.38 = 75.9ms
        // mod_depth_max = 7 samples + headroom
        let max_delay_ms = 80.0; // Covers all parameter combinations

        // Spring A: shorter, brighter, moderate chirp
        // ~80 sections × stretch 4 = equivalent to ~320 unit-delay allpasses
        let spring_a = SpringUnit::new(
            sample_rate,
            30.0, // 30ms echo delay
            max_delay_ms,
            80,     // allpass sections
            4,      // stretch factor
            0.55,   // allpass coefficient (moderate chirp)
            5000.0, // damping LP freq
            0.7,    // mod rate Hz
            3.0,    // mod depth samples
        );

        // Spring B: longer, darker, stronger chirp
        // Slightly detuned for stereo decorrelation
        let spring_b = SpringUnit::new(
            sample_rate,
            42.0, // 42ms echo delay (different from A)
            max_delay_ms,
            100,    // more sections (chirpier)
            4,      // stretch factor
            0.58,   // slightly different coefficient
            4000.0, // darker damping
            0.5,    // different mod rate
            3.5,    // slightly more mod
        );

        // Spring C: mid length, its own detune — engages with the
        // 3-spring tank for denser interference.
        let spring_c = SpringUnit::new(
            sample_rate,
            36.0, // between A and B
            max_delay_ms,
            90,     // sections
            4,      // stretch factor
            0.565,  // coefficient between A and B
            4500.0, // damping between A and B
            0.6,    // mod rate
            3.2,    // mod depth
        );

        let mut input_lp = Lp1::new();
        input_lp.set_freq(8000.0, sample_rate);
        let mut tone_lp = Lp1::new();
        tone_lp.set_freq(6000.0, sample_rate);

        Self {
            spring_a,
            spring_b,
            spring_c,
            num_springs: 2,
            dwell: SpringDwell::Clean,
            input_lp,
            tone_lp,
            sample_rate,
        }
    }
}

impl ReverbAlgorithm for Spring {
    fn reset(&mut self) {
        self.spring_c.reset();
        self.spring_a.reset();
        self.spring_b.reset();
        self.input_lp.reset();
        self.tone_lp.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay → loop gain (dwell control)
        // 0.0 → short splashy decay, 1.0 → long sustain
        let gain = 0.5 + params.decay * 0.45; // 0.5 to 0.95
        self.spring_a.loop_gain = gain;
        self.spring_b.loop_gain = gain;

        // Size → echo delay length (spring physical length)
        let delay_a = 20.0 + params.size * 35.0; // 20ms to 55ms
        let delay_b = delay_a * 1.38; // Spring B is ~38% longer
        self.spring_a.delay_samples = (self.sample_rate * delay_a * 0.001) as usize;
        self.spring_b.delay_samples = (self.sample_rate * delay_b * 0.001) as usize;

        // Diffusion → allpass coefficient (chirp intensity / "drip" amount)
        // Low diffusion = mild chirp, high = aggressive drippy chirp
        let ap_a = 0.35 + params.diffusion * 0.35; // 0.35 to 0.70
        let ap_b = ap_a + 0.03; // Spring B slightly chirpier
        self.spring_a.dispersion.coefficient = ap_a;
        self.spring_b.dispersion.coefficient = ap_b;

        // Also adjust number of active sections with diffusion
        let sections_a = 40 + (params.diffusion * 80.0) as usize; // 40 to 120
        let sections_b = 50 + (params.diffusion * 100.0) as usize; // 50 to 150
        self.spring_a.dispersion.active_sections = sections_a;
        self.spring_b.dispersion.active_sections = sections_b;

        // Damping → feedback LP frequency
        let damp_a = 2000.0 + (1.0 - params.damping) * 8000.0; // 2k to 10k
        let damp_b = damp_a * 0.8; // Spring B always darker
        self.spring_a.damp.set_freq(damp_a, self.sample_rate);
        self.spring_b.damp.set_freq(damp_b, self.sample_rate);

        // Modulation → delay modulation depth (echo smearing)
        let mod_depth = 1.0 + params.modulation * 6.0; // 1 to 7 samples
        self.spring_a.mod_depth = mod_depth;
        self.spring_b.mod_depth = mod_depth * 1.2;

        // Tone → output LP
        let tone_freq = 3000.0 + (1.0 + params.tone) * 0.5 * 9000.0; // 3k to 12k
        self.tone_lp.set_freq(tone_freq, self.sample_rate);

        // Input bandwidth
        let input_freq = 4000.0 + (1.0 + params.tone) * 0.5 * 8000.0;
        self.input_lp.set_freq(input_freq, self.sample_rate);

        // Extra A → spring tension (adjusts mod rate — tighter = less flutter)
        let mod_rate_a = 0.3 + (1.0 - params.extra_a) * 1.5; // 0.3 to 1.8 Hz
        let mod_rate_b = mod_rate_a * 0.7;
        self.spring_a.mod_rate = mod_rate_a / self.sample_rate;
        self.spring_b.mod_rate = mod_rate_b / self.sample_rate;
    }

    fn set_spring_params(&mut self, params: &SpringParams) -> bool {
        self.dwell = params.dwell;
        self.num_springs = (params.springs as usize).clamp(1, 3);
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let mono = (left + right) * 0.5;

        // Dwell: preamp drive INTO the tank. Tube and up add harmonic
        // content (soft asymmetry) before the springs, like cranking an
        // outboard unit's Dwell.
        let drive = self.dwell.drive();
        let driven = if drive > 1.001 {
            let x = mono * drive;
            let asym = if self.dwell == SpringDwell::Clean || self.dwell == SpringDwell::Combo {
                x
            } else {
                x + 0.12 * x * x.abs()
            };
            asym.tanh() / drive.tanh()
        } else {
            mono
        };
        let input = self.input_lp.tick(driven);

        // Active springs: 1 = A centered, 2 = A/B panned, 3 = +C center.
        let a_out = self.spring_a.tick(input);
        let (out_l, out_r);
        match self.num_springs {
            1 => {
                out_l = a_out * 0.5;
                out_r = a_out * 0.5;
            }
            3 => {
                let b_out = self.spring_b.tick(input);
                let c_out = self.spring_c.tick(input);
                out_l = a_out * 0.55 + b_out * 0.25 + c_out * 0.33;
                out_r = a_out * 0.25 + b_out * 0.55 + c_out * 0.33;
            }
            _ => {
                let b_out = self.spring_b.tick(input);
                out_l = a_out * 0.65 + b_out * 0.35;
                out_r = a_out * 0.35 + b_out * 0.65;
            }
        }

        // Output tone filtering
        let final_l = self.tone_lp.tick(out_l);
        let final_r = self.tone_lp.tick(out_r);

        (final_l, final_r)
    }
}
