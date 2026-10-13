//! Pitch drawing — freehand contour with an explicit apply.
//!
//! Vovious's Pitch Drawing Mode, and the one part of its design that has
//! no equivalent anywhere else in this editor:
//!
//! - a click adds an **anchor**; existing anchors drag;
//! - the line between anchors is **sinusoidal**, not linear and not a
//!   spline — "the natural way a human sings", and the reason a drawn
//!   note does not sound like a synthesiser;
//! - the original pitch stays visible underneath for the whole session;
//! - the draft has **its own undo**, and `Return` collapses the entire
//!   drawing into *one* step of the document's history.
//!
//! That last property is why this is a layer rather than a tool. Our pen
//! commits per stroke, so twenty corrections cost twenty undos and the
//! state before you started drawing is unrecoverable in practice. Here
//! the document is touched exactly twice: a live preview that is not
//! recorded, and one recorded edit on apply.
//!
//! Anchors may sit in unvoiced regions. Forbidding that would break
//! dragging *through* a consonant, and an anchor there still shapes the
//! voiced line either side of it.

use crate::doc::{Curve, ExpressionDoc, NoteId, Point};
use crate::edit::Edit;

/// Samples written per anchor span. Dense enough that the sinusoid
/// reads as a curve rather than a chord of straight segments.
pub const SAMPLES_PER_SPAN: usize = 24;

/// How close, in document units per pixel, counts as grabbing an anchor.
pub const GRAB_PX: f64 = 7.0;

/// A pitch drawing in progress.
#[derive(Clone, Debug, PartialEq)]
pub struct PitchDraft {
    pub note: NoteId,
    /// Anchors, sorted by time. Values are semitones from the note's
    /// row, the same units the pitch curve uses.
    anchors: Vec<Point>,
    /// The note's pitch curve when the draft opened. Restored on
    /// dismiss, and drawn underneath throughout.
    captured: Vec<Point>,
    /// Anchor states, for undo *within* the draft.
    past: Vec<Vec<Point>>,
    future: Vec<Vec<Point>>,
    /// The widest span any preview has written to.
    ///
    /// Not the same as the current span, and the difference matters:
    /// undoing an anchor *narrows* the drawing, so a restore covering
    /// only the new span leaves whatever an earlier, wider preview drew
    /// beyond its edge. Everything that writes uses this instead.
    dirty: Option<(f64, f64)>,
}

impl PitchDraft {
    /// Open a draft on a note, capturing its current curve.
    pub fn open(doc: &ExpressionDoc, note: NoteId) -> Option<Self> {
        let n = doc.note(note)?;
        Some(Self {
            note,
            anchors: Vec::new(),
            captured: n.pitch.points().to_vec(),
            past: Vec::new(),
            future: Vec::new(),
            dirty: None,
        })
    }

    pub fn anchors(&self) -> &[Point] {
        &self.anchors
    }

    /// The curve as it was before drawing started — the thin line drawn
    /// underneath.
    pub fn original(&self) -> &[Point] {
        &self.captured
    }

    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    fn checkpoint(&mut self) {
        self.past.push(self.anchors.clone());
        // A new action makes the redo branch unreachable, the same rule
        // the document's own history follows.
        self.future.clear();
    }

    /// Add an anchor, replacing any at the same time.
    pub fn add(&mut self, t: f64, value: f64) {
        self.checkpoint();
        match self
            .anchors
            .binary_search_by(|a| a.t.partial_cmp(&t).unwrap_or(core::cmp::Ordering::Equal))
        {
            Ok(i) => self.anchors[i].value = value,
            Err(i) => self.anchors.insert(
                i,
                Point {
                    t,
                    value,
                    ..Point::default()
                },
            ),
        }
    }

    /// The anchor within `tolerance` of `t`, if any.
    pub fn anchor_at(&self, t: f64, tolerance: f64) -> Option<usize> {
        self.anchors
            .iter()
            .enumerate()
            .filter(|(_, a)| (a.t - t).abs() <= tolerance)
            .min_by(|(_, a), (_, b)| {
                (a.t - t)
                    .abs()
                    .partial_cmp(&(b.t - t).abs())
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// Begin dragging an existing anchor. Call once per gesture, so the
    /// whole drag is one step of the draft's undo rather than one per
    /// pointer move.
    pub fn begin_move(&mut self) {
        self.checkpoint();
    }

    /// Move an anchor already grabbed. Re-sorts, so an anchor dragged
    /// past its neighbour behaves rather than inverting the curve.
    pub fn move_to(&mut self, index: usize, t: f64, value: f64) -> bool {
        let Some(a) = self.anchors.get_mut(index) else {
            return false;
        };
        a.t = t;
        a.value = value;
        self.anchors
            .sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap_or(core::cmp::Ordering::Equal));
        true
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.anchors.len() {
            return false;
        }
        self.checkpoint();
        self.anchors.remove(index);
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        match self.past.pop() {
            Some(prev) => {
                self.future
                    .push(core::mem::replace(&mut self.anchors, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.future.pop() {
            Some(next) => {
                self.past.push(core::mem::replace(&mut self.anchors, next));
                true
            }
            None => false,
        }
    }

    /// The drawn curve, as points to write into the note.
    ///
    /// Outside the first and last anchor the captured curve shows
    /// through: a drawing that covers half a note should leave the other
    /// half as it was sung, not flatten it to the nearest anchor.
    pub fn rendered(&self, t0: f64, t1: f64) -> Vec<Point> {
        if self.anchors.is_empty() {
            return Vec::new();
        }
        if self.anchors.len() == 1 {
            let a = self.anchors[0];
            return vec![Point {
                t: a.t,
                value: a.value,
                ..Point::default()
            }];
        }
        let mut out = Vec::with_capacity(self.anchors.len() * SAMPLES_PER_SPAN);
        for pair in self.anchors.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let span = b.t - a.t;
            if span <= 0.0 {
                continue;
            }
            for i in 0..SAMPLES_PER_SPAN {
                let f = i as f64 / SAMPLES_PER_SPAN as f64;
                let t = a.t + span * f;
                if t < t0 || t > t1 {
                    continue;
                }
                out.push(Point {
                    t,
                    value: a.value + (b.value - a.value) * sine_ease(f),
                    ..Point::default()
                });
            }
        }
        // The loop stops one sample short of each anchor, so the final
        // one is added explicitly or the drawing never reaches it.
        if let Some(last) = self.anchors.last().filter(|l| l.t >= t0 && l.t <= t1) {
            out.push(*last);
        }
        out
    }

    /// The span the drawing covers.
    pub fn span(&self) -> Option<(f64, f64)> {
        match (self.anchors.first(), self.anchors.last()) {
            (Some(a), Some(b)) if b.t > a.t => Some((a.t, b.t)),
            _ => None,
        }
    }

    /// The span every write covers: the union of the current drawing
    /// and everything a previous preview touched.
    pub fn dirty_span(&self) -> Option<(f64, f64)> {
        match (self.span(), self.dirty) {
            (Some((a0, a1)), Some((b0, b1))) => Some((a0.min(b0), a1.max(b1))),
            (Some(s), None) | (None, Some(s)) => Some(s),
            (None, None) => None,
        }
    }

    /// The points to write across the dirty span.
    ///
    /// Drawn where the anchors reach, and the captured curve everywhere
    /// else — so narrowing a drawing puts back what was sung rather than
    /// leaving the previous, wider version stranded there.
    fn combined(&self) -> Vec<Point> {
        let Some((d0, d1)) = self.dirty_span() else {
            return Vec::new();
        };
        let (s0, s1) = self.span().unwrap_or((f64::MAX, f64::MIN));
        let mut out: Vec<Point> = self
            .captured
            .iter()
            .copied()
            .filter(|p| p.t >= d0 && p.t <= d1 && (p.t < s0 || p.t > s1))
            .collect();
        out.extend(self.rendered(s0.max(d0), s1.min(d1)));
        out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(core::cmp::Ordering::Equal));
        out
    }

    /// Write the drawing into the document *without* recording it.
    ///
    /// Restores the captured curve first, so every preview rebuilds
    /// from the same input rather than layering on the last one.
    pub fn preview_edits(&mut self) -> Vec<Edit> {
        let Some((t0, t1)) = self.dirty_span() else {
            return Vec::new();
        };
        self.dirty = Some((t0, t1));
        vec![
            Edit::RestoreDimension {
                note: self.note,
                dimension: crate::doc::Dimension::Pitch,
                t0,
                t1,
                points: self.captured.clone(),
            },
            Edit::DrawDimension {
                note: self.note,
                dimension: crate::doc::Dimension::Pitch,
                t0,
                t1,
                points: self.combined(),
            },
        ]
    }

    /// The single edit that commits the drawing.
    ///
    /// One `DrawDimension`, so the whole session — however many anchors were
    /// placed, moved and undone along the way — is one step of the
    /// document's history.
    pub fn apply_edit(&self) -> Option<Edit> {
        let (t0, t1) = self.dirty_span()?;
        Some(Edit::DrawDimension {
            note: self.note,
            dimension: crate::doc::Dimension::Pitch,
            t0,
            t1,
            points: self.combined(),
        })
    }

    /// The edit that puts everything back, for dismiss.
    pub fn cancel_edit(&self) -> Option<Edit> {
        let (t0, t1) = self.dirty_span()?;
        Some(Edit::RestoreDimension {
            note: self.note,
            dimension: crate::doc::Dimension::Pitch,
            t0,
            t1,
            points: self.captured.clone(),
        })
    }
}

/// Raised-cosine easing: flat at both ends, steepest in the middle.
///
/// This is the shape, and it is the whole reason drawn pitch sounds
/// sung. A linear ramp between anchors arrives at its target at full
/// speed and stops dead, which reads as a synthesiser glide; a spline
/// overshoots and puts pitch where no anchor asked for it. A voice
/// accelerates out of one pitch and decelerates into the next.
pub fn sine_ease(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    0.5 - 0.5 * (core::f64::consts::PI * x).cos()
}

/// Sample the drawing as a curve, for hit-testing and rendering.
pub fn as_curve(points: &[Point]) -> Curve {
    Curve::from_points(points.to_vec())
}
