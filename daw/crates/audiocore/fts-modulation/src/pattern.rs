//! MSEG pattern engine — multi-segment envelope generator.
//!
//! Stores a set of points with curve types, builds segments for efficient
//! lookup, and evaluates the envelope at any phase (0..1).
//!
//! Credits: KottV/SimpleSide (SSCurve), tiagolr (gate12/filtr/time12/reevr).

use crate::curves::{self, CurveType};

/// A single control point in the pattern.
#[derive(Debug, Clone)]
pub struct Point {
    /// Unique identifier.
    pub id: u64,
    /// X position (0..1).
    pub x: f64,
    /// Y value (0..1).
    pub y: f64,
    /// Curve tension (-1..1).
    pub tension: f64,
    /// Curve type for the segment starting at this point.
    pub curve_type: CurveType,
    /// Crossing this point hard-clears the modulated target's tail
    /// (the REEV-R reverb trick: rhythmic wash kills).
    pub clear_tails: bool,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            id: 0,
            x,
            y,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        }
    }
}

/// A precomputed segment between two adjacent points.
#[derive(Debug, Clone)]
struct Segment {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
    tension: f64,
    curve_type: CurveType,
}

/// MSEG pattern with up to 12 selectable slots.
///
/// Points are stored sorted by x. Segments are rebuilt when points change.
/// Evaluation uses binary search for O(log n) lookup.
#[derive(Debug, Clone)]
pub struct Pattern {
    points: Vec<Point>,
    segments: Vec<Segment>,
    next_id: u64,

    /// Global tension multiplier applied on top of per-point tension.
    pub tension_mult: f64,
    /// Attack tension override (used in dual-tension mode).
    pub tension_atk: f64,
    /// Release tension override (used in dual-tension mode).
    pub tension_rel: f64,
    /// Enable dual-tension mode (attack/release based on segment direction).
    pub dual_tension: bool,
}

impl Pattern {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            segments: Vec::new(),
            next_id: 1,
            tension_mult: 1.0,
            tension_atk: 0.0,
            tension_rel: 0.0,
            dual_tension: false,
        }
    }

    /// Create a default pattern with two points (full range).
    pub fn default_ramp() -> Self {
        let mut p = Self::new();
        p.add_point(Point {
            id: 0,
            x: 0.0,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        p.add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        p
    }

    /// Create a pattern with a sine-like shape (using HalfSine curves).
    pub fn default_sine() -> Self {
        let mut p = Self::new();
        p.add_point(Point {
            id: 0,
            x: 0.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        p.add_point(Point {
            id: 0,
            x: 0.5,
            y: 1.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        p.add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.0,
            tension: 0.0,
            curve_type: CurveType::HalfSine,
            clear_tails: false,
        });
        p
    }

    /// Number of points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Get a reference to the points.
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Add a point and rebuild segments. Returns the assigned ID.
    pub fn add_point(&mut self, mut point: Point) -> u64 {
        point.id = self.next_id;
        self.next_id += 1;
        self.points.push(point);
        self.points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        self.build_segments();
        self.points.last().map_or(0, |p| p.id)
    }

    /// Remove a point by ID and rebuild segments.
    pub fn remove_point(&mut self, id: u64) -> bool {
        let before = self.points.len();
        self.points.retain(|p| p.id != id);
        if self.points.len() != before {
            self.build_segments();
            true
        } else {
            false
        }
    }

    /// Update a point's position/value by ID and rebuild segments.
    pub fn update_point(&mut self, id: u64, x: f64, y: f64) {
        if let Some(p) = self.points.iter_mut().find(|p| p.id == id) {
            p.x = x.clamp(0.0, 1.0);
            p.y = y.clamp(0.0, 1.0);
        }
        self.points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        self.build_segments();
    }

    /// Set all points at once and rebuild segments.
    pub fn set_points(&mut self, points: Vec<Point>) {
        self.points = points;
        for p in &mut self.points {
            if p.id == 0 {
                p.id = self.next_id;
                self.next_id += 1;
            }
        }
        self.points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        self.build_segments();
    }

    /// Clear all points.
    pub fn clear(&mut self) {
        self.points.clear();
        self.segments.clear();
    }

    /// Evaluate the pattern at a given phase (0..1). Returns 0..1.
    pub fn get_y(&self, x: f64) -> f64 {
        if self.segments.is_empty() {
            return 0.5;
        }

        let x = x.clamp(0.0, 1.0);

        // Binary search for the segment containing x
        let idx = self.find_segment(x);
        let seg = &self.segments[idx];

        let span = seg.x2 - seg.x1;
        if span < 1e-15 {
            return seg.y1;
        }

        let t = ((x - seg.x1) / span).clamp(0.0, 1.0);

        // Apply global tension modifiers
        let tension = self.effective_tension(seg);

        curves::evaluate(seg.curve_type, t, seg.y1, seg.y2, tension)
    }

    /// Build segments from the sorted point list.
    ///
    /// Adds ghost points outside 0..1 for seamless wrapping.
    fn build_segments(&mut self) {
        self.segments.clear();

        if self.points.len() < 2 {
            if let Some(p) = self.points.first() {
                // Single point — constant value
                self.segments.push(Segment {
                    x1: 0.0,
                    x2: 1.0,
                    y1: p.y,
                    y2: p.y,
                    tension: 0.0,
                    curve_type: CurveType::Hold,
                });
            }
            return;
        }

        // Build ghost-wrapped point list for seamless looping
        let mut pts: Vec<(f64, f64, f64, CurveType)> = Vec::with_capacity(self.points.len() + 2);

        // Ghost: last point wrapped before start
        let last = self.points.last().unwrap();
        pts.push((last.x - 1.0, last.y, last.tension, last.curve_type));

        // All real points
        for p in &self.points {
            pts.push((p.x, p.y, p.tension, p.curve_type));
        }

        // Ghost: first point wrapped after end
        let first = &self.points[0];
        pts.push((first.x + 1.0, first.y, first.tension, first.curve_type));

        // Build segments from consecutive pairs, clamped to 0..1
        for w in pts.windows(2) {
            let (x1, y1, tension, curve_type) = w[0];
            let (x2, y2, _, _) = w[1];

            // Only include segments that overlap with 0..1
            if x2 <= 0.0 || x1 >= 1.0 {
                continue;
            }

            self.segments.push(Segment {
                x1: x1.max(0.0),
                x2: x2.min(1.0),
                y1,
                y2,
                tension,
                curve_type,
            });
        }
    }

    /// Binary search for the segment containing x.
    fn find_segment(&self, x: f64) -> usize {
        if self.segments.len() <= 1 {
            return 0;
        }
        match self
            .segments
            .binary_search_by(|seg| seg.x1.partial_cmp(&x).unwrap())
        {
            Ok(i) => i,
            Err(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
        }
    }

    /// Compute effective tension for a segment, applying global modifiers.
    fn effective_tension(&self, seg: &Segment) -> f64 {
        if self.dual_tension {
            // Rising = attack, falling = release
            let base = if seg.y2 > seg.y1 {
                self.tension_atk
            } else {
                self.tension_rel
            };
            (seg.tension + base).clamp(-1.0, 1.0) * self.tension_mult
        } else {
            seg.tension * self.tension_mult
        }
    }

    /// Transform all points so the average Y matches `target_y`.
    ///
    /// Used by filtr/reevr to link a knob (e.g., cutoff) to the pattern center.
    /// Whether a `clear_tails` point lies in the phase interval
    /// (prev, now] (wrap-aware — patterns cycle).
    pub fn clear_crossed(&self, prev_phase: f64, now_phase: f64) -> bool {
        let p0 = prev_phase.rem_euclid(1.0);
        let p1 = now_phase.rem_euclid(1.0);
        self.points.iter().any(|p| {
            if !p.clear_tails {
                return false;
            }
            if p0 <= p1 {
                p.x > p0 && p.x <= p1
            } else {
                p.x > p0 || p.x <= p1
            }
        })
    }

    pub fn transform(&mut self, target_y: f64) {
        if self.points.is_empty() {
            return;
        }
        let avg: f64 = self.points.iter().map(|p| p.y).sum::<f64>() / self.points.len() as f64;
        let offset = target_y - avg;
        for p in &mut self.points {
            p.y = (p.y + offset).clamp(0.0, 1.0);
        }
        self.build_segments();
    }

    /// Invert the pattern (flip Y values).
    pub fn invert(&mut self) {
        for p in &mut self.points {
            p.y = 1.0 - p.y;
        }
        self.build_segments();
    }

    /// Reverse the pattern (mirror X positions).
    pub fn reverse(&mut self) {
        for p in &mut self.points {
            p.x = 1.0 - p.x;
        }
        self.points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        self.build_segments();
    }

    /// Double the pattern — compress to first half and repeat.
    pub fn double(&mut self) {
        let orig: Vec<Point> = self.points.drain(..).collect();
        for p in &orig {
            self.points.push(Point {
                id: self.next_id,
                x: p.x * 0.5,
                y: p.y,
                tension: p.tension,
                curve_type: p.curve_type,
                clear_tails: false,
            });
            self.next_id += 1;
        }
        for p in &orig {
            self.points.push(Point {
                id: self.next_id,
                x: 0.5 + p.x * 0.5,
                y: p.y,
                tension: p.tension,
                curve_type: p.curve_type,
                clear_tails: false,
            });
            self.next_id += 1;
        }
        self.points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        self.build_segments();
    }

    /// Rotate the pattern by an offset (0..1).
    pub fn rotate(&mut self, offset: f64) {
        for p in &mut self.points {
            p.x = (p.x + offset).fract();
            if p.x < 0.0 {
                p.x += 1.0;
            }
        }
        self.points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        self.build_segments();
    }
}

impl Default for Pattern {
    fn default() -> Self {
        Self::new()
    }
}

/// A bank of 12 selectable patterns (matching tiagolr's MIDI note % 12 selection).
pub struct PatternBank {
    patterns: [Pattern; 12],
    active: usize,
}

impl PatternBank {
    pub fn new() -> Self {
        Self {
            patterns: std::array::from_fn(|_| Pattern::new()),
            active: 0,
        }
    }

    /// Get the active pattern.
    pub fn active(&self) -> &Pattern {
        &self.patterns[self.active]
    }

    /// Get the active pattern mutably.
    pub fn active_mut(&mut self) -> &mut Pattern {
        &mut self.patterns[self.active]
    }

    /// Set the active pattern index (0..11).
    pub fn set_active(&mut self, index: usize) {
        self.active = index.min(11);
    }

    /// Get a pattern by index.
    pub fn get(&self, index: usize) -> &Pattern {
        &self.patterns[index.min(11)]
    }

    /// Get a pattern by index mutably.
    pub fn get_mut(&mut self, index: usize) -> &mut Pattern {
        &mut self.patterns[index.min(11)]
    }

    /// Active pattern index.
    pub fn active_index(&self) -> usize {
        self.active
    }
}

impl Default for PatternBank {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pattern_returns_midpoint() {
        let p = Pattern::new();
        assert_eq!(p.get_y(0.5), 0.5);
    }

    #[test]
    fn ramp_pattern() {
        let p = Pattern::default_ramp();
        let y0 = p.get_y(0.0);
        let y1 = p.get_y(1.0);
        assert!((y0 - 1.0).abs() < 1e-5, "Start should be 1.0: {y0}");
        assert!((y1 - 0.0).abs() < 1e-5, "End should be 0.0: {y1}");
    }

    #[test]
    fn ramp_monotonically_decreasing() {
        let p = Pattern::default_ramp();
        let mut prev = p.get_y(0.0);
        for i in 1..=100 {
            let x = i as f64 / 100.0;
            let y = p.get_y(x);
            assert!(y <= prev + 1e-10, "Should decrease: {prev} -> {y} at x={x}");
            prev = y;
        }
    }

    #[test]
    fn sine_pattern_peaks_at_midpoint() {
        let p = Pattern::default_sine();
        let mid = p.get_y(0.5);
        assert!(mid > 0.9, "Sine should peak near 1.0 at midpoint: {mid}");
    }

    #[test]
    fn invert_flips_values() {
        let mut p = Pattern::default_ramp();
        p.invert();
        let y0 = p.get_y(0.0);
        let y1 = p.get_y(1.0);
        assert!(
            (y0 - 0.0).abs() < 1e-5,
            "Inverted start should be 0.0: {y0}"
        );
        assert!((y1 - 1.0).abs() < 1e-5, "Inverted end should be 1.0: {y1}");
    }

    #[test]
    fn transform_shifts_center() {
        // Use points that won't clamp: y=0.3, y=0.7 — avg=0.5
        let mut p = Pattern::new();
        p.add_point(Point {
            id: 0,
            x: 0.0,
            y: 0.3,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        p.add_point(Point {
            id: 0,
            x: 1.0,
            y: 0.7,
            tension: 0.0,
            curve_type: CurveType::Curve,
            clear_tails: false,
        });
        // Shift center from 0.5 to 0.6
        p.transform(0.6);
        let avg: f64 = p.points().iter().map(|pt| pt.y).sum::<f64>() / p.len() as f64;
        assert!((avg - 0.6).abs() < 1e-5, "Average should be 0.6: {avg}");
    }

    #[test]
    fn pattern_bank_selection() {
        let mut bank = PatternBank::new();
        bank.get_mut(0).add_point(Point::new(0.0, 1.0));
        bank.get_mut(0).add_point(Point::new(1.0, 0.0));
        bank.set_active(0);
        assert!(bank.active().get_y(0.0) > 0.9);
    }

    #[test]
    fn add_remove_points() {
        let mut p = Pattern::new();
        let _id1 = p.add_point(Point::new(0.0, 0.0));
        let id2 = p.add_point(Point::new(0.5, 1.0));
        let _id3 = p.add_point(Point::new(1.0, 0.0));
        assert_eq!(p.len(), 3);

        p.remove_point(id2);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn double_doubles_pattern() {
        let mut p = Pattern::default_ramp();
        let original_len = p.len();
        p.double();
        assert_eq!(p.len(), original_len * 2);
    }

    #[test]
    fn get_y_clamped_input() {
        let p = Pattern::default_ramp();
        let y_neg = p.get_y(-0.5);
        let y_over = p.get_y(1.5);
        assert!(y_neg.is_finite());
        assert!(y_over.is_finite());
    }
}
