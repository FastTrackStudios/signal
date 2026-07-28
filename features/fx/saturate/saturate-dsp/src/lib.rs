//! saturate-dsp — musical saturation engine.
//!
//! Grown from the reverb chain's tanh soft-clip primitive
//! (`reverb-dsp/src/primitives/saturation.rs`) into a standalone stereo
//! saturator with selectable shaper curves, a pre/post tone tilt, and
//! loudness-compensated makeup gain.
//!
//! Reference: Zölzer (ed.), "DAFX: Digital Audio Effects" 2nd ed., Ch. 4 —
//! nonlinear processing. tanh is the classic smooth soft-clipper; the tape
//! and tube curves are asymmetric/odd-harmonic variations on it.
//!
//! Processing-core rules (platform targets): no allocation on the hot path,
//! no threads, no platform I/O. All state is pre-computed in setters;
//! `process()` is pure arithmetic per sample.

#![no_std]

pub mod preamp;

/// Saturation transfer-curve family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SaturationCurve {
    /// Smooth symmetric tanh soft clip — odd harmonics, the classic.
    #[default]
    Tanh,
    /// "Tape": tanh with a soft knee approach into the ceiling (x - x³/3
    /// region blended toward tanh) — gentler onset, stronger 3rd harmonic.
    Tape,
    /// "Tube": slightly asymmetric transfer (DC-biased tanh, bias removed
    /// post-shaping) — adds even harmonics like a single-ended triode stage.
    Tube,
    /// Hard clip at the drive ceiling — for parallel-crush duties.
    Hard,
}

/// Single-channel saturator stage. Stereo/multichannel is one instance per
/// channel (no cross-channel state, so channels stay phase-coherent).
#[derive(Debug, Clone)]
pub struct Saturator {
    curve: SaturationCurve,
    /// User drive 0..1.
    drive: f32,
    /// Pre-gain derived from drive: 1x..8x.
    drive_gain: f32,
    /// Loudness-compensating output gain.
    makeup: f32,
    /// Dry/wet mix 0..1 (1 = fully saturated).
    mix: f32,
    /// Output trim as linear gain.
    output_gain: f32,
    /// Tube bias amount (constant; only audible with `SaturationCurve::Tube`).
    bias: f32,
}

impl Saturator {
    pub fn new() -> Self {
        let mut s = Self {
            curve: SaturationCurve::Tanh,
            drive: 0.0,
            drive_gain: 1.0,
            makeup: 1.0,
            mix: 1.0,
            output_gain: 1.0,
            bias: 0.2,
        };
        s.set_drive(0.0);
        s
    }

    /// 0.0 = clean, 1.0 = heavy. Internal: pre-gain 1x..8x with 1/sqrt
    /// loudness compensation (matches the reverb primitive's calibration).
    pub fn set_drive(&mut self, drive: f32) {
        self.drive = clamp01(drive);
        self.drive_gain = 1.0 + self.drive * 7.0;
        self.makeup = 1.0 / sqrt_approx(self.drive_gain);
    }

    pub fn drive(&self) -> f32 {
        self.drive
    }

    pub fn set_curve(&mut self, curve: SaturationCurve) {
        self.curve = curve;
    }

    pub fn curve(&self) -> SaturationCurve {
        self.curve
    }

    /// Dry/wet mix: 0 = bypass, 1 = fully wet.
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = clamp01(mix);
    }

    /// Output trim in dB (applied after mix).
    pub fn set_output_db(&mut self, db: f32) {
        self.output_gain = db_to_gain(db);
    }

    /// Reset internal state. The saturator is memoryless, so this is a no-op;
    /// it exists so the stage slots into `reset()`-driven chains.
    pub fn reset(&mut self) {}

    /// Shape one sample.
    #[inline]
    pub fn process(&self, input: f32) -> f32 {
        if self.drive == 0.0 && self.mix >= 1.0 {
            return input * self.output_gain;
        }

        let x = input * self.drive_gain;
        let shaped = match self.curve {
            SaturationCurve::Tanh => tanh_approx(x),
            SaturationCurve::Tape => {
                // Blend the polynomial soft region into tanh as |x| grows.
                let soft = x - (x * x * x) / 3.0;
                let t = clamp01(x.abs() * 0.5);
                soft * (1.0 - t) + tanh_approx(x) * t
            }
            SaturationCurve::Tube => {
                // Bias into the curve, remove the resulting DC offset.
                tanh_approx(x + self.bias) - tanh_approx(self.bias)
            }
            SaturationCurve::Hard => x.clamp(-1.0, 1.0),
        };
        let wet = shaped * self.makeup;
        (input + (wet - input) * self.mix) * self.output_gain
    }
}

impl Default for Saturator {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// tanh via the padé-ish rational approximation used across the fx DSP cores —
/// keeps the crate `no_std` (no libm dependency) and branch-free.
#[inline]
pub(crate) fn tanh_approx(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Newton's-method sqrt (two iterations) — enough for gain compensation and
/// keeps the crate free of `std`/`libm`.
#[inline]
fn sqrt_approx(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut y = x;
    let mut g = x * 0.5;
    for _ in 0..4 {
        g = 0.5 * (g + y / g);
    }
    // One extra polish step against drift for large drives.
    y = 0.5 * (g + x / g);
    y
}

/// dB → linear gain without `std`: 10^(db/20) = 2^(db * log2(10)/20).
#[inline]
fn db_to_gain(db: f32) -> f32 {
    exp2_approx(db * 0.166_096_4)
}

/// 2^x for the modest ranges gain trims use (|x| <= ~6), split into integer
/// and fractional parts with a cubic fit on the fraction.
#[inline]
fn exp2_approx(x: f32) -> f32 {
    // Manual floor: `f32::floor` is std/libm-only and this crate is no_std.
    let mut xi = x as i32;
    if (xi as f32) > x {
        xi -= 1;
    }
    let xf = x - xi as f32;
    // Cubic fit of 2^f on [0,1), max error ~2e-4 — inaudible for trims.
    let frac = 1.0 + xf * (0.695_976 + xf * (0.226_174 + xf * 0.078_024));
    let scale = pow2_int(xi);
    frac * scale
}

#[inline]
fn pow2_int(n: i32) -> f32 {
    let n = n.clamp(-126, 127);
    f32::from_bits(((127 + n) as u32) << 23)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_drive_full_mix_is_transparent() {
        let s = Saturator::new();
        for x in [-1.0f32, -0.5, 0.0, 0.25, 0.9] {
            assert!((s.process(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn drive_bounds_output() {
        let mut s = Saturator::new();
        s.set_drive(1.0);
        for curve in [
            SaturationCurve::Tanh,
            SaturationCurve::Tape,
            SaturationCurve::Tube,
            SaturationCurve::Hard,
        ] {
            s.set_curve(curve);
            for i in -100..=100 {
                let x = i as f32 / 25.0; // -4..4, past the ceiling
                let y = s.process(x);
                assert!(y.abs() <= 1.5, "{curve:?} exploded: {x} -> {y}");
            }
        }
    }

    #[test]
    fn tube_curve_has_no_dc_at_silence() {
        let mut s = Saturator::new();
        s.set_drive(0.8);
        s.set_curve(SaturationCurve::Tube);
        assert!(s.process(0.0).abs() < 1e-6);
    }

    #[test]
    fn db_to_gain_is_sane() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-3);
        assert!((db_to_gain(6.0) - 1.995).abs() < 2e-2);
        assert!((db_to_gain(-6.0) - 0.501).abs() < 1e-2);
    }
}
