//! Timing — stretching the gaps between notes.
//!
//! The manual's Timestretching Mode. Boundaries between adjacent notes
//! draw as full-height vertical lines with a small horizontal tick at
//! mid-height, and **where on the line you grab decides what the drag
//! does**:
//!
//! | grab | effect |
//! |---|---|
//! | **above** the tick | the note(s) *left* of the line stretch; the ones right of it only **move** |
//! | **below** the tick | the notes on **both** sides stretch |
//!
//! That is a genuinely good idea and worth stating plainly: the two
//! operations differ only in whether the material after the boundary
//! keeps its own length, and making that a position on one line rather
//! than two separate tools means the choice is made in the same motion
//! as the drag.
//!
//! Double-clicking a line snaps it to the beat.
//!
//! ## Why this edits notes, not samples
//!
//! Stretching is expressed as `Resize` + `MoveTime` on the document.
//! The audio domain's warp markers are *derived* from the difference
//! between where a note now sits and where it was analyzed — see
//! `expression-editor-audio`. Keeping it that way means the same
//! gesture works on a MIDI take, where there is nothing to warp and
//! moving the notes is the whole edit.

use crate::doc::{ExpressionDoc, NoteId};
use crate::edit::Edit;

/// Hard limits on a single stretch, from the manual: material can be
/// expanded four times or reduced to an eighth, and beyond that the
/// gesture refuses rather than degrading.
pub const MAX_FACTOR: f64 = 4.0;
/// Reduce limit — an eighth of the original length.
pub const MIN_FACTOR: f64 = 0.125;

/// A boundary between two adjacent notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Separator {
    /// Where the boundary sits now.
    pub t: f64,
    /// The note ending here, if any.
    pub left: Option<NoteId>,
    /// The note starting here, if any.
    pub right: Option<NoteId>,
}

/// What a drag on a separator does, chosen by where it was grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StretchLaw {
    /// Grabbed above the tick: the left side stretches, the right side
    /// keeps its length and slides.
    LeftStretchRightMoves,
    /// Grabbed below the tick: both sides stretch, so nothing after the
    /// affected pair moves at all.
    BothStretch,
}

impl StretchLaw {
    /// Pick the law from a grab at `y` on a line spanning `0..h`.
    pub fn at(y: f64, h: f64) -> Self {
        if y < h * 0.5 {
            StretchLaw::LeftStretchRightMoves
        } else {
            StretchLaw::BothStretch
        }
    }
}

/// Boundaries between notes that touch or abut.
///
/// Only real adjacencies: a gap of silence is not a boundary you can
/// drag, because there is nothing on one side of it whose length would
/// change. Notes are considered adjacent when the next one starts at or
/// before the previous one ends, within `tolerance`.
pub fn separators(doc: &ExpressionDoc, tolerance: f64) -> Vec<Separator> {
    let mut notes: Vec<_> = doc.notes.iter().collect();
    notes.sort_by(|a, b| a.start.total_cmp(&b.start));

    let mut out = Vec::new();
    for pair in notes.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (b.start - a.end).abs() <= tolerance {
            out.push(Separator {
                t: (a.end + b.start) * 0.5,
                left: Some(a.id),
                right: Some(b.id),
            });
        }
    }
    out
}

/// How far a boundary sits from the nearest grid division.
///
/// The manual colours the lines by this, which is the fastest way to
/// see that a phrase is dragging without reading the grid.
pub fn beat_deviation(t: f64, origin: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return 0.0;
    }
    let rel = t - origin;
    let nearest = (rel / step).round() * step;
    rel - nearest
}

/// The edits that move a boundary to `to`.
///
/// Returns an empty vector when the move would take either side past
/// the stretch limits, so an over-drag simply stops rather than
/// producing something the renderer cannot honour.
pub fn plan(doc: &ExpressionDoc, sep: Separator, to: f64, law: StretchLaw) -> Vec<Edit> {
    let Some(left_id) = sep.left else {
        return Vec::new();
    };
    let Some(left) = doc.note(left_id) else {
        return Vec::new();
    };
    let delta = to - sep.t;
    if delta == 0.0 {
        return Vec::new();
    }

    // The left note always stretches: it is the one the boundary is the
    // end of, in both laws.
    let left_len = left.end - left.start;
    if left_len <= 0.0 {
        return Vec::new();
    }
    let left_factor = (left_len + delta) / left_len;
    if !within_limits(left_factor) {
        return Vec::new();
    }

    let mut edits = vec![Edit::Resize {
        note: left_id,
        start: left.start,
        end: left.end + delta,
    }];

    let Some(right_id) = sep.right else {
        return edits;
    };
    let Some(right) = doc.note(right_id) else {
        return edits;
    };

    match law {
        StretchLaw::LeftStretchRightMoves => {
            // Everything from the boundary on keeps its length and
            // slides, so the phrase after it is undisturbed except in
            // time.
            let following: Vec<NoteId> = doc
                .notes
                .iter()
                .filter(|n| n.start >= right.start - f64::EPSILON && n.id != left_id)
                .map(|n| n.id)
                .collect();
            edits.push(Edit::MoveTime {
                notes: following,
                delta,
            });
        }
        StretchLaw::BothStretch => {
            // The right note absorbs the change, so its end — and
            // therefore everything after it — does not move.
            let right_len = right.end - right.start;
            if right_len <= 0.0 {
                return edits;
            }
            let right_factor = (right_len - delta) / right_len;
            if !within_limits(right_factor) {
                return Vec::new();
            }
            edits.push(Edit::Resize {
                note: right_id,
                start: right.start + delta,
                end: right.end,
            });
        }
    }
    edits
}

/// Whether a stretch factor is inside the algorithm's limits.
pub fn within_limits(factor: f64) -> bool {
    factor.is_finite() && (MIN_FACTOR..=MAX_FACTOR).contains(&factor)
}

/// Where a double-click sends a boundary: the nearest grid division.
pub fn snap_to_beat(t: f64, origin: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return t;
    }
    origin + ((t - origin) / step).round() * step
}
