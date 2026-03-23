//! Multi-point curve — piecewise-linear mapping from macro position to parameter value.
//!
//! Each curve is a sorted list of `CurvePoint`s. Interpolation between points
//! is linear. Values outside the curve range clamp to the nearest endpoint.
//!
//! A curve with two points at macro_value 0.0 and 1.0 is equivalent to the
//! classic min/max binding.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// A single point in a multi-point macro curve.
///
/// Maps a macro knob position to a specific parameter value.
/// Both values are normalized 0.0–1.0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct CurvePoint {
    /// Macro knob position (0.0–1.0).
    pub macro_value: f64,
    /// Target parameter value at this position (0.0–1.0).
    pub param_value: f64,
}

impl CurvePoint {
    pub fn new(macro_value: f64, param_value: f64) -> Self {
        Self {
            macro_value: macro_value.clamp(0.0, 1.0),
            param_value: param_value.clamp(0.0, 1.0),
        }
    }
}

/// A piecewise-linear curve defined by sorted control points.
///
/// Points are always kept sorted by `macro_value`. Interpolation between
/// adjacent points is linear. Values below the first point clamp to
/// that point's param_value; values above the last point clamp similarly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct MultiPointCurve {
    /// Control points, sorted by `macro_value` ascending.
    pub points: Vec<CurvePoint>,
}

impl Default for MultiPointCurve {
    fn default() -> Self {
        Self {
            points: vec![
                CurvePoint::new(0.0, 0.0),
                CurvePoint::new(1.0, 1.0),
            ],
        }
    }
}

impl MultiPointCurve {
    /// Create a curve from min/max values (equivalent to the classic binding).
    pub fn min_max(min: f64, max: f64) -> Self {
        Self {
            points: vec![
                CurvePoint::new(0.0, min),
                CurvePoint::new(1.0, max),
            ],
        }
    }

    /// Create a curve from a list of points. Points are sorted on creation.
    pub fn from_points(mut points: Vec<CurvePoint>) -> Self {
        points.sort_by(|a, b| a.macro_value.partial_cmp(&b.macro_value).unwrap());
        Self { points }
    }

    /// Add a point to the curve, maintaining sort order.
    /// If a point at the same macro_value already exists, it is replaced.
    pub fn set_point(&mut self, point: CurvePoint) {
        // Remove existing point at same position (within epsilon)
        self.points
            .retain(|p| (p.macro_value - point.macro_value).abs() > 1e-6);
        // Insert in sorted position
        let idx = self
            .points
            .partition_point(|p| p.macro_value < point.macro_value);
        self.points.insert(idx, point);
    }

    /// Remove the point nearest to the given macro_value.
    /// Won't remove if fewer than 2 points remain.
    pub fn remove_nearest(&mut self, macro_value: f64) -> Option<CurvePoint> {
        if self.points.len() <= 2 {
            return None;
        }
        let idx = self
            .points
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = (a.macro_value - macro_value).abs();
                let db = (b.macro_value - macro_value).abs();
                da.partial_cmp(&db).unwrap()
            })
            .map(|(i, _)| i)?;
        Some(self.points.remove(idx))
    }

    /// Evaluate the curve at a given macro knob position using piecewise-linear interpolation.
    pub fn evaluate(&self, macro_value: f64) -> f64 {
        let macro_value = macro_value.clamp(0.0, 1.0);

        if self.points.is_empty() {
            return macro_value; // identity fallback
        }
        if self.points.len() == 1 {
            return self.points[0].param_value;
        }

        // Clamp to endpoints
        let first = &self.points[0];
        if macro_value <= first.macro_value {
            return first.param_value;
        }
        let last = &self.points[self.points.len() - 1];
        if macro_value >= last.macro_value {
            return last.param_value;
        }

        // Find the two surrounding points and interpolate
        for window in self.points.windows(2) {
            let a = &window[0];
            let b = &window[1];
            if macro_value >= a.macro_value && macro_value <= b.macro_value {
                let range = b.macro_value - a.macro_value;
                if range < 1e-10 {
                    return a.param_value;
                }
                let t = (macro_value - a.macro_value) / range;
                return a.param_value + (b.param_value - a.param_value) * t;
            }
        }

        // Shouldn't reach here, but fallback
        last.param_value
    }

    /// Number of control points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether the curve has no points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_max_equivalent() {
        let curve = MultiPointCurve::min_max(0.2, 0.8);
        assert!((curve.evaluate(0.0) - 0.2).abs() < 1e-6);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 1e-6);
        assert!((curve.evaluate(1.0) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn three_point_curve() {
        // 0% → 10, 60% → 4, 100% → 11 (user's example from conversation)
        // Using normalized: 0→0.1, 0.6→0.04, 1.0→0.11
        let curve = MultiPointCurve::from_points(vec![
            CurvePoint::new(0.0, 0.1),
            CurvePoint::new(0.6, 0.04),
            CurvePoint::new(1.0, 0.11),
        ]);

        assert!((curve.evaluate(0.0) - 0.1).abs() < 1e-6);
        assert!((curve.evaluate(0.6) - 0.04).abs() < 1e-6);
        assert!((curve.evaluate(1.0) - 0.11).abs() < 1e-6);

        // Midpoint between 0.0 and 0.6: at 0.3, should be lerp(0.1, 0.04, 0.5) = 0.07
        assert!((curve.evaluate(0.3) - 0.07).abs() < 1e-6);
    }

    #[test]
    fn set_point_replaces_existing() {
        let mut curve = MultiPointCurve::min_max(0.0, 1.0);
        assert_eq!(curve.len(), 2);

        curve.set_point(CurvePoint::new(0.5, 0.3));
        assert_eq!(curve.len(), 3);

        // Replace the midpoint
        curve.set_point(CurvePoint::new(0.5, 0.7));
        assert_eq!(curve.len(), 3);
        assert!((curve.evaluate(0.5) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn remove_nearest() {
        let mut curve = MultiPointCurve::from_points(vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.5, 0.3),
            CurvePoint::new(1.0, 1.0),
        ]);

        let removed = curve.remove_nearest(0.48);
        assert!(removed.is_some());
        assert!((removed.unwrap().macro_value - 0.5).abs() < 1e-6);
        assert_eq!(curve.len(), 2);
    }

    #[test]
    fn remove_nearest_wont_go_below_two() {
        let mut curve = MultiPointCurve::min_max(0.0, 1.0);
        assert!(curve.remove_nearest(0.0).is_none());
        assert_eq!(curve.len(), 2);
    }

    #[test]
    fn clamps_outside_range() {
        let curve = MultiPointCurve::min_max(0.2, 0.8);
        assert!((curve.evaluate(-1.0) - 0.2).abs() < 1e-6);
        assert!((curve.evaluate(2.0) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn serde_round_trip() {
        let curve = MultiPointCurve::from_points(vec![
            CurvePoint::new(0.0, 0.1),
            CurvePoint::new(0.5, 0.5),
            CurvePoint::new(1.0, 0.9),
        ]);
        let json = serde_json::to_string(&curve).unwrap();
        let parsed: MultiPointCurve = serde_json::from_str(&json).unwrap();
        assert_eq!(curve, parsed);
    }
}
