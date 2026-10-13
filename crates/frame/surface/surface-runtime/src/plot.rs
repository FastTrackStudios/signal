//! Plot geometry: points in unit space → SVG path data for a node's box.
//!
//! Points are `(x, y)` with `x` 0..1 left → right and `y` 0..1 bottom →
//! top; a NaN `y` is a gap (the pen lifts). Paths are built in the slot
//! node's own coordinates, decimated to its pixel width first so a 4096-bin
//! spectrum costs no more to draw than the columns it lands on.

use std::fmt::Write as _;
use std::sync::Arc;

use surface_proto::PlotStyle;

/// What a plot or curve shows this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    /// Unit-space points, sorted by `x`.
    pub points: Arc<Vec<(f64, f64)>>,
    pub style: PlotStyle,
    /// Where `Area` slots close to (unit `y`).
    pub baseline: f64,
    /// Where `Marker` slots sit (unit space), if anywhere.
    pub marker: Option<(f64, f64)>,
}

impl Trace {
    /// Whether there is anything to draw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.points.iter().any(|p| p.1.is_finite())
    }
}

/// At most two points (the column's extremes, in order) per pixel column of
/// `columns`: peaks survive, the rest is redundant. Gaps are kept.
#[must_use]
pub fn decimate(points: &[(f64, f64)], columns: usize) -> Vec<(f64, f64)> {
    let columns = columns.max(1);
    if points.len() <= columns * 2 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(columns * 2 + 2);
    let col = |x: f64| ((x.clamp(0.0, 1.0) * columns as f64) as usize).min(columns - 1);
    let mut i = 0;
    while i < points.len() {
        let c = col(points[i].0);
        let mut lo: Option<(usize, (f64, f64))> = None;
        let mut hi: Option<(usize, (f64, f64))> = None;
        let mut gap = false;
        let start = i;
        while i < points.len() && col(points[i].0) == c {
            let p = points[i];
            if p.1.is_finite() {
                if lo.is_none_or(|(_, q)| p.1 < q.1) {
                    lo = Some((i, p));
                }
                if hi.is_none_or(|(_, q)| p.1 > q.1) {
                    hi = Some((i, p));
                }
            } else {
                gap = true;
            }
            i += 1;
        }
        match (lo, hi) {
            (Some((a, pa)), Some((b, pb))) if a != b => {
                if a < b {
                    out.extend([pa, pb]);
                } else {
                    out.extend([pb, pa]);
                }
            }
            (Some((_, p)), _) => out.push(p),
            _ => {}
        }
        if gap {
            out.push((points[start].0, f64::NAN));
        }
    }
    out
}

fn px((x, y): (f64, f64), w: f64, h: f64) -> (f64, f64) {
    (x * w, (1.0 - y.clamp(0.0, 1.0)) * h)
}

/// Runs of consecutive finite points.
fn runs(points: &[(f64, f64)]) -> impl Iterator<Item = &[(f64, f64)]> {
    points.split(|p| !p.1.is_finite()).filter(|r| !r.is_empty())
}

/// The line through `points` (a staircase for `Steps`, circles for `Dots`,
/// bars for `Bars`) in a `w × h` box.
#[must_use]
pub fn line_path(points: &[(f64, f64)], w: f64, h: f64, style: PlotStyle, dot: f64) -> String {
    let cols = match style {
        PlotStyle::Bars => (w / 3.0).max(1.0) as usize,
        PlotStyle::Dots => (w / (dot * 2.5).max(1.0)).max(1.0) as usize,
        _ => w.max(1.0).ceil() as usize,
    };
    let pts = decimate(points, cols);
    let mut d = String::new();
    match style {
        PlotStyle::Dots => {
            for p in pts.iter().filter(|p| p.1.is_finite()) {
                let (x, y) = px(*p, w, h);
                let r = dot;
                let _ = write!(
                    d,
                    "M {:.2} {y:.2} a {r:.2} {r:.2} 0 1 0 {:.2} 0 a {r:.2} {r:.2} 0 1 0 {:.2} 0 Z ",
                    x - r,
                    2.0 * r,
                    -2.0 * r
                );
            }
        }
        PlotStyle::Bars => bars(&mut d, &pts, w, h, 0.0),
        PlotStyle::Steps => {
            for run in runs(&pts) {
                for (i, p) in run.iter().enumerate() {
                    let (x, y) = px(*p, w, h);
                    if i == 0 {
                        let _ = write!(d, "M {x:.2} {y:.2} ");
                    } else {
                        let _ = write!(d, "H {x:.2} V {y:.2} ");
                    }
                }
            }
        }
        PlotStyle::Line | PlotStyle::Area { .. } => {
            for run in runs(&pts) {
                for (i, p) in run.iter().enumerate() {
                    let (x, y) = px(*p, w, h);
                    let _ = write!(d, "{} {x:.2} {y:.2} ", if i == 0 { 'M' } else { 'L' });
                }
            }
        }
    }
    d.truncate(d.trim_end().len());
    d
}

/// The region between the line and `baseline` (unit `y`), closed.
#[must_use]
pub fn area_path(points: &[(f64, f64)], w: f64, h: f64, style: PlotStyle, baseline: f64) -> String {
    let cols = match style {
        PlotStyle::Bars => (w / 3.0).max(1.0) as usize,
        _ => w.max(1.0).ceil() as usize,
    };
    let pts = decimate(points, cols);
    let mut d = String::new();
    if matches!(style, PlotStyle::Bars) {
        bars(&mut d, &pts, w, h, baseline);
    } else {
        let base = (1.0 - baseline.clamp(0.0, 1.0)) * h;
        for run in runs(&pts) {
            let (x0, _) = px(run[0], w, h);
            let _ = write!(d, "M {x0:.2} {base:.2} ");
            let mut last_y = None;
            for p in run {
                let (x, y) = px(*p, w, h);
                if matches!(style, PlotStyle::Steps)
                    && let Some(ly) = last_y
                {
                    let _ = write!(d, "L {x:.2} {ly:.2} ");
                }
                let _ = write!(d, "L {x:.2} {y:.2} ");
                last_y = Some(y);
            }
            let (xn, _) = px(run[run.len() - 1], w, h);
            let _ = write!(d, "L {xn:.2} {base:.2} Z ");
        }
    }
    d.truncate(d.trim_end().len());
    d
}

/// One bar per point, from `baseline` to the point, 70% of its column.
fn bars(d: &mut String, pts: &[(f64, f64)], w: f64, h: f64, baseline: f64) {
    let n = pts.iter().filter(|p| p.1.is_finite()).count().max(1);
    let bw = (w / n as f64 * 0.7).max(1.0);
    let base = (1.0 - baseline.clamp(0.0, 1.0)) * h;
    for p in pts.iter().filter(|p| p.1.is_finite()) {
        let (x, y) = px(*p, w, h);
        let x0 = (x - bw / 2.0).clamp(0.0, (w - bw).max(0.0));
        let (top, bottom) = if y < base { (y, base) } else { (base, y) };
        if bottom - top > 0.01 {
            let _ = write!(
                d,
                "M {x0:.2} {top:.2} H {:.2} V {bottom:.2} H {x0:.2} Z ",
                x0 + bw
            );
        }
    }
}

/// `y` on a polyline sorted by `x`, interpolated at `x` (clamped to its
/// ends; `None` when it has no finite points).
#[must_use]
pub fn sample(points: &[(f64, f64)], x: f64) -> Option<f64> {
    let finite: Vec<&(f64, f64)> = points
        .iter()
        .filter(|p| p.0.is_finite() && p.1.is_finite())
        .collect();
    let (first, last) = (finite.first()?, finite.last()?);
    if x <= first.0 {
        return Some(first.1);
    }
    if x >= last.0 {
        return Some(last.1);
    }
    let i = finite.partition_point(|p| p.0 < x);
    let (a, b) = (finite[i - 1], finite[i]);
    let span = b.0 - a.0;
    Some(if span <= 0.0 {
        b.1
    } else {
        a.1 + (b.1 - a.1) * (x - a.0) / span
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_maps_unit_space_to_the_box_with_y_up() {
        let d = line_path(
            &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.5)],
            100.0,
            50.0,
            PlotStyle::Line,
            2.0,
        );
        assert_eq!(d, "M 0.00 50.00 L 50.00 0.00 L 100.00 25.00");
    }

    #[test]
    fn gaps_lift_the_pen_and_empty_draws_nothing() {
        let d = line_path(
            &[(0.0, 0.2), (0.3, f64::NAN), (0.6, 0.4), (1.0, 0.4)],
            10.0,
            10.0,
            PlotStyle::Line,
            1.0,
        );
        assert_eq!(d.matches('M').count(), 2, "{d}");
        assert!(line_path(&[], 10.0, 10.0, PlotStyle::Line, 1.0).is_empty());
        assert!(area_path(&[(0.0, f64::NAN)], 10.0, 10.0, PlotStyle::Line, 0.0).is_empty());
    }

    #[test]
    fn areas_close_to_their_baseline() {
        let d = area_path(
            &[(0.0, 0.5), (1.0, 0.5)],
            10.0,
            10.0,
            PlotStyle::Area { baseline: 0.0 },
            1.0,
        );
        assert_eq!(d, "M 0.00 0.00 L 0.00 5.00 L 10.00 5.00 L 10.00 0.00 Z");
    }

    #[test]
    fn decimation_keeps_peaks_and_bounds_the_work() {
        let pts: Vec<(f64, f64)> = (0..4096)
            .map(|i| (f64::from(i) / 4095.0, if i == 2000 { 1.0 } else { 0.1 }))
            .collect();
        let d = decimate(&pts, 100);
        assert!(d.len() <= 200, "{}", d.len());
        assert!(d.iter().any(|p| p.1 == 1.0), "the peak survives");
        assert_eq!(
            decimate(&pts[..50], 100).len(),
            50,
            "short input is untouched"
        );
    }

    #[test]
    fn steps_bars_and_dots() {
        let pts = [(0.0, 0.0), (0.5, 1.0), (1.0, 0.5)];
        assert_eq!(
            line_path(&pts, 10.0, 10.0, PlotStyle::Steps, 1.0),
            "M 0.00 10.00 H 5.00 V 0.00 H 10.00 V 5.00"
        );
        let bars = line_path(&pts, 30.0, 10.0, PlotStyle::Bars, 1.0);
        assert_eq!(
            bars.matches('Z').count(),
            2,
            "a zero bar draws nothing: {bars}"
        );
        assert_eq!(
            line_path(&pts, 30.0, 10.0, PlotStyle::Dots, 1.0)
                .matches('Z')
                .count(),
            3
        );
    }

    #[test]
    fn sampling_interpolates_and_clamps() {
        let pts = [(0.0, 0.0), (1.0, 10.0)];
        assert_eq!(sample(&pts, 0.25), Some(2.5));
        assert_eq!(sample(&pts, -1.0), Some(0.0));
        assert_eq!(sample(&pts, 3.0), Some(10.0));
        assert_eq!(sample(&[], 0.5), None);
    }
}
