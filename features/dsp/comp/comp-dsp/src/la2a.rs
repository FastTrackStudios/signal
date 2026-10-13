//! An LA-2A, built from a measured one.
//!
//! Two knobs — Peak Reduction and Gain — and no attack, release, ratio or
//! threshold control, because the unit has none. What it has is a T4 optical
//! cell ([`crate::opto`]) and a front panel that moves the threshold.
//!
//! Every constant below was read off a `UADx` LA-2A Gray rather than chosen:
//! the static surface was captured by sweeping Peak Reduction at seven
//! stimulus levels, and the makeup curve by sweeping Gain with Peak Reduction
//! off, so what the gain measured *was* the makeup.
//!
//! # Why tables and not formulae
//!
//! `threshold ≈ -3.63 - 49.42·PR` fits the compressing region well and is
//! wrong in two places that matter. It has the unit compressing below
//! `PR ≈ 0.23`, where the real one does nothing at any level up to 0 dBFS;
//! and it misses that the slope is not constant — the measured ratio falls
//! from 4.5:1 at light settings to 3.7:1 wide open, which is the cell's
//! nonlinearity and precisely the thing worth reproducing. A line through it
//! is a tidier number and a worse model.

use crate::opto::OptoCell;

/// Peak Reduction → threshold in dBFS, measured at 1 kHz.
///
/// `None` below the onset: the unit does not compress there at any level the
/// capture reached, and inventing a very high threshold instead would have it
/// compress on a hot enough signal when the real one does not.
const THRESHOLD_CURVE: &[(f64, Option<f64>)] = &[
    (0.000, None),
    (0.067, None),
    (0.133, None),
    (0.200, None),
    (0.267, Some(-15.2)),
    (0.333, Some(-20.8)),
    (0.400, Some(-25.2)),
    (0.467, Some(-27.7)),
    (0.533, Some(-29.1)),
    (0.600, Some(-31.2)),
    (0.667, Some(-36.1)),
    (0.733, Some(-40.2)),
    (0.800, Some(-43.8)),
    (0.867, Some(-48.2)),
    (0.933, Some(-50.6)),
    (1.000, Some(-51.0)),
];

/// Peak Reduction → the slope of gain reduction against level, as a ratio.
///
/// It is not constant: 4.5:1 lightly compressed, 3.7:1 wide open. An optical
/// cell's photoresistance is nonlinear in the light falling on it, and this is
/// that seen from outside.
const RATIO_CURVE: &[(f64, f64)] = &[
    (0.267, 4.47),
    (0.333, 4.78),
    (0.400, 4.47),
    (0.467, 4.79),
    (0.533, 4.78),
    (0.600, 4.51),
    (0.667, 4.26),
    (0.733, 4.28),
    (0.800, 4.15),
    (0.867, 3.81),
    (0.933, 3.68),
    (1.000, 3.67),
];

/// Gain → makeup in dB.
///
/// Steep, then a plateau: the knob runs out of travel around +14.5 dB. The
/// plugin reports both knobs as dial numbers ("Min, 14, 35, 55…"), not dB, so
/// this could only come from measurement.
const MAKEUP_CURVE: &[(f64, f64)] = &[
    (0.000, -60.00),
    (0.067, -60.00),
    (0.133, -12.55),
    (0.200, -1.57),
    (0.267, 3.53),
    (0.333, 8.63),
    (0.400, 11.76),
    (0.467, 12.55),
    (0.533, 13.33),
    (0.600, 13.73),
    (0.667, 14.12),
    (0.733, 14.12),
    (0.800, 14.12),
    (0.867, 14.51),
    (0.933, 14.51),
    (1.000, 14.51),
];

fn interp(curve: &[(f64, f64)], x: f64) -> f64 {
    let Some(&(first_x, first_y)) = curve.first() else {
        return 0.0;
    };
    if x <= first_x {
        return first_y;
    }
    for pair in curve.windows(2) {
        let [(x0, y0), (x1, y1)] = pair else {
            continue;
        };
        if x <= *x1 {
            return if (x1 - x0).abs() < 1e-12 {
                *y0
            } else {
                y0 + (y1 - y0) * (x - x0) / (x1 - x0)
            };
        }
    }
    curve.last().map_or(0.0, |&(_, y)| y)
}

/// Threshold at a knob position, or `None` where the unit does not compress.
fn threshold_db(pr: f64) -> Option<f64> {
    let pr = pr.clamp(0.0, 1.0);
    // Interpolate only between two points that both compress; crossing the
    // onset, take the compressing one so the transition is not smeared into
    // a threshold the unit never has.
    for w in THRESHOLD_CURVE.windows(2) {
        let &[(x0, a), (x1, b)] = w else {
            continue;
        };
        if pr < x0 {
            break;
        }
        if pr <= x1 {
            return match (a, b) {
                (Some(a), Some(b)) => Some(a + (b - a) * (pr - x0) / (x1 - x0).max(1e-12)),
                (None, Some(b)) => {
                    // Onset sits between these two. Below the midpoint the
                    // unit is still idle.
                    if pr < (x0 + x1) * 0.5 { None } else { Some(b) }
                }
                _ => None,
            };
        }
    }
    THRESHOLD_CURVE.last().and_then(|(_, v)| *v)
}

/// Measured Peak Reduction → static attenuation law for the LA-2A Gray.
#[derive(Debug, Clone, Copy)]
pub struct La2aGainComputer {
    pub peak_reduction: f64,
}
impl crate::components::GainComputer for La2aGainComputer {
    fn reduction_db(&self, level_db: f64) -> f64 {
        let Some(threshold) = threshold_db(self.peak_reduction) else {
            return 0.0;
        };
        let ratio = interp(RATIO_CURVE, self.peak_reduction).max(1.0);
        (level_db - threshold).max(0.0) * (1.0 - 1.0 / ratio)
    }
}

/// Measured LA-2A Gray compression and makeup. Saturation and Limit mode are
/// not fitted by this model; the optical cell documents the release-shape limit.
#[derive(Debug, Clone)]
pub struct La2a {
    core:
        crate::components::Compressor<crate::components::PeakDetector, La2aGainComputer, OptoCell>,
    gain: f64,
}
impl La2a {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut unit = Self {
            core: crate::components::Compressor::new(
                crate::components::PeakDetector::new(sample_rate, 2.0),
                La2aGainComputer {
                    peak_reduction: 0.232,
                },
                OptoCell::new(sample_rate),
                crate::components::Transparent,
            ),
            gain: 0.286,
        };
        unit.set_gain(0.286);
        unit
    }
    pub const fn set_peak_reduction(&mut self, pr: f64) {
        self.core.gain_computer.peak_reduction = pr.clamp(0.0, 1.0);
    }
    pub fn set_gain(&mut self, gain: f64) {
        self.gain = gain.clamp(0.0, 1.0);
        self.core.makeup_db = interp(MAKEUP_CURVE, self.gain);
    }
    #[must_use]
    pub const fn makeup_db(&self) -> f64 {
        self.core.makeup_db
    }
    #[must_use]
    pub const fn gain_reduction_db(&self) -> f64 {
        self.core.gain_reduction_db()
    }
    pub fn reset(&mut self) {
        self.core.reset();
    }
    /// Sample-rate changes start a new stream.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let pr = self.core.gain_computer.peak_reduction;
        let gain = self.gain;
        *self = Self::new(sample_rate);
        self.set_peak_reduction(pr);
        self.set_gain(gain);
    }
    pub fn process(&mut self, input: f64) -> f64 {
        self.core.process(input)
    }
    pub(crate) const fn apply_controls(&mut self, prepared: &Self) {
        self.core.gain_computer = prepared.core.gain_computer;
        self.core.makeup_db = prepared.core.makeup_db;
        self.gain = prepared.gain;
    }
    pub fn process_with_sidechain(&mut self, input: f64, sidechain: f64) -> f64 {
        self.core.process_with_sidechain(input, sidechain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// Settled gain, in dB, for a steady sine at `level_db` peak.
    fn settled_gain_db(pr: f64, gain: f64, level_db: f64) -> f64 {
        let mut unit = La2a::new(SR);
        unit.set_peak_reduction(pr);
        unit.set_gain(gain);
        unit.reset();
        let amp = 10.0f64.powf(level_db / 20.0);
        let n = (SR * 2.0) as usize;
        let mut in_energy = 0.0;
        let mut out_energy = 0.0;
        for i in 0..n {
            let x = amp * (std::f64::consts::TAU * 1000.0 * i as f64 / SR).sin();
            let y = unit.process(x);
            // Measure only the settled tail, as the capture does.
            if i > n * 3 / 4 {
                in_energy += x * x;
                out_energy += y * y;
            }
        }
        10.0 * (out_energy.max(1e-30) / in_energy.max(1e-30)).log10()
    }

    /// The measured surface: (Peak Reduction, stimulus dBFS, settled gain dB),
    /// UADx LA-2A Gray at 1 kHz with Gain at its default 0.286.
    const SURFACE: &[(f64, f64, f64)] = &[
        (0.00, -36.0, 4.94),
        (0.00, 0.0, 4.52),
        (0.27, -12.0, 2.40),
        (0.27, -6.0, -2.05),
        (0.27, 0.0, -6.92),
        (0.40, -18.0, -0.35),
        (0.40, -12.0, -5.01),
        (0.40, -6.0, -10.09),
        (0.40, 0.0, -14.75),
        (0.53, -24.0, 0.92),
        (0.53, -12.0, -8.61),
        (0.53, 0.0, -17.93),
        (0.67, -24.0, -3.95),
        (0.67, -12.0, -13.69),
        (0.67, 0.0, -22.59),
        (0.80, -24.0, -10.31),
        (0.80, -12.0, -19.62),
        (0.80, 0.0, -27.67),
        (0.93, -24.0, -14.75),
        (0.93, -12.0, -23.65),
        (0.93, 0.0, -31.27),
    ];

    #[test]
    fn the_makeup_curve_matches_the_measured_gain_knob() {
        // Gain at its default must give the +4.94 dB the plugin gave.
        let mut unit = La2a::new(SR);
        unit.set_gain(0.286);
        assert!(
            (unit.makeup_db() - 4.94).abs() < 0.6,
            "{:.2}",
            unit.makeup_db()
        );
        // And it plateaus rather than climbing forever.
        unit.set_gain(1.0);
        let top = unit.makeup_db();
        unit.set_gain(0.8);
        assert!(
            (top - unit.makeup_db()).abs() < 1.0,
            "the top of the knob should plateau"
        );
    }

    #[test]
    fn it_does_not_compress_below_the_onset() {
        // Peak Reduction under about 0.23 does nothing at any level, which a
        // straight-line threshold fit gets wrong.
        for pr in [0.0, 0.1, 0.2] {
            let g = settled_gain_db(pr, 0.286, 0.0);
            assert!(
                (g - 4.94).abs() < 1.0,
                "PR {pr} at 0 dBFS should be makeup only, got {g:.2} dB"
            );
        }
    }

    #[test]
    fn the_static_surface_matches_the_measured_unit() {
        let mut worst: (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0);
        let mut sum = 0.0;
        for &(pr, level, want) in SURFACE {
            let got = settled_gain_db(pr, 0.286, level);
            let err = (got - want).abs();
            sum += err;
            if err > worst.3 {
                worst = (pr, level, got, err);
            }
        }
        let mean = sum / SURFACE.len() as f64;
        // Measured at 0.29 dB mean, 0.73 dB worst. The bound is set just above
        // that so a regression is caught rather than absorbed.
        assert!(
            mean < 0.45,
            "mean error {mean:.2} dB over {} points; worst {:.2} dB at PR {:.2}, {} dBFS (got {:.2})",
            SURFACE.len(),
            worst.3,
            worst.0,
            worst.1,
            worst.2
        );
    }

    #[test]
    fn report_the_surface_against_the_measured_unit() {
        // A printout, not an assertion: how close the model sits to the
        // plugin at every measured point. Run with --nocapture.
        println!("\n     PR   level   plugin    model     err");
        let mut sum = 0.0;
        for &(pr, level, want) in SURFACE {
            let got = settled_gain_db(pr, 0.286, level);
            sum += (got - want).abs();
            println!(
                "  {pr:>5.2}{level:>8.0}{want:>9.2}{got:>9.2}{:>8.2}",
                got - want
            );
        }
        println!("  mean absolute error {:.2} dB", sum / SURFACE.len() as f64);
    }

    #[test]
    fn more_peak_reduction_compresses_harder() {
        let mut previous = f64::INFINITY;
        for pr in [0.27, 0.40, 0.53, 0.67, 0.80, 0.93] {
            let g = settled_gain_db(pr, 0.286, -6.0);
            assert!(g < previous, "PR {pr} gave {g:.2}, not below {previous:.2}");
            previous = g;
        }
    }

    #[test]
    fn a_silent_input_does_not_divide_by_zero() {
        let mut unit = La2a::new(SR);
        for _ in 0..1000 {
            assert!(unit.process(0.0).is_finite());
        }
    }
}
