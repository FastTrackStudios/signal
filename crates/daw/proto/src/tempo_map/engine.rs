//! Shared tempo/time-signature map evaluation.
//!
//! This is the canonical math used by offline RPP analysis and
//! standalone DAW tests. Tempo is integrated in quarter-note units
//! because REAPER BPM is quarter-notes per minute; displayed beats are
//! derived from the active time-signature denominator.

use crate::{TempoPoint, TimeSignature};

const EPSILON: f64 = 1e-9;

/// REAPER tempo envelope interpolation shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoShape {
    /// Keep the previous tempo until the next point.
    Hold,
    /// Linearly interpolate BPM from this point to the next point.
    Linear,
    /// Approximate REAPER's bezier tempo transition using point tension.
    Bezier,
}

impl TempoShape {
    /// Map REAPER `PT` shape integers onto the shared shape enum.
    pub fn from_reaper_shape(shape: Option<i32>) -> Self {
        match shape {
            Some(0) => Self::Linear,
            Some(5) => Self::Bezier,
            Some(1) | None => Self::Hold,
            _ => Self::Linear,
        }
    }
}

/// Evaluation engine for a sorted tempo/time-signature point map.
#[derive(Debug, Clone)]
pub struct TempoMapEngine {
    default_bpm: f64,
    default_time_signature: TimeSignature,
    points: Vec<TempoPoint>,
}

impl TempoMapEngine {
    pub fn new(
        default_bpm: f64,
        default_time_signature: TimeSignature,
        points: Vec<TempoPoint>,
    ) -> Self {
        let mut points = points;
        points.sort_by(|a, b| {
            a.position_seconds()
                .partial_cmp(&b.position_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self {
            default_bpm: default_bpm.max(f64::EPSILON),
            default_time_signature,
            points,
        }
    }

    pub fn points(&self) -> &[TempoPoint] {
        &self.points
    }

    pub fn tempo_at(&self, seconds: f64) -> f64 {
        self.points
            .iter()
            .rfind(|point| point.position_seconds() <= seconds)
            .map(|point| point.bpm)
            .unwrap_or(self.default_bpm)
    }

    pub fn time_signature_at(&self, seconds: f64) -> TimeSignature {
        self.points
            .iter()
            .filter(|point| point.position_seconds() <= seconds)
            .filter_map(|point| point.time_signature)
            .next_back()
            .unwrap_or(self.default_time_signature)
    }

    /// Integrate BPM over time and return absolute quarter notes.
    pub fn seconds_to_quarter_notes(&self, seconds: f64) -> f64 {
        if seconds <= 0.0 {
            return 0.0;
        }
        if self.points.is_empty() {
            return seconds * self.default_bpm / 60.0;
        }

        let mut total_qn = 0.0f64;
        let first = &self.points[0];

        if seconds <= first.position_seconds() {
            return seconds * self.default_bpm / 60.0;
        }
        total_qn += first.position_seconds().max(0.0) * self.default_bpm / 60.0;

        for idx in 0..self.points.len() {
            let start = &self.points[idx];
            let next = self.points.get(idx + 1);
            let seg_from = start.position_seconds().max(0.0);
            let seg_to = next
                .map(TempoPoint::position_seconds)
                .unwrap_or(seconds)
                .min(seconds);
            if seg_to <= seg_from {
                continue;
            }

            total_qn += if let Some(end) = next {
                integrate_between_points(start, end, seg_from, seg_to)
            } else {
                integrate_hold_tempo_segment(start.bpm, seg_from, seg_to)
            };

            if seconds <= seg_to + EPSILON {
                break;
            }
        }

        total_qn
    }

    /// Inverse of [`Self::seconds_to_quarter_notes`].
    pub fn quarter_notes_to_seconds(&self, target_qn: f64) -> f64 {
        if target_qn <= 0.0 {
            return 0.0;
        }
        if self.points.is_empty() {
            return target_qn / (self.default_bpm / 60.0);
        }

        let mut elapsed_qn = 0.0f64;
        let first = &self.points[0];
        let first_seconds = first.position_seconds().max(0.0);
        let pre_first_qn = first_seconds * self.default_bpm / 60.0;
        if target_qn <= pre_first_qn + EPSILON {
            return target_qn / (self.default_bpm / 60.0);
        }
        elapsed_qn += pre_first_qn;

        for idx in 0..self.points.len() {
            let start = &self.points[idx];
            let next = self.points.get(idx + 1);
            let seg_from = start.position_seconds().max(0.0);

            if let Some(end) = next {
                let seg_to = end.position_seconds();
                if seg_to <= seg_from {
                    continue;
                }
                let seg_qn = integrate_between_points(start, end, seg_from, seg_to);
                if target_qn <= elapsed_qn + seg_qn + EPSILON {
                    return solve_segment_seconds(
                        start,
                        end,
                        seg_from,
                        seg_to,
                        target_qn - elapsed_qn,
                    );
                }
                elapsed_qn += seg_qn;
            } else {
                return seg_from
                    + ((target_qn - elapsed_qn).max(0.0) / (start.bpm.max(f64::EPSILON) / 60.0));
            }
        }

        let last = self.points.last().expect("points is not empty");
        last.position_seconds()
            + ((target_qn - elapsed_qn).max(0.0) / (last.bpm.max(f64::EPSILON) / 60.0))
    }

    pub fn time_to_musical(&self, seconds: f64) -> (i32, i32, f64) {
        self.quarter_notes_to_musical(self.seconds_to_quarter_notes(seconds))
    }

    pub fn musical_to_time(&self, measure: i32, beat: i32, fraction: f64) -> f64 {
        self.quarter_notes_to_seconds(self.musical_to_quarter_notes(measure, beat, fraction))
    }

    pub fn quarter_notes_to_musical(&self, target_qn: f64) -> (i32, i32, f64) {
        if target_qn <= 0.0 {
            return (1, 1, 0.0);
        }

        let mut cursor = MusicalCursor::new(signature_metric(self.default_time_signature));
        let mut prev_qn = 0.0;

        for (change_qn, metric) in self.time_signature_changes_qn() {
            if change_qn <= prev_qn + EPSILON {
                cursor.apply_time_signature(metric);
                continue;
            }

            let stop_qn = target_qn.min(change_qn);
            cursor.advance(stop_qn - prev_qn);
            if target_qn <= change_qn + EPSILON {
                return cursor.as_parts();
            }

            prev_qn = change_qn;
            cursor.apply_time_signature(metric);
        }

        cursor.advance(target_qn - prev_qn);
        cursor.as_parts()
    }

    pub fn musical_to_quarter_notes(&self, measure: i32, beat: i32, fraction: f64) -> f64 {
        let target_measure = (measure - 1).max(0);
        let target_display_beat_offset = (beat - 1).max(0) as f64 + fraction.max(0.0);
        let mut cursor = MusicalCursor::new(signature_metric(self.default_time_signature));
        let mut prev_qn = 0.0;

        for (change_qn, metric) in self.time_signature_changes_qn() {
            let target_quarter_offset =
                target_display_beat_offset * cursor.quarter_notes_per_display_beat;
            if target_measure < cursor.measure
                || (target_measure == cursor.measure
                    && target_quarter_offset <= cursor.quarter_offset)
            {
                return prev_qn;
            }

            let segment_qn = (change_qn - prev_qn).max(0.0);
            let target_from_cursor = ((target_measure - cursor.measure) as f64
                * cursor.quarter_notes_per_measure)
                + target_quarter_offset
                - cursor.quarter_offset;
            if target_from_cursor <= segment_qn + EPSILON {
                return prev_qn + target_from_cursor.max(0.0);
            }

            cursor.advance(segment_qn);
            prev_qn = change_qn;
            cursor.apply_time_signature(metric);
        }

        let target_quarter_offset =
            target_display_beat_offset * cursor.quarter_notes_per_display_beat;
        let target_from_cursor = ((target_measure - cursor.measure) as f64
            * cursor.quarter_notes_per_measure)
            + target_quarter_offset
            - cursor.quarter_offset;
        prev_qn + target_from_cursor.max(0.0)
    }

    fn time_signature_changes_qn(&self) -> Vec<(f64, SignatureMetric)> {
        let mut changes: Vec<(f64, SignatureMetric)> = Vec::new();
        for point in &self.points {
            if let Some(ts) = point.time_signature {
                let qn = self.seconds_to_quarter_notes(point.position_seconds());
                let metric = signature_metric(ts);
                if let Some((last_qn, last_metric)) = changes.last_mut()
                    && (qn - *last_qn).abs() <= EPSILON
                {
                    *last_metric = metric;
                    continue;
                }
                changes.push((qn, metric));
            }
        }
        changes
    }
}

fn integrate_hold_tempo_segment(bpm: f64, from: f64, to: f64) -> f64 {
    if to <= from {
        return 0.0;
    }
    (to - from) * bpm.max(f64::EPSILON) / 60.0
}

fn integrate_linear_tempo_segment(start: &TempoPoint, end: &TempoPoint, from: f64, to: f64) -> f64 {
    let start_seconds = start.position_seconds();
    let seg_duration = end.position_seconds() - start_seconds;
    if seg_duration <= EPSILON {
        return integrate_hold_tempo_segment(start.bpm, from, to);
    }

    let off_from = (from - start_seconds).clamp(0.0, seg_duration);
    let off_to = (to - start_seconds).clamp(0.0, seg_duration);
    if off_to <= off_from {
        return 0.0;
    }

    let slope = (end.bpm - start.bpm) / seg_duration;
    let tempo_integral =
        start.bpm * (off_to - off_from) + 0.5 * slope * (off_to * off_to - off_from * off_from);
    tempo_integral / 60.0
}

fn bezier_shape_antiderivative(u: f64, tension: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    let t = tension.clamp(-0.999_999, 0.999_999);
    let raw = (1.0 + t.abs()) / (1.0 - t.abs());
    let a = raw.powf(0.85).clamp(1.0, 12.0);

    if t >= 0.0 {
        u.powf(a + 1.0) / (a + 1.0)
    } else {
        u + (1.0 - u).powf(a + 1.0) / (a + 1.0)
    }
}

fn integrate_bezier_tempo_segment(start: &TempoPoint, end: &TempoPoint, from: f64, to: f64) -> f64 {
    let start_seconds = start.position_seconds();
    let seg_duration = end.position_seconds() - start_seconds;
    if seg_duration <= EPSILON {
        return integrate_hold_tempo_segment(start.bpm, from, to);
    }

    let off_from = (from - start_seconds).clamp(0.0, seg_duration);
    let off_to = (to - start_seconds).clamp(0.0, seg_duration);
    if off_to <= off_from {
        return 0.0;
    }

    let u_from = off_from / seg_duration;
    let u_to = off_to / seg_duration;
    let bias_integral = bezier_shape_antiderivative(u_to, start.bezier_tension.unwrap_or(0.0))
        - bezier_shape_antiderivative(u_from, start.bezier_tension.unwrap_or(0.0));
    let du = u_to - u_from;
    let tempo_integral = seg_duration * (start.bpm * du + (end.bpm - start.bpm) * bias_integral);
    tempo_integral / 60.0
}

fn integrate_between_points(start: &TempoPoint, end: &TempoPoint, from: f64, to: f64) -> f64 {
    if to <= from {
        return 0.0;
    }
    match TempoShape::from_reaper_shape(start.shape) {
        TempoShape::Hold => integrate_hold_tempo_segment(start.bpm, from, to),
        TempoShape::Linear => integrate_linear_tempo_segment(start, end, from, to),
        TempoShape::Bezier => integrate_bezier_tempo_segment(start, end, from, to),
    }
}

fn solve_segment_seconds(
    start: &TempoPoint,
    end: &TempoPoint,
    seg_from: f64,
    seg_to: f64,
    target_qn: f64,
) -> f64 {
    if target_qn <= 0.0 {
        return seg_from;
    }
    match TempoShape::from_reaper_shape(start.shape) {
        TempoShape::Hold => seg_from + (target_qn / (start.bpm.max(f64::EPSILON) / 60.0)),
        TempoShape::Linear | TempoShape::Bezier => {
            let mut lo = seg_from;
            let mut hi = seg_to;
            for _ in 0..64 {
                let mid = (lo + hi) * 0.5;
                let qn = integrate_between_points(start, end, seg_from, mid);
                if qn < target_qn {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) * 0.5
        }
    }
}

#[derive(Clone, Copy)]
struct MusicalCursor {
    measure: i32,
    quarter_offset: f64,
    quarter_notes_per_measure: f64,
    quarter_notes_per_display_beat: f64,
}

impl MusicalCursor {
    fn new(metric: SignatureMetric) -> Self {
        Self {
            measure: 0,
            quarter_offset: 0.0,
            quarter_notes_per_measure: metric.quarter_notes_per_measure,
            quarter_notes_per_display_beat: metric.quarter_notes_per_display_beat,
        }
    }

    fn advance(&mut self, quarter_notes: f64) {
        if quarter_notes <= 0.0 {
            return;
        }
        let total = self.quarter_offset + quarter_notes;
        let measures = (total / self.quarter_notes_per_measure).floor();
        self.measure += measures as i32;
        self.quarter_offset = total - (measures * self.quarter_notes_per_measure);
        if (self.quarter_notes_per_measure - self.quarter_offset).abs() < EPSILON {
            self.measure += 1;
            self.quarter_offset = 0.0;
        }
    }

    fn apply_time_signature(&mut self, metric: SignatureMetric) {
        if self.quarter_offset > EPSILON {
            self.measure += 1;
            self.quarter_offset = 0.0;
        }
        self.quarter_notes_per_measure = metric.quarter_notes_per_measure;
        self.quarter_notes_per_display_beat = metric.quarter_notes_per_display_beat;
    }

    fn as_parts(self) -> (i32, i32, f64) {
        let display_beat_offset = self.quarter_offset / self.quarter_notes_per_display_beat;
        let beat_floor = display_beat_offset.floor();
        (
            self.measure + 1,
            beat_floor as i32 + 1,
            display_beat_offset - beat_floor,
        )
    }
}

#[derive(Clone, Copy)]
struct SignatureMetric {
    quarter_notes_per_measure: f64,
    quarter_notes_per_display_beat: f64,
}

fn signature_metric(time_signature: TimeSignature) -> SignatureMetric {
    let numerator = time_signature.numerator.max(1) as f64;
    let denominator = time_signature.denominator.max(1) as f64;
    let quarter_notes_per_display_beat = 4.0 / denominator;
    SignatureMetric {
        quarter_notes_per_measure: numerator * quarter_notes_per_display_beat,
        quarter_notes_per_display_beat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, PositionInSeconds};

    fn point(seconds: f64, bpm: f64, shape: i32) -> TempoPoint {
        TempoPoint {
            position: Position::from_time(PositionInSeconds::from_seconds(seconds)),
            bpm,
            shape: Some(shape),
            ..TempoPoint::default()
        }
    }

    #[test]
    fn integrates_linear_tempo_shape() {
        let engine = TempoMapEngine::new(
            60.0,
            TimeSignature::new(4, 4),
            vec![point(0.0, 60.0, 0), point(10.0, 120.0, 1)],
        );
        assert!((engine.seconds_to_quarter_notes(10.0) - 15.0).abs() < 1e-9);
        assert!((engine.quarter_notes_to_seconds(15.0) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn integrates_hold_tempo_shape() {
        let engine = TempoMapEngine::new(
            60.0,
            TimeSignature::new(4, 4),
            vec![point(0.0, 60.0, 1), point(10.0, 120.0, 1)],
        );
        assert!((engine.seconds_to_quarter_notes(10.0) - 10.0).abs() < 1e-9);
    }
}
