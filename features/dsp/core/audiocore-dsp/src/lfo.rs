//! Phase accumulator + waveform shapes — the LFO primitive that was
//! re-rolled (with drifting wrap styles) across the fx crates.

use core::f64::consts::TAU;

/// Common LFO waveforms. Value range is [-1, 1] over phase [0, 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    Square,
    /// Falls from +1 to −1 over the cycle.
    Saw,
    /// Rises from −1 to +1 over the cycle.
    Ramp,
}

impl LfoShape {
    /// Waveform value at `phase` in [0, 1).
    #[inline]
    pub fn value(self, phase: f64) -> f64 {
        match self {
            LfoShape::Sine => (TAU * phase).sin(),
            LfoShape::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
            LfoShape::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::Saw => 1.0 - 2.0 * phase,
            LfoShape::Ramp => 2.0 * phase - 1.0,
        }
    }
}

/// A bare phase accumulator in [0, 1): set a rate, tick once per
/// sample, read the phase or a shaped value. No allocation, no state
/// beyond the phase itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct Phasor {
    phase: f64,
}

impl Phasor {
    pub fn new() -> Self {
        Self { phase: 0.0 }
    }

    pub fn with_phase(phase: f64) -> Self {
        Self {
            phase: phase.rem_euclid(1.0),
        }
    }

    /// Advance by `rate_hz` at `sample_rate`; returns the NEW phase.
    #[inline]
    pub fn tick(&mut self, rate_hz: f64, sample_rate: f64) -> f64 {
        self.phase += rate_hz / sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        if self.phase < 0.0 {
            self.phase += 1.0;
        }
        self.phase
    }

    #[inline]
    pub fn phase(&self) -> f64 {
        self.phase
    }

    pub fn set_phase(&mut self, phase: f64) {
        self.phase = phase.rem_euclid(1.0);
    }

    /// Advance and evaluate a shape in one call.
    #[inline]
    pub fn tick_shape(&mut self, shape: LfoShape, rate_hz: f64, sample_rate: f64) -> f64 {
        shape.value(self.tick(rate_hz, sample_rate))
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phasor_wraps_and_stays_in_range() {
        let mut p = Phasor::new();
        for _ in 0..100_000 {
            let ph = p.tick(3.7, 48_000.0);
            assert!((0.0..1.0).contains(&ph));
        }
    }

    #[test]
    fn shapes_span_full_range() {
        for shape in [
            LfoShape::Sine,
            LfoShape::Triangle,
            LfoShape::Square,
            LfoShape::Saw,
            LfoShape::Ramp,
        ] {
            let mut min = f64::MAX;
            let mut max = f64::MIN;
            for i in 0..1000 {
                let v = shape.value(i as f64 / 1000.0);
                min = min.min(v);
                max = max.max(v);
            }
            assert!(max > 0.99 && min < -0.99, "{shape:?}: {min}..{max}");
        }
    }
}
