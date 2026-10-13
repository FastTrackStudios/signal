//! Soft clipping functions from Airwindows — sin(), golden ratio, slew-aware.

use std::f64::consts::FRAC_PI_2;

// r[impl dsp.clip.sin]
/// Sin-based soft clip (from ClipSoftly, Pressure6).
/// Maps [-pi/2, pi/2] through sin(), preserving zero crossings.
#[inline]
pub fn sin_clip(sample: f64) -> f64 {
    sample.clamp(-FRAC_PI_2, FRAC_PI_2).sin()
}

// r[impl dsp.clip.golden]
// r[impl dsp.clip.golden.state]
/// Golden ratio interpolated hard clip (from ClipOnly2, ADClip8).
/// When transitioning out of clip, blends using phi to control intersample peaks.
pub struct GoldenClip {
    was_clipped_l: bool,
    was_clipped_r: bool,
    last_l: f64,
    last_r: f64,
}

const INV_PHI: f64 = 0.618033988749894; // 1/φ, φ = 1.618033988749894
const ONE_MINUS_INV_PHI: f64 = 0.381966011250105;

impl GoldenClip {
    pub fn new() -> Self {
        Self {
            was_clipped_l: false,
            was_clipped_r: false,
            last_l: 0.0,
            last_r: 0.0,
        }
    }

    #[inline]
    pub fn tick(&mut self, sample: f64, ch: usize) -> f64 {
        let (was_clipped, last) = match ch {
            0 => (&mut self.was_clipped_l, &mut self.last_l),
            _ => (&mut self.was_clipped_r, &mut self.last_r),
        };

        let output = if sample > 1.0 {
            *was_clipped = true;
            1.0
        } else if sample < -1.0 {
            *was_clipped = true;
            -1.0
        } else if *was_clipped {
            // Transitioning out of clip — golden ratio interpolation
            *was_clipped = false;
            (sample * ONE_MINUS_INV_PHI) + (*last * INV_PHI)
        } else {
            sample
        };

        *last = output;
        output
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for GoldenClip {
    fn default() -> Self {
        Self::new()
    }
}
