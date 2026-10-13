//! One drawn curve, pointed at any lane.
//!
//! [`velocity::Curve`](crate::velocity::Curve) turned out to be the
//! general tool in this crate and velocity turned out to be one of its
//! uses. A Bézier over a span is equally the right answer for a CC swell,
//! a pitch-bend scoop, a gradual legato, or pushing a phrase off the
//! grid. This module is that observation made concrete: pick a [`Lane`],
//! draw once, apply.
//!
//! ## Two kinds of lane
//!
//! The split that shapes the whole module:
//!
//! - **Per-note lanes** ([`Lane::Velocity`], [`Lane::Length`],
//!   [`Lane::Timing`]) sample the curve once per note, at that note's
//!   position in the selection. There are exactly as many results as
//!   notes.
//! - **Continuous lanes** ([`Lane::Cc`], [`Lane::PitchBend`]) sample the
//!   curve at a fixed *resolution* across the time span, because a CC
//!   ramp is a stream of events that exists independently of where the
//!   notes happen to fall. Drawing a swell under one held chord has to
//!   work, and it can't if the lane only emits where notes are.
//!
//! That's why [`ShapeEdit`] is an enum rather than a `Vec<something>`:
//! the two kinds genuinely produce different things, and flattening them
//! into a common "edit" type would mean inventing a position for CC
//! events that don't have a note to belong to.

use crate::velocity::{Curve, MAX_VELOCITY, MIN_VELOCITY, Note, Range, VelocityEdit, targets};

/// What a shape is applied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Note velocity — the original use.
    Velocity,
    /// Note length, as a multiplier on each note's current length.
    /// Curve height 1.0 means "leave it", 0.0 means "as short as
    /// allowed" — see [`Shape::length_range`].
    Length,
    /// Note start, nudged earlier or later. Curve height 0.5 is "don't
    /// move"; above pushes late, below pulls early.
    Timing,
    /// A continuous controller lane.
    Cc(u8),
    /// The pitch-bend lane.
    PitchBend,
}

impl Lane {
    /// Whether this lane emits one value per note (rather than a stream
    /// across time).
    pub fn is_per_note(self) -> bool {
        matches!(self, Lane::Velocity | Lane::Length | Lane::Timing)
    }

    pub fn label(self) -> String {
        match self {
            Lane::Velocity => "Velocity".to_string(),
            Lane::Length => "Length".to_string(),
            Lane::Timing => "Timing".to_string(),
            Lane::Cc(n) => format!("CC{n}"),
            Lane::PitchBend => "Pitch Bend".to_string(),
        }
    }
}

/// A note with the timing information the continuous lanes need.
///
/// [`crate::velocity::Note`] deliberately carries only what the velocity
/// engines use; shaping length and timing needs more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedNote {
    pub index: u32,
    pub velocity: u8,
    pub selected: bool,
    pub start_ppq: f64,
    pub length_ppq: f64,
}

impl ShapedNote {
    pub fn end_ppq(&self) -> f64 {
        self.start_ppq + self.length_ppq
    }

    fn as_note(&self) -> Note {
        Note {
            index: self.index,
            velocity: self.velocity,
            selected: self.selected,
        }
    }
}

/// A note's new length, in PPQ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LengthEdit {
    pub index: u32,
    pub length_ppq: f64,
}

/// A note's new start, in PPQ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimingEdit {
    pub index: u32,
    pub start_ppq: f64,
}

/// One controller event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CcPoint {
    pub position_ppq: f64,
    pub value: u8,
}

/// One pitch-bend event. `value` is centred at 0, ±8191.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BendPoint {
    pub position_ppq: f64,
    pub value: i16,
}

/// What applying a shape produced.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeEdit {
    Velocity(Vec<VelocityEdit>),
    Length(Vec<LengthEdit>),
    Timing(Vec<TimingEdit>),
    Cc {
        controller: u8,
        points: Vec<CcPoint>,
    },
    PitchBend(Vec<BendPoint>),
}

impl ShapeEdit {
    /// How many events this would write. For status lines.
    pub fn len(&self) -> usize {
        match self {
            ShapeEdit::Velocity(v) => v.len(),
            ShapeEdit::Length(v) => v.len(),
            ShapeEdit::Timing(v) => v.len(),
            ShapeEdit::Cc { points, .. } => points.len(),
            ShapeEdit::PitchBend(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A curve aimed at a lane.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub curve: Curve,
    pub lane: Lane,
    /// Velocity clamp, for [`Lane::Velocity`].
    pub range: Range,
    /// Shortest and longest a note may become, as a multiplier on its
    /// current length. Only [`Lane::Length`] reads it.
    pub length_range: (f64, f64),
    /// How far [`Lane::Timing`] may push a note, in PPQ, at full
    /// deflection.
    pub timing_depth_ppq: f64,
    /// Events per quarter note for the continuous lanes.
    ///
    /// Eight is REAPER's own default CC density for drawn ramps: fine
    /// enough that a swell sounds smooth, coarse enough that a bar
    /// doesn't cost hundreds of events. Raising it past ~32 mostly buys
    /// project bloat.
    pub resolution_per_qn: f64,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            curve: Curve::default(),
            lane: Lane::Velocity,
            range: Range::default(),
            length_range: (0.1, 2.0),
            timing_depth_ppq: 120.0,
            resolution_per_qn: 8.0,
        }
    }
}

/// Ticks per quarter note, matching [`crate::arp::PPQ`].
const PPQ: f64 = 960.0;

impl Shape {
    pub fn new(curve: Curve, lane: Lane) -> Self {
        Self {
            curve,
            lane,
            ..Self::default()
        }
    }

    /// Apply the shape to `notes`.
    ///
    /// The time span for the continuous lanes is taken from the notes
    /// themselves — first onset to last release — so drawing a swell
    /// under a selection covers exactly that selection.
    pub fn apply(&self, notes: &[ShapedNote]) -> ShapeEdit {
        let picked: Vec<ShapedNote> = {
            let plain: Vec<Note> = notes.iter().map(|n| n.as_note()).collect();
            targets(&plain).map(|(i, _)| notes[i]).collect()
        };

        match self.lane {
            Lane::Velocity => ShapeEdit::Velocity(self.velocity(&picked)),
            Lane::Length => ShapeEdit::Length(self.length(&picked)),
            Lane::Timing => ShapeEdit::Timing(self.timing(&picked)),
            Lane::Cc(controller) => ShapeEdit::Cc {
                controller,
                points: self.cc(&picked),
            },
            Lane::PitchBend => ShapeEdit::PitchBend(self.bend(&picked)),
        }
    }

    /// Curve height at note `i` of `n`, in 0.0..=1.0.
    fn at_note(&self, i: usize, n: usize) -> f64 {
        let last = n.saturating_sub(1);
        let t = if last == 0 {
            0.0
        } else {
            i as f64 / last as f64
        };
        self.curve.evaluate(t).1
    }

    fn velocity(&self, notes: &[ShapedNote]) -> Vec<VelocityEdit> {
        notes
            .iter()
            .enumerate()
            .map(|(i, note)| VelocityEdit {
                index: note.index,
                velocity: self
                    .range
                    .clamp(self.at_note(i, notes.len()) * f64::from(MAX_VELOCITY)),
            })
            .collect()
    }

    fn length(&self, notes: &[ShapedNote]) -> Vec<LengthEdit> {
        let (lo, hi) = self.length_range;
        notes
            .iter()
            .enumerate()
            .map(|(i, note)| {
                let factor = lo + self.at_note(i, notes.len()) * (hi - lo);
                LengthEdit {
                    index: note.index,
                    // Never zero: a zero-length note is not a note, and
                    // some backends treat it as a delete.
                    length_ppq: (note.length_ppq * factor).max(1.0),
                }
            })
            .collect()
    }

    fn timing(&self, notes: &[ShapedNote]) -> Vec<TimingEdit> {
        notes
            .iter()
            .enumerate()
            .map(|(i, note)| {
                // Centred: 0.5 is no movement, so a flat curve at half
                // height is the identity and the control is bipolar
                // without needing a separate sign.
                let offset = (self.at_note(i, notes.len()) - 0.5) * 2.0 * self.timing_depth_ppq;
                TimingEdit {
                    index: note.index,
                    start_ppq: (note.start_ppq + offset).max(0.0),
                }
            })
            .collect()
    }

    /// Onset of the first note to release of the last.
    fn span(&self, notes: &[ShapedNote]) -> Option<(f64, f64)> {
        let start = notes
            .iter()
            .map(|n| n.start_ppq)
            .fold(f64::INFINITY, f64::min);
        let end = notes
            .iter()
            .map(|n| n.end_ppq())
            .fold(f64::NEG_INFINITY, f64::max);
        (end > start).then_some((start, end))
    }

    /// Sample positions across the span, inclusive of both ends.
    fn samples(&self, notes: &[ShapedNote]) -> Vec<(f64, f64)> {
        let Some((start, end)) = self.span(notes) else {
            return Vec::new();
        };
        let step = PPQ / self.resolution_per_qn.max(1.0);
        let count = (((end - start) / step).ceil() as usize).max(1);
        (0..=count)
            .map(|i| {
                let t = i as f64 / count as f64;
                (start + (end - start) * t, self.curve.evaluate(t).1)
            })
            .collect()
    }

    fn cc(&self, notes: &[ShapedNote]) -> Vec<CcPoint> {
        self.samples(notes)
            .into_iter()
            .map(|(position_ppq, y)| CcPoint {
                position_ppq,
                value: (y * f64::from(MAX_VELOCITY)).round().clamp(0.0, 127.0) as u8,
            })
            .collect()
    }

    fn bend(&self, notes: &[ShapedNote]) -> Vec<BendPoint> {
        self.samples(notes)
            .into_iter()
            .map(|(position_ppq, y)| BendPoint {
                position_ppq,
                // Centred like the timing lane: half height is no bend,
                // so a flat curve leaves the pitch alone.
                value: (((y - 0.5) * 2.0) * 8191.0).round().clamp(-8192.0, 8191.0) as i16,
            })
            .collect()
    }
}

/// Convenience: the velocity clamp bounds, re-exported so a UI building a
/// [`Shape`] doesn't have to reach into `velocity`.
pub const VELOCITY_BOUNDS: (u8, u8) = (MIN_VELOCITY, MAX_VELOCITY);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::velocity::CurvePreset;

    fn notes(n: usize) -> Vec<ShapedNote> {
        (0..n)
            .map(|i| ShapedNote {
                index: i as u32,
                velocity: 64,
                selected: false,
                start_ppq: i as f64 * (PPQ / 4.0),
                length_ppq: PPQ / 8.0,
            })
            .collect()
    }

    #[test]
    fn the_velocity_lane_matches_what_the_velocity_engine_does() {
        // The whole premise: generalizing the curve must not change what
        // it already did.
        let ns = notes(16);
        let plain: Vec<Note> = ns.iter().map(|n| n.as_note()).collect();
        let direct = CurvePreset::Rise.curve().apply(&plain, Range::default());

        let shaped = Shape::new(CurvePreset::Rise.curve(), Lane::Velocity).apply(&ns);
        assert_eq!(shaped, ShapeEdit::Velocity(direct));
    }

    #[test]
    fn the_length_lane_scales_each_note() {
        let shape = Shape {
            length_range: (0.5, 2.0),
            ..Shape::new(CurvePreset::Rise.curve(), Lane::Length)
        };
        let ShapeEdit::Length(edits) = shape.apply(&notes(9)) else {
            panic!("wrong lane");
        };
        // Rise runs 0 → 1, so the factor runs 0.5 → 2.0.
        assert!((edits[0].length_ppq - PPQ / 8.0 * 0.5).abs() < 1.0);
        assert!((edits[8].length_ppq - PPQ / 8.0 * 2.0).abs() < 1.0);
        assert!(edits.windows(2).all(|w| w[0].length_ppq <= w[1].length_ppq));
    }

    #[test]
    fn a_note_never_becomes_zero_length() {
        let shape = Shape {
            length_range: (0.0, 0.0),
            ..Shape::new(CurvePreset::Fall.curve(), Lane::Length)
        };
        let ShapeEdit::Length(edits) = shape.apply(&notes(4)) else {
            panic!("wrong lane");
        };
        assert!(edits.iter().all(|e| e.length_ppq >= 1.0));
    }

    #[test]
    fn a_flat_half_height_curve_is_the_identity_for_timing() {
        let flat = Curve::new([
            crate::velocity::Point::new(0.0, 0.5),
            crate::velocity::Point::new(1.0, 0.5),
        ]);
        let ns = notes(8);
        let ShapeEdit::Timing(edits) = Shape::new(flat, Lane::Timing).apply(&ns) else {
            panic!("wrong lane");
        };
        for (edit, note) in edits.iter().zip(&ns) {
            assert!((edit.start_ppq - note.start_ppq).abs() < 1e-6, "{edit:?}");
        }
    }

    #[test]
    fn timing_pushes_late_above_centre_and_early_below() {
        let shape = Shape {
            timing_depth_ppq: 100.0,
            ..Shape::new(CurvePreset::Rise.curve(), Lane::Timing)
        };
        // Offset a bar in: a note at PPQ 0 clamps at 0 and can't move
        // earlier, which would make "pushes early" untestable on it.
        let ns: Vec<ShapedNote> = notes(9)
            .into_iter()
            .map(|n| ShapedNote {
                start_ppq: n.start_ppq + PPQ * 4.0,
                ..n
            })
            .collect();
        let ShapeEdit::Timing(edits) = shape.apply(&ns) else {
            panic!("wrong lane");
        };
        // Rise starts at 0 (early by the full depth) and ends at 1 (late).
        assert!(edits[0].start_ppq < ns[0].start_ppq);
        assert!(edits[8].start_ppq > ns[8].start_ppq);
    }

    #[test]
    fn timing_never_pushes_a_note_before_the_take() {
        let shape = Shape {
            timing_depth_ppq: 10_000.0,
            ..Shape::new(CurvePreset::Fall.curve(), Lane::Timing)
        };
        let ShapeEdit::Timing(edits) = shape.apply(&notes(4)) else {
            panic!("wrong lane");
        };
        assert!(edits.iter().all(|e| e.start_ppq >= 0.0));
    }

    #[test]
    fn a_cc_lane_emits_across_the_span_not_per_note() {
        // One held chord, three notes, all at the same position. A
        // per-note lane would give three values; a swell needs many.
        let held: Vec<ShapedNote> = [60u8, 64, 67]
            .iter()
            .enumerate()
            .map(|(i, _)| ShapedNote {
                index: i as u32,
                velocity: 96,
                selected: false,
                start_ppq: 0.0,
                length_ppq: PPQ * 4.0,
            })
            .collect();

        let shape = Shape::new(CurvePreset::Rise.curve(), Lane::Cc(11));
        let ShapeEdit::Cc { controller, points } = shape.apply(&held) else {
            panic!("wrong lane");
        };
        assert_eq!(controller, 11);
        // Four quarter notes at 8 per QN, plus the closing sample.
        assert_eq!(points.len(), 33);
        assert_eq!(points.first().unwrap().value, 0);
        assert_eq!(points.last().unwrap().value, 127);
        assert!(points.windows(2).all(|w| w[0].value <= w[1].value));
    }

    #[test]
    fn cc_resolution_controls_the_event_count() {
        let ns = notes(4); // spans 3*(PPQ/4) + PPQ/8
        let sparse = Shape {
            resolution_per_qn: 2.0,
            ..Shape::new(CurvePreset::Rise.curve(), Lane::Cc(1))
        };
        let dense = Shape {
            resolution_per_qn: 32.0,
            ..Shape::new(CurvePreset::Rise.curve(), Lane::Cc(1))
        };
        assert!(sparse.apply(&ns).len() < dense.apply(&ns).len());
    }

    #[test]
    fn pitch_bend_is_centred_so_a_half_height_curve_does_not_bend() {
        let flat = Curve::new([
            crate::velocity::Point::new(0.0, 0.5),
            crate::velocity::Point::new(1.0, 0.5),
        ]);
        let ShapeEdit::PitchBend(points) = Shape::new(flat, Lane::PitchBend).apply(&notes(4))
        else {
            panic!("wrong lane");
        };
        assert!(points.iter().all(|p| p.value == 0), "{points:?}");
    }

    #[test]
    fn pitch_bend_reaches_the_full_range_at_the_extremes() {
        let ShapeEdit::PitchBend(points) =
            Shape::new(CurvePreset::Rise.curve(), Lane::PitchBend).apply(&notes(8))
        else {
            panic!("wrong lane");
        };
        assert_eq!(points.first().unwrap().value, -8191);
        assert_eq!(points.last().unwrap().value, 8191);
    }

    #[test]
    fn every_lane_honours_the_selection() {
        let mut ns = notes(8);
        ns[3].selected = true;
        ns[4].selected = true;

        for lane in [Lane::Velocity, Lane::Length, Lane::Timing] {
            let edit = Shape::new(CurvePreset::Rise.curve(), lane).apply(&ns);
            assert_eq!(edit.len(), 2, "{lane:?} should touch only the selection");
        }
    }

    #[test]
    fn a_continuous_lane_spans_only_the_selection() {
        let mut ns = notes(16);
        ns[0].selected = true;
        ns[1].selected = true;
        let shape = Shape::new(CurvePreset::Rise.curve(), Lane::Cc(1));
        let ShapeEdit::Cc { points, .. } = shape.apply(&ns) else {
            panic!("wrong lane");
        };
        let last = points.last().unwrap().position_ppq;
        assert!(
            last <= ns[1].end_ppq() + 1.0,
            "ran past the selection: {last}"
        );
    }

    #[test]
    fn no_notes_produces_no_edits_in_any_lane() {
        for lane in [
            Lane::Velocity,
            Lane::Length,
            Lane::Timing,
            Lane::Cc(1),
            Lane::PitchBend,
        ] {
            assert!(
                Shape::new(CurvePreset::Rise.curve(), lane)
                    .apply(&[])
                    .is_empty(),
                "{lane:?}"
            );
        }
    }

    #[test]
    fn lanes_know_whether_they_are_per_note() {
        assert!(Lane::Velocity.is_per_note());
        assert!(Lane::Length.is_per_note());
        assert!(Lane::Timing.is_per_note());
        assert!(!Lane::Cc(7).is_per_note());
        assert!(!Lane::PitchBend.is_per_note());
    }

    #[test]
    fn lane_labels_name_the_controller() {
        assert_eq!(Lane::Cc(11).label(), "CC11");
        assert_eq!(Lane::PitchBend.label(), "Pitch Bend");
    }
}
