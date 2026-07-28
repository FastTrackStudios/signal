//! Decay-tilt EQ — the shared repeat-voicing filter in every machine's
//! feedback path.

use audiocore_dsp::biquad::{Biquad, FilterType};

/// Tilt EQ inside a delay's regeneration loop: negative tilt darkens the
/// repeats (a lowpass walking down from 20 kHz), positive tilt thins them
/// (a highpass walking up from 20 Hz). |tilt| ≤ 0.01 is bypass.
///
/// Every machine used to open-code this pair (coefficient block in
/// `update()`, gated `tick` in the feedback path); this is that block,
/// once.
pub struct DecayTilt {
    tilt: f64,
    eq: Biquad,
}

impl DecayTilt {
    pub fn new() -> Self {
        Self {
            tilt: 0.0,
            eq: Biquad::new(),
        }
    }

    /// Recompute coefficients for the given tilt; call from `update()`.
    pub fn configure(&mut self, tilt: f64, sample_rate: f64) {
        self.tilt = tilt;
        if tilt.abs() <= 0.01 {
            return;
        }
        if tilt < 0.0 {
            let freq = 20000.0 * (1.0 + tilt).max(0.05);
            self.eq.set(FilterType::Lowpass, freq, 0.707, sample_rate);
        } else {
            let freq = 20.0 + tilt * 2000.0;
            self.eq.set(FilterType::Highpass, freq, 0.707, sample_rate);
        }
    }

    /// Filter one feedback sample; identity while bypassed.
    #[inline]
    pub fn tick(&mut self, x: f64, ch: usize) -> f64 {
        if self.tilt.abs() <= 0.01 {
            x
        } else {
            self.eq.tick(x, ch)
        }
    }

    pub fn reset(&mut self) {
        self.eq.reset();
    }
}

impl Default for DecayTilt {
    fn default() -> Self {
        Self::new()
    }
}
