//! Razor edits — rectangular time × row areas.
//!
//! REAPER's razor edit is the answer to "I want *this rectangle* of the
//! part", independent of what is selected and independent of note
//! boundaries. Behaviour names are decoded in-tree at
//! `features/reaper/reaper-input/.../behaviors/razor_edit.rs` (17 drag,
//! 9 click, plus edge behaviours), and this models the same operations.
//!
//! The important property, and the one that makes razors different from
//! a marquee: an area **slices** notes at its edges rather than
//! selecting whole ones. Dragging a razor over the middle of a held
//! note takes the middle of that note with it. That is what makes it
//! usable for comping and for rhythmic rearrangement, and it is why the
//! area — not the note — is the unit of operation.

use crate::doc::{Dimension, ExpressionDoc, Note, NoteId};

/// Which axis a razor drag is locked to.
///
/// MRE's `H` and `L`. One enum rather than two booleans because they are
/// mutually exclusive there and two booleans would have a fourth state
/// that means nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RazorAxis {
    /// Time only — the rows the area covers cannot change.
    Horizontal,
    /// Rows only — the span of time cannot change.
    Vertical,
}

/// A rectangular selection over time and rows.
///
/// Rows are inclusive on both ends: a razor over one row has
/// `row_lo == row_hi`, not a zero-height rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RazorArea {
    pub t0: f64,
    pub t1: f64,
    pub row_lo: i32,
    pub row_hi: i32,
}

impl RazorArea {
    pub fn new(t0: f64, t1: f64, row_a: i32, row_b: i32) -> Self {
        Self {
            t0: t0.min(t1),
            t1: t0.max(t1),
            row_lo: row_a.min(row_b),
            row_hi: row_a.max(row_b),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.t1 - self.t0 <= 0.0
    }

    pub fn width(&self) -> f64 {
        self.t1 - self.t0
    }

    pub fn rows(&self) -> i32 {
        self.row_hi - self.row_lo + 1
    }

    pub fn contains(&self, t: f64, row: i32) -> bool {
        t >= self.t0 && t <= self.t1 && row >= self.row_lo && row <= self.row_hi
    }

    /// Does this area overlap any part of the note?
    pub fn touches(&self, note: &Note) -> bool {
        note.row >= self.row_lo
            && note.row <= self.row_hi
            && note.start < self.t1
            && note.end > self.t0
    }

    pub fn translated(&self, dt: f64, drows: i32) -> Self {
        Self {
            t0: self.t0 + dt,
            t1: self.t1 + dt,
            row_lo: self.row_lo + drows,
            row_hi: self.row_hi + drows,
        }
    }
}

/// Split every note crossing `t` on rows the area covers, so the area's
/// edge becomes a real note boundary.
///
/// Returns the ids of the newly created right-hand halves.
fn slice_at(doc: &mut ExpressionDoc, t: f64, row_lo: i32, row_hi: i32) -> Vec<NoteId> {
    let crossing: Vec<NoteId> = doc
        .notes
        .iter()
        .filter(|n| n.row >= row_lo && n.row <= row_hi && n.start < t && n.end > t)
        .map(|n| n.id)
        .collect();
    let mut created = Vec::new();
    for id in crossing {
        let before: Vec<NoteId> = doc.notes.iter().map(|n| n.id).collect();
        let split = crate::edit::Edit::SplitNote { note: id, t };
        if split.apply(doc)
            && let Some(new_id) = doc
                .notes
                .iter()
                .map(|n| n.id)
                .find(|id| !before.contains(id))
        {
            created.push(new_id);
        }
    }
    created
}

/// Slice both edges of `area`, then return the notes that lie inside it.
///
/// Slicing first is what makes every razor operation exact: after this,
/// "the notes in the area" is a well-defined set with no partial
/// overlaps left to reason about.
pub fn carve(doc: &mut ExpressionDoc, area: RazorArea) -> Vec<NoteId> {
    slice_at(doc, area.t0, area.row_lo, area.row_hi);
    slice_at(doc, area.t1, area.row_lo, area.row_hi);
    doc.notes
        .iter()
        .filter(|n| {
            n.row >= area.row_lo
                && n.row <= area.row_hi
                // After slicing, an interior note is fully contained.
                && n.start >= area.t0 - 1e-6
                && n.end <= area.t1 + 1e-6
                && n.end > n.start
        })
        .map(|n| n.id)
        .collect()
}

/// Notes inside the area *without* slicing — for a read-only query such
/// as "what would this take?".
pub fn peek(doc: &ExpressionDoc, area: RazorArea) -> Vec<NoteId> {
    doc.notes
        .iter()
        .filter(|n| area.touches(n))
        .map(|n| n.id)
        .collect()
}

/// Clear an area: slice its edges and delete what is inside.
pub fn delete_contents(doc: &mut ExpressionDoc, area: RazorArea) -> bool {
    let ids = carve(doc, area);
    if ids.is_empty() {
        return false;
    }
    crate::edit::Edit::DeleteNotes(ids).apply(doc)
}

/// Move an area's contents by `(dt, drows)`.
///
/// `copy` leaves the originals behind. Either way the destination is
/// cleared first, so dropping an area onto occupied ground replaces
/// rather than piling up — the behaviour that makes comping work.
///
/// `insert` turns that off: both survive, and the razor becomes a way of
/// layering rather than of choosing. MRE's `I`.
pub fn move_contents(
    doc: &mut ExpressionDoc,
    area: RazorArea,
    dt: f64,
    drows: i32,
    copy: bool,
    insert: bool,
) -> bool {
    let ids = carve(doc, area);
    if ids.is_empty() {
        return false;
    }
    let dest = area.translated(dt, drows);
    if copy {
        // Clear the destination before duplicating, or the copy lands
        // on top of whatever was already there.
        if !insert {
            delete_contents(doc, dest);
        }
        crate::edit::Edit::CopyNotes {
            notes: ids,
            time_delta: dt,
            row_delta: drows,
        }
        .apply(doc)
    } else {
        // Non-overlapping destination: clear it, then move. Overlapping
        // (a nudge) must not clear the source out from under itself.
        if !insert
            && (dest.t0 >= area.t1
                || dest.t1 <= area.t0
                || dest.row_lo > area.row_hi
                || dest.row_hi < area.row_lo)
        {
            delete_contents(doc, dest);
        }
        let mut ok = crate::edit::Edit::MoveTime {
            notes: ids.clone(),
            delta: dt,
        }
        .apply(doc);
        if drows != 0 {
            ok |= crate::edit::Edit::Transpose {
                notes: ids,
                semitones: drows,
            }
            .apply(doc);
        }
        ok
    }
}

/// Stretch an area's contents into a new time span, carrying them.
///
/// Both note positions and lengths scale, so a bar of sixteenths
/// razored and stretched becomes a bar of eighths — the rhythmic
/// rearrangement gesture.
pub fn stretch_contents(
    doc: &mut ExpressionDoc,
    area: RazorArea,
    new_t0: f64,
    new_t1: f64,
) -> bool {
    let ids = carve(doc, area);
    if ids.is_empty() || area.width() <= 0.0 || new_t1 - new_t0 <= 0.0 {
        return false;
    }
    let factor = (new_t1 - new_t0) / area.width();
    let mut ok = false;
    for id in ids {
        let Some(n) = doc.note(id).cloned() else {
            continue;
        };
        let start = new_t0 + (n.start - area.t0) * factor;
        let end = new_t0 + (n.end - area.t0) * factor;
        ok |= crate::edit::Edit::MoveTime {
            notes: vec![id],
            delta: start - n.start,
        }
        .apply(doc);
        ok |= crate::edit::Edit::Resize {
            note: id,
            start,
            end,
        }
        .apply(doc);
    }
    ok
}

/// Reverse an area's contents in time, mirroring about its centre.
pub fn reverse_contents(doc: &mut ExpressionDoc, area: RazorArea) -> bool {
    let ids = carve(doc, area);
    if ids.is_empty() {
        return false;
    }
    let mut ok = false;
    for id in ids {
        let Some(n) = doc.note(id).cloned() else {
            continue;
        };
        // Mirror the note's whole span, not just its onset, or the
        // lengths end up hanging off the wrong side.
        let new_start = area.t0 + (area.t1 - n.end);
        ok |= crate::edit::Edit::MoveTime {
            notes: vec![id],
            delta: new_start - n.start,
        }
        .apply(doc);
    }
    ok
}

/// Reverse the *pitches* in the area, leaving the rhythm alone.
///
/// MRE calls this "retrograde value only". The onsets stay exactly where
/// they are and the pitch sequence is read backwards — so a rising line
/// becomes a falling one over the identical groove, which is the version
/// you want when the rhythm is the part that already works.
pub fn reverse_pitches(doc: &mut ExpressionDoc, area: RazorArea) -> bool {
    let mut ids = carve(doc, area);
    if ids.len() < 2 {
        return false;
    }
    // By onset, because "backwards" is a statement about time even
    // though nothing here moves in time.
    ids.sort_by(|a, b| {
        let (x, y) = (doc.note(*a).map(|n| n.start), doc.note(*b).map(|n| n.start));
        x.partial_cmp(&y).unwrap_or(core::cmp::Ordering::Equal)
    });
    let rows: Vec<i32> = ids
        .iter()
        .filter_map(|id| doc.note(*id).map(|n| n.row))
        .collect();
    if rows.len() != ids.len() {
        return false;
    }
    let mut ok = false;
    for (id, row) in ids.iter().zip(rows.iter().rev()) {
        let Some(n) = doc.note(*id) else { continue };
        let delta = row - n.row;
        if delta != 0 {
            ok |= crate::edit::Edit::Transpose {
                notes: vec![*id],
                semitones: delta,
            }
            .apply(doc);
        }
    }
    ok
}

/// Mirror the pitches in the area about their own centre.
///
/// MRE's "invert". The axis is the middle of what is actually in the
/// area rather than the area's rows, so inverting twice returns the
/// original and a rectangle drawn loosely around a phrase inverts the
/// phrase rather than flinging it to the far side of the rectangle.
pub fn invert_pitches(doc: &mut ExpressionDoc, area: RazorArea) -> bool {
    let ids = carve(doc, area);
    if ids.is_empty() {
        return false;
    }
    let rows: Vec<i32> = ids
        .iter()
        .filter_map(|id| doc.note(*id).map(|n| n.row))
        .collect();
    let (Some(&lo), Some(&hi)) = (rows.iter().min(), rows.iter().max()) else {
        return false;
    };
    // Doubled so a half-step centre stays exact in integers: mirroring
    // `row` gives `sum - row`, and `sum` is `lo + hi`.
    let sum = lo + hi;
    let mut ok = false;
    for id in ids {
        let Some(n) = doc.note(id) else { continue };
        let delta = sum - n.row - n.row;
        if delta != 0 {
            ok |= crate::edit::Edit::Transpose {
                notes: vec![id],
                semitones: delta,
            }
            .apply(doc);
        }
    }
    ok
}

/// Set every note in the area to one velocity.
pub fn set_velocity(doc: &mut ExpressionDoc, area: RazorArea, velocity: f64) -> bool {
    let ids = carve(doc, area);
    crate::edit::Edit::SetVelocity {
        notes: ids,
        velocity,
    }
    .apply(doc)
}

/// Erase one expression dimension across the area, leaving the notes.
///
/// The edge-splicing rule this needs is [`crate::doc::Curve::clear_range`], which is
/// also what the dimension and controller erasers use — the razor was where
/// it was first got right, not where it belongs.
pub fn clear_lane(doc: &mut ExpressionDoc, area: RazorArea, dimension: Dimension) -> bool {
    let ids = peek(doc, area);
    let default = dimension.default_value();
    let mut ok = false;
    for id in ids {
        let Some(n) = doc.note_mut(id) else { continue };
        let (t0, t1) = (area.t0.max(n.start), area.t1.min(n.end));
        if t1 <= t0 {
            continue;
        }
        // `ok` tracks that a note was *in* the area, not that its curve
        // had anything to clear — an already-empty dimension inside a razor
        // is still a successful clear.
        n.curve_mut(dimension).clear_range(t0, t1, default);
        ok = true;
    }
    ok
}

/// The razor areas currently active, plus which one the pointer grabbed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RazorSet {
    pub areas: Vec<RazorArea>,
}

impl RazorSet {
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    pub fn clear(&mut self) {
        self.areas.clear();
    }

    /// Add an area, merging it into any it overlaps on the same rows.
    ///
    /// Merging keeps the set canonical: two adjacent razors over the
    /// same rows behave as one, so an operation cannot slice a note
    /// twice at an interior seam.
    pub fn add(&mut self, area: RazorArea) {
        if area.is_empty() {
            return;
        }
        let mut merged = area;
        self.areas.retain(|a| {
            let same_rows = a.row_lo == merged.row_lo && a.row_hi == merged.row_hi;
            let overlaps = a.t0 <= merged.t1 && merged.t0 <= a.t1;
            if same_rows && overlaps {
                merged.t0 = merged.t0.min(a.t0);
                merged.t1 = merged.t1.max(a.t1);
                false
            } else {
                true
            }
        });
        self.areas.push(merged);
    }

    /// The area under a point, if any. Later areas win, matching draw
    /// order.
    pub fn at(&self, t: f64, row: i32) -> Option<(usize, RazorArea)> {
        self.areas
            .iter()
            .enumerate()
            .rev()
            .find(|(_, a)| a.contains(t, row))
            .map(|(i, a)| (i, *a))
    }

    pub fn remove_at(&mut self, t: f64, row: i32) -> bool {
        match self.at(t, row) {
            Some((i, _)) => {
                self.areas.remove(i);
                true
            }
            None => false,
        }
    }

    /// Total time span covered, for a readout.
    pub fn total_span(&self) -> f64 {
        self.areas.iter().map(|a| a.width()).sum()
    }
}
