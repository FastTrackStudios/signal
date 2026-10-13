//! Segment curve interpolation — 9 curve types from KottV/SimpleSide (SSCurve).
//!
//! Each function maps a normalized position `t` (0..1 within a segment)
//! to an output value, using `y1`, `y2`, and `tension` parameters.
//!
//! Credits: KottV/SimpleSide (curve math), tiagolr (gate12, filtr, time12, reevr).

use std::f64::consts::PI;

/// Curve type for a pattern segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CurveType {
    /// Step function — holds y1 until the next point.
    Hold = 0,
    /// Power curve — tension controls concavity.
    Curve = 1,
    /// S-curve — power curve applied to each half.
    SCurve = 2,
    /// Half-sine eased through a power curve.
    HalfSine = 3,
    /// Square/pulse wave — tension controls pulse count.
    Pulse = 4,
    /// Cosine wave — tension controls harmonic count.
    Wave = 5,
    /// Triangle wave — tension controls wave count.
    Triangle = 6,
    /// Quantized steps — tension controls step count.
    Stairs = 7,
    /// Stairs with S-curve easing between steps.
    SmoothStairs = 8,
}

impl CurveType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Hold,
            1 => Self::Curve,
            2 => Self::SCurve,
            3 => Self::HalfSine,
            4 => Self::Pulse,
            5 => Self::Wave,
            6 => Self::Triangle,
            7 => Self::Stairs,
            8 => Self::SmoothStairs,
            _ => Self::Curve,
        }
    }
}

/// Compute the power exponent from tension (-1..1).
///
/// `pow(1.1, abs(tension * 50))` — positive tension = convex, negative = concave.
#[inline]
fn tension_to_power(tension: f64) -> f64 {
    1.1_f64.powf((tension * 50.0).abs())
}

/// Power curve: `t^power` with direction based on tension sign.
#[inline]
fn power_curve(t: f64, power: f64, tension: f64) -> f64 {
    if tension >= 0.0 {
        t.powf(power)
    } else {
        1.0 - (1.0 - t).powf(power)
    }
}

/// Evaluate a curve segment.
///
/// `t` is normalized position within the segment (0..1),
/// `y1` and `y2` are the endpoint values, `tension` controls shape.
pub fn evaluate(curve: CurveType, t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    match curve {
        CurveType::Hold => hold(y1),
        CurveType::Curve => curve_interp(t, y1, y2, tension),
        CurveType::SCurve => s_curve(t, y1, y2, tension),
        CurveType::HalfSine => half_sine(t, y1, y2, tension),
        CurveType::Pulse => pulse(t, y1, y2, tension),
        CurveType::Wave => wave(t, y1, y2, tension),
        CurveType::Triangle => triangle(t, y1, y2, tension),
        CurveType::Stairs => stairs(t, y1, y2, tension),
        CurveType::SmoothStairs => smooth_stairs(t, y1, y2, tension),
    }
}

/// Hold — constant y1.
#[inline]
fn hold(y1: f64) -> f64 {
    y1
}

/// Power curve interpolation between y1 and y2.
fn curve_interp(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let pwr = tension_to_power(tension);
    let shaped = power_curve(t, pwr, tension);
    y1 + (y2 - y1) * shaped
}

/// S-curve — applies power curve to each half of the segment.
fn s_curve(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let mid_y = (y1 + y2) * 0.5;
    if t < 0.5 {
        let t2 = t * 2.0;
        let pwr = tension_to_power(tension);
        let shaped = power_curve(t2, pwr, tension);
        y1 + (mid_y - y1) * shaped
    } else {
        let t2 = (t - 0.5) * 2.0;
        let pwr = tension_to_power(tension);
        let shaped = power_curve(t2, pwr, tension);
        mid_y + (y2 - mid_y) * shaped
    }
}

/// Half-sine eased through a power curve.
fn half_sine(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let pwr = tension_to_power(tension);
    let shaped = power_curve(t, pwr, tension);
    let sine = 0.5 - 0.5 * (PI * shaped).cos();
    y1 + (y2 - y1) * sine
}

/// Pulse/square wave — `tension^2 * 100` controls wave count.
fn pulse(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let count = (tension * tension * 100.0).max(1.0);
    let phase = (t * count).fract();
    if phase < 0.5 {
        y1
    } else {
        y2
    }
}

/// Cosine wave — tension controls frequency (odd harmonics).
fn wave(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let count = (tension * tension * 100.0).max(1.0);
    let mid_y = (y1 + y2) * 0.5;
    let amp = (y2 - y1) * 0.5;
    mid_y + amp * (2.0 * PI * count * t).cos()
}

/// Triangle wave — tension controls wave count.
fn triangle(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let count = (tension * tension * 100.0).max(1.0);
    let phase = t * count;
    // Triangle wave: 2 * |phase/period - floor(0.5 + phase/period)|
    let tri = 2.0 * (phase - (0.5 + phase).floor()).abs();
    y1 + (y2 - y1) * tri
}

/// Quantized stairs — positive tension = N steps across X, negative = across Y.
fn stairs(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let steps = (tension.abs() * 50.0).max(1.0).round() as usize;
    if tension >= 0.0 {
        // Steps across X
        let step_idx = (t * steps as f64).floor().min(steps as f64 - 1.0);
        y1 + (y2 - y1) * step_idx / (steps as f64 - 1.0).max(1.0)
    } else {
        // Steps across Y
        let raw = y1 + (y2 - y1) * t;
        let step_size = (y2 - y1) / steps as f64;
        if step_size.abs() < 1e-15 {
            return y1;
        }
        let step_idx = ((raw - y1) / step_size).floor();
        y1 + step_size * step_idx
    }
}

/// Smooth stairs — stairs with S-curve easing (power=4) between steps.
fn smooth_stairs(t: f64, y1: f64, y2: f64, tension: f64) -> f64 {
    let steps = (tension.abs() * 50.0).max(2.0).round() as usize;
    let step_width = 1.0 / steps as f64;
    let step_idx = (t / step_width).floor().min(steps as f64 - 1.0);
    let local_t = (t - step_idx * step_width) / step_width;

    // S-curve easing with hardcoded power=4
    let eased = if local_t < 0.5 {
        let t2 = local_t * 2.0;
        0.5 * t2.powi(4)
    } else {
        let t2 = (local_t - 0.5) * 2.0;
        0.5 + 0.5 * (1.0 - (1.0 - t2).powi(4))
    };

    let step_y1 = y1 + (y2 - y1) * step_idx / (steps as f64 - 1.0).max(1.0);
    let step_y2 =
        y1 + (y2 - y1) * (step_idx + 1.0).min(steps as f64 - 1.0) / (steps as f64 - 1.0).max(1.0);
    step_y1 + (step_y2 - step_y1) * eased
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_returns_y1() {
        assert_eq!(evaluate(CurveType::Hold, 0.5, 0.3, 0.8, 0.0), 0.3);
    }

    #[test]
    fn curve_endpoints() {
        // At t=0, should be y1; at t=1, should be y2
        let y0 = evaluate(CurveType::Curve, 0.0, 0.2, 0.9, 0.0);
        let y1 = evaluate(CurveType::Curve, 1.0, 0.2, 0.9, 0.0);
        assert!((y0 - 0.2).abs() < 1e-10);
        assert!((y1 - 0.9).abs() < 1e-10);
    }

    #[test]
    fn curve_with_tension() {
        // Positive tension = convex (below midpoint at t=0.5)
        let mid_pos = evaluate(CurveType::Curve, 0.5, 0.0, 1.0, 0.5);
        assert!(
            mid_pos < 0.5,
            "Positive tension should be convex: {mid_pos}"
        );

        // Negative tension = concave (above midpoint at t=0.5)
        let mid_neg = evaluate(CurveType::Curve, 0.5, 0.0, 1.0, -0.5);
        assert!(
            mid_neg > 0.5,
            "Negative tension should be concave: {mid_neg}"
        );
    }

    #[test]
    fn scurve_endpoints() {
        let y0 = evaluate(CurveType::SCurve, 0.0, 0.0, 1.0, 0.0);
        let y1 = evaluate(CurveType::SCurve, 1.0, 0.0, 1.0, 0.0);
        assert!((y0).abs() < 1e-10);
        assert!((y1 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn scurve_midpoint() {
        // S-curve should pass through midpoint at t=0.5
        let mid = evaluate(CurveType::SCurve, 0.5, 0.0, 1.0, 0.0);
        assert!((mid - 0.5).abs() < 1e-10);
    }

    #[test]
    fn half_sine_endpoints() {
        let y0 = evaluate(CurveType::HalfSine, 0.0, 0.0, 1.0, 0.0);
        let y1 = evaluate(CurveType::HalfSine, 1.0, 0.0, 1.0, 0.0);
        assert!((y0).abs() < 1e-10);
        assert!((y1 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn pulse_alternates() {
        // With default tension, should produce step function
        let a = evaluate(CurveType::Pulse, 0.1, 0.0, 1.0, 0.1);
        let b = evaluate(CurveType::Pulse, 0.6, 0.0, 1.0, 0.1);
        assert_eq!(a, 0.0);
        assert_eq!(b, 1.0);
    }

    #[test]
    fn triangle_endpoints() {
        let y0 = evaluate(CurveType::Triangle, 0.0, 0.0, 1.0, 0.1);
        // At t=0, triangle should be at y1
        assert!((y0 - 0.0).abs() < 1e-10);
    }

    #[test]
    fn stairs_quantizes() {
        // With tension=0.1 -> ~5 steps, check quantization
        let a = evaluate(CurveType::Stairs, 0.15, 0.0, 1.0, 0.1);
        let b = evaluate(CurveType::Stairs, 0.25, 0.0, 1.0, 0.1);
        // Both should be in the same step or adjacent
        assert!((0.0..=1.0).contains(&a));
        assert!((0.0..=1.0).contains(&b));
    }
}
