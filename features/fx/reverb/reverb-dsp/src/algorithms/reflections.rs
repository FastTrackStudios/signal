//! Reflections reverb — psychoacoustically accurate early reflections.
//!
//! Based on Strymon BigSky Reflections: computes geometrically-derived
//! early reflections based on source position within a modeled room.
//! Pure early reflections with no late reverb tail.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::multitap_delay::{MultitapDelay, Tap};
use crate::primitives::one_pole::Lp1;

/// Maximum number of reflection taps.
const MAX_REFLECTIONS: usize = 64;

pub struct Reflections {
    taps_l: MultitapDelay,
    taps_r: MultitapDelay,
    // Per-tap damping (simulates wall absorption)
    damp_l: Lp1,
    damp_r: Lp1,
    sample_rate: f64,
}

impl Reflections {
    pub fn new(sample_rate: f64) -> Self {
        let max_delay = (sample_rate * 0.3) as usize; // 300ms max reflection

        let mut refl = Self {
            taps_l: MultitapDelay::new(max_delay),
            taps_r: MultitapDelay::new(max_delay),
            damp_l: Lp1::new(),
            damp_r: Lp1::new(),
            sample_rate,
        };

        refl.damp_l.set_freq(8000.0, sample_rate);
        refl.damp_r.set_freq(8000.0, sample_rate);
        refl.generate_reflections(0.5, 0.5);

        refl
    }

    /// Generate reflection taps based on room size and source position.
    fn generate_reflections(&mut self, size: f64, position: f64) {
        let scale = self.sample_rate / 48000.0;
        let room_scale = 0.2 + size * 1.6; // Room size multiplier

        // Generate geometrically-inspired reflection pattern
        // Simulates first and second order reflections in a rectangular room
        let mut taps_l = Vec::with_capacity(MAX_REFLECTIONS);
        let mut taps_r = Vec::with_capacity(MAX_REFLECTIONS);

        // Source offset from center (affects L/R timing)
        let offset = (position - 0.5) * 2.0; // -1.0 to 1.0

        // First-order reflections (walls, floor, ceiling)
        let base_delays = [
            37.0, 53.0, 79.0, 97.0, 127.0, 151.0, // Direct walls
            181.0, 211.0, 251.0, 293.0, 337.0, 383.0, // Floor/ceiling
        ];
        let base_gains = [
            0.85, 0.78, 0.72, 0.65, 0.58, 0.50, 0.44, 0.38, 0.32, 0.27, 0.22, 0.18,
        ];

        for (i, (&delay, &gain)) in base_delays.iter().zip(base_gains.iter()).enumerate() {
            let d = (delay * scale * room_scale) as usize;
            // Offset L/R timing based on source position
            let lr_offset = (offset * delay * 0.15 * scale) as isize;
            let d_l = (d as isize + lr_offset).max(1) as usize;
            let d_r = (d as isize - lr_offset).max(1) as usize;

            // Alternating polarity for some reflections
            let polarity = if i % 3 == 0 { -1.0 } else { 1.0 };

            taps_l.push(Tap {
                delay_samples: d_l,
                gain: gain * polarity,
            });
            taps_r.push(Tap {
                delay_samples: d_r,
                gain: gain * polarity,
            });
        }

        // Second-order reflections (wall-to-wall bounces)
        let mut rng = audiocore_dsp::prng::XorShift32::new(12345);
        for i in 0..MAX_REFLECTIONS.saturating_sub(base_delays.len()) {
            let r = (rng.next() as f64) / (u32::MAX as f64);
            let delay = 400.0 + r * 2000.0;
            let d = (delay * scale * room_scale) as usize;
            let gain = 0.15 * (0.97_f64).powi(i as i32);
            let lr_offset = ((rng.next_bipolar()) * delay * 0.1 * scale) as isize;

            taps_l.push(Tap {
                delay_samples: (d as isize + lr_offset).max(1) as usize,
                gain,
            });
            taps_r.push(Tap {
                delay_samples: (d as isize - lr_offset).max(1) as usize,
                gain,
            });
        }

        self.taps_l.set_taps(&taps_l);
        self.taps_r.set_taps(&taps_r);
    }
}

impl ReverbAlgorithm for Reflections {
    fn reset(&mut self) {
        self.taps_l.reset();
        self.taps_r.reset();
        self.damp_l.reset();
        self.damp_r.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Regenerate reflections when size or position changes
        self.generate_reflections(params.size, params.extra_a);

        // Damping -> wall absorption
        let freq = 2000.0 + (1.0 - params.damping) * 14000.0;
        self.damp_l.set_freq(freq, self.sample_rate);
        self.damp_r.set_freq(freq, self.sample_rate);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let out_l = self.taps_l.tick(left);
        let out_r = self.taps_r.tick(right);

        // Apply wall absorption damping
        let damped_l = self.damp_l.tick(out_l);
        let damped_r = self.damp_r.tick(out_r);

        (damped_l, damped_r)
    }
}
