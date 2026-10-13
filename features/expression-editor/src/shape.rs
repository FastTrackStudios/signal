//! Curve shapes and freehand simplification.
//!
//! Shapes are unit maps `x: 0..1 -> y: 0..1` applied between a
//! gesture's own endpoints, so restyling a curve never moves its
//! boundary values.

/// The shape family offered on the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Exponential,
    SCurve,
}

impl Shape {
    pub const ALL: [Shape; 6] = [
        Shape::Linear,
        Shape::EaseIn,
        Shape::EaseOut,
        Shape::EaseInOut,
        Shape::Exponential,
        Shape::SCurve,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Shape::Linear => "Linear",
            Shape::EaseIn => "Ease In",
            Shape::EaseOut => "Ease Out",
            Shape::EaseInOut => "Ease In/Out",
            Shape::Exponential => "Exponential",
            Shape::SCurve => "S-Curve",
        }
    }

    /// The unit map. Every shape satisfies `amount(0) == 0` and
    /// `amount(1) == 1`.
    pub fn amount(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Shape::Linear => x,
            Shape::EaseIn => x * x,
            Shape::EaseOut => 1.0 - (1.0 - x) * (1.0 - x),
            Shape::EaseInOut => {
                if x < 0.5 {
                    2.0 * x * x
                } else {
                    1.0 - 2.0 * (1.0 - x) * (1.0 - x)
                }
            }
            // Normalized so it still lands on 1 at x = 1.
            Shape::Exponential => ((8.0 * x).exp_m1()) / (8.0f64.exp_m1()),
            Shape::SCurve => smoothstep(x),
        }
    }
}

/// Hermite smoothstep — the workhorse for every magnet blend.
pub fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `smoothstep` remapped so it is 0 below `edge0` and 1 above `edge1`.
pub fn smoothstep_between(edge0: f64, edge1: f64, x: f64) -> f64 {
    if (edge1 - edge0).abs() < 1e-12 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    smoothstep((x - edge0) / (edge1 - edge0))
}

/// Ramer–Douglas–Peucker over `(t, value)` pairs.
///
/// Freehand strokes commit at screen resolution — one point per pixel —
/// which is right for feel but wrong for storage. This trims the
/// redundant collinear runs without rounding off the gesture.
pub fn simplify(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 || tolerance <= 0.0 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    simplify_range(points, 0, points.len() - 1, tolerance, &mut keep);
    points
        .iter()
        .zip(&keep)
        .filter_map(|(&p, &k)| k.then_some(p))
        .collect()
}

fn simplify_range(points: &[(f64, f64)], first: usize, last: usize, tol: f64, keep: &mut [bool]) {
    if last <= first + 1 {
        return;
    }
    let a = points[first];
    let b = points[last];
    let mut worst = 0.0;
    let mut worst_i = first;
    for (i, &p) in points.iter().enumerate().take(last).skip(first + 1) {
        let d = perpendicular_distance(p, a, b);
        if d > worst {
            worst = d;
            worst_i = i;
        }
    }
    if worst > tol {
        keep[worst_i] = true;
        simplify_range(points, first, worst_i, tol, keep);
        simplify_range(points, worst_i, last, tol, keep);
    }
}

fn perpendicular_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    // Distance from the infinite line, not the segment: the endpoints
    // are always kept, so the segment case never arises.
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len2.sqrt()
}
