//! Portable geometry + SVG-path helpers for pattern (MSEG) editors.
//!
//! Framework-free (no Dioxus): everything here maps between the
//! normalized pattern space (`x`, `y` ∈ 0..1, y = 1 at the TOP of the
//! value range) and a pixel viewBox, and renders
//! [`fts_modulation::Pattern`] curves as SVG path strings. The Dioxus
//! component in `signal-ui` (and any future host: plugin editors, the
//! web remote, story previews) wires pointer events to these helpers.
//!
//! Mirrors the `eq-ui` split: interaction math lives here, wasm-clean
//! and unit-tested; the component stays a thin event shim.

use fts_modulation::{CurveType, Pattern, Point};

/// Maps normalized pattern coordinates to a pixel viewBox and back.
///
/// Pattern `y = 1.0` renders at the top (`py = 0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternMapper {
    pub width: f64,
    pub height: f64,
    /// Inset in px so edge points stay grabbable.
    pub pad: f64,
}

impl PatternMapper {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            pad: 6.0,
        }
    }

    #[inline]
    pub fn x_to_px(&self, x: f64) -> f64 {
        self.pad + x.clamp(0.0, 1.0) * (self.width - 2.0 * self.pad)
    }

    #[inline]
    pub fn y_to_px(&self, y: f64) -> f64 {
        self.pad + (1.0 - y.clamp(0.0, 1.0)) * (self.height - 2.0 * self.pad)
    }

    #[inline]
    pub fn px_to_x(&self, px: f64) -> f64 {
        ((px - self.pad) / (self.width - 2.0 * self.pad)).clamp(0.0, 1.0)
    }

    #[inline]
    pub fn px_to_y(&self, py: f64) -> f64 {
        (1.0 - (py - self.pad) / (self.height - 2.0 * self.pad)).clamp(0.0, 1.0)
    }
}

/// Index of the point nearest to pixel `(px, py)` within `radius_px`,
/// or `None`. Points are the editable handles, so hit-testing runs in
/// pixel space (a fixed grab radius regardless of zoom).
pub fn nearest_point(
    points: &[Point],
    mapper: &PatternMapper,
    px: f64,
    py: f64,
    radius_px: f64,
) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, p) in points.iter().enumerate() {
        let dx = mapper.x_to_px(p.x) - px;
        let dy = mapper.y_to_px(p.y) - py;
        let d2 = dx * dx + dy * dy;
        if d2 <= radius_px * radius_px && best.is_none_or(|(_, bd)| d2 < bd) {
            best = Some((i, d2));
        }
    }
    best.map(|(i, _)| i)
}

/// Sample the pattern into an SVG stroke path (`M … L …`) and a closed
/// fill path (down to the bottom edge). `samples` ≥ 2; 128–256 renders
/// smoothly at typical widths.
pub fn pattern_paths(
    pattern: &Pattern,
    mapper: &PatternMapper,
    samples: usize,
) -> (String, String) {
    let n = samples.max(2);
    let mut stroke = String::with_capacity(n * 14 + 8);
    for i in 0..n {
        let x = i as f64 / (n - 1) as f64;
        let px = mapper.x_to_px(x);
        let py = mapper.y_to_px(pattern.get_y(x));
        if i == 0 {
            stroke.push_str(&format!("M{px:.1} {py:.1}"));
        } else {
            stroke.push_str(&format!(" L{px:.1} {py:.1}"));
        }
    }
    let bottom = mapper.y_to_px(0.0);
    let left = mapper.x_to_px(0.0);
    let right = mapper.x_to_px(1.0);
    let fill = format!("{stroke} L{right:.1} {bottom:.1} L{left:.1} {bottom:.1} Z");
    (stroke, fill)
}

/// Drag update: clamp a moved point into range and keep it between its
/// x-neighbors (matching how MSEG editors constrain reorder). First and
/// last points are pinned to x = 0 / x = 1.
pub fn constrained_move(points: &[Point], index: usize, x: f64, y: f64) -> (f64, f64) {
    let n = points.len();
    let eps = 1.0e-4;
    let x = if index == 0 {
        0.0
    } else if index + 1 == n {
        1.0
    } else {
        let lo = points[index - 1].x + eps;
        let hi = points[index + 1].x - eps;
        x.clamp(lo.min(hi), hi.max(lo))
    };
    (x, y.clamp(0.0, 1.0))
}

/// Cycle a point's curve type through the 9 variants (right-click /
/// modifier-click gesture in the editor).
pub fn next_curve_type(current: CurveType) -> CurveType {
    CurveType::from_u8((current as u8 + 1) % 9)
}

/// Scroll-wheel tension adjust: accumulate wheel delta into −1..1.
pub fn adjust_tension(current: f64, wheel_delta: f64) -> f64 {
    (current + wheel_delta * 0.05).clamp(-1.0, 1.0)
}

/// Build a `Pattern` evaluator from any point slice (the component
/// keeps its working copy as plain points).
pub fn build_pattern(points: &[Point], tension_mult: f64) -> Pattern {
    let mut pat = Pattern::new();
    pat.set_points(points.to_vec());
    pat.tension_mult = tension_mult;
    pat
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn mapper_round_trips() {
        let m = PatternMapper::new(400.0, 200.0);
        for &(x, y) in &[(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)] {
            assert!((m.px_to_x(m.x_to_px(x)) - x).abs() < 1e-9);
            assert!((m.px_to_y(m.y_to_px(y)) - y).abs() < 1e-9);
        }
        // y = 1 is at the TOP.
        assert!(m.y_to_px(1.0) < m.y_to_px(0.0));
    }

    #[test]
    fn hit_testing_picks_the_closest_handle() {
        let m = PatternMapper::new(400.0, 200.0);
        let pts = vec![pt(0.0, 1.0), pt(0.5, 0.0), pt(1.0, 1.0)];
        let px = m.x_to_px(0.5);
        let py = m.y_to_px(0.0);
        assert_eq!(nearest_point(&pts, &m, px + 3.0, py - 3.0, 10.0), Some(1));
        assert_eq!(nearest_point(&pts, &m, m.width / 2.0, 10.0, 10.0), None);
    }

    #[test]
    fn moves_stay_ordered_and_pinned() {
        let pts = vec![pt(0.0, 1.0), pt(0.4, 0.2), pt(0.6, 0.8), pt(1.0, 1.0)];
        // Endpoints pin to 0 / 1.
        assert_eq!(constrained_move(&pts, 0, 0.3, 0.5).0, 0.0);
        assert_eq!(constrained_move(&pts, 3, 0.7, 0.5).0, 1.0);
        // Interior point can't cross its neighbors.
        let (x, _) = constrained_move(&pts, 1, 0.9, 0.5);
        assert!(x < 0.6, "must stay left of the next point: {x}");
        // y clamps.
        assert_eq!(constrained_move(&pts, 1, 0.4, 1.7).1, 1.0);
    }

    #[test]
    fn paths_cover_the_full_width() {
        let pts = vec![pt(0.0, 1.0), pt(1.0, 0.0)];
        let m = PatternMapper::new(400.0, 200.0);
        let (stroke, fill) = pattern_paths(&build_pattern(&pts, 0.0), &m, 64);
        assert!(stroke.starts_with(&format!("M{:.1}", m.x_to_px(0.0))));
        assert!(fill.ends_with('Z'));
    }

    #[test]
    fn curve_cycle_wraps() {
        let mut c = CurveType::from_u8(0);
        for _ in 0..9 {
            c = next_curve_type(c);
        }
        assert_eq!(c as u8, 0);
    }
}
