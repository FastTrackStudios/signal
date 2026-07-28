//! Rotary-arc geometry — the 270° sweep from 135° (7 o'clock) every knob in
//! the suite draws, and the drag feel constants that go with it. One copy;
//! the guitar and keys knobs each had their own.

use std::f64::consts::PI;

/// Arc start: 135° = 7 o'clock.
pub const START_ANGLE: f64 = 135.0;
/// Arc sweep: 270°, ending at 5 o'clock.
pub const SWEEP: f64 = 270.0;
/// Pixels of vertical drag per full 0→1 sweep — the shared drag feel.
pub const SENSITIVITY: f64 = 150.0;

/// The point at `deg` degrees on the circle around `(cx, cy)` with radius `r`.
pub fn arc_point(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let rad = deg * PI / 180.0;
    (cx + r * rad.cos(), cy + r * rad.sin())
}

/// An SVG path drawing the arc from `from` to `to` degrees.
pub fn arc_path(cx: f64, cy: f64, r: f64, from: f64, to: f64) -> String {
    let (x1, y1) = arc_point(cx, cy, r, from);
    let (x2, y2) = arc_point(cx, cy, r, to);
    let large = if (to - from).abs() > 180.0 { 1 } else { 0 };
    format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
}

/// The arc end-angle for a normalized value 0..1.
pub fn angle_for_value(v: f64) -> f64 {
    START_ANGLE + v.clamp(0.0, 1.0) * SWEEP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_maps_onto_the_sweep() {
        assert_eq!(angle_for_value(0.0), START_ANGLE);
        assert_eq!(angle_for_value(1.0), START_ANGLE + SWEEP);
        assert_eq!(angle_for_value(2.0), START_ANGLE + SWEEP); // clamped
    }

    #[test]
    fn arc_path_flags_large_arcs() {
        let small = arc_path(50.0, 50.0, 40.0, 135.0, 200.0);
        assert!(small.contains(" 0 0 1 "));
        let large = arc_path(50.0, 50.0, 40.0, 135.0, 405.0);
        assert!(large.contains(" 0 1 1 "));
    }
}
