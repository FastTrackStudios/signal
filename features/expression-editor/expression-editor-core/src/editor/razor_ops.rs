//! Editor razor behavior.
use super::*;

impl Editor {
    // ── razor operations ─────────────────────────────────────────────
    //
    // The razor's *operations* have existed in `razor::` since it was
    // written; what was missing was any way to reach them. They took a
    // single area and a document, which is the right shape for the
    // algorithm and the wrong one for a command: every caller then had
    // to loop the set, open a gesture, and remember that carving
    // invalidates ids.
    //
    // These are that missing layer. One undo step each, all areas, and
    // no argument the caller has to get right.

    /// Run `op` over every area, as one undo step.
    ///
    /// Areas are snapshotted first because the operations carve, which
    /// rewrites `doc.notes` underneath an iteration over `self.razor`.
    /// The whole set, not just one: a razor set is a *single* selection
    /// that happens to be discontiguous, and an operation that quietly
    /// applied to one rectangle of four would be the surprise.
    pub(super) fn razor_op(&mut self, op: impl Fn(&mut ExpressionDoc, RazorArea) -> bool) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        self.begin_gesture();
        let mut ok = false;
        for area in areas {
            ok |= op(&mut self.doc, area);
        }
        ok
    }

    /// Delete everything inside the areas, keeping the areas.
    ///
    /// The areas stay so a delete can be followed by another operation
    /// on the same region — clearing them as well would make the
    /// commonest pair (delete, then paste something else there) take two
    /// gestures to set up.
    pub fn razor_delete_contents(&mut self) -> bool {
        self.razor_op(razor::delete_contents)
    }

    /// Mirror each area's contents in time about its own centre.
    pub fn razor_reverse(&mut self) -> bool {
        self.razor_op(razor::reverse_contents)
    }

    /// Split notes at the area edges and leave everything in place.
    ///
    /// Every other razor operation carves as a side effect of doing
    /// something else. This is the carve on its own — the way to turn
    /// "this rectangle" into real note boundaries you can then work with
    /// by ordinary means.
    pub fn razor_split(&mut self) -> bool {
        self.razor_op(|doc, area| !razor::carve(doc, area).is_empty())
    }

    /// Scale each area's contents in time about the area's start.
    ///
    /// `2.0` turns a bar of eighths into a bar of quarters spilling past
    /// the area; `0.5` halves it. The area itself is scaled to match, so
    /// the rectangle keeps describing what it holds.
    pub fn razor_scale(&mut self, factor: f64) -> bool {
        if factor <= 0.0 {
            return false;
        }
        let ok = self.razor_op(|doc, area| {
            razor::stretch_contents(doc, area, area.t0, area.t0 + area.width() * factor)
        });
        if ok {
            for a in &mut self.razor.areas {
                a.t1 = a.t0 + a.width() * factor;
            }
        }
        ok
    }

    /// Reverse the pitches, keeping the rhythm.
    pub fn razor_reverse_pitches(&mut self) -> bool {
        self.razor_op(razor::reverse_pitches)
    }

    /// Mirror the pitches about their own centre.
    pub fn razor_invert(&mut self) -> bool {
        self.razor_op(razor::invert_pitches)
    }

    /// Copy the contents forward by the area's own width, and take the
    /// areas with them.
    ///
    /// The areas move rather than staying put, so pressing it twice
    /// gives you three copies rather than two in the same place — the
    /// gesture is "again", and again has to land somewhere new.
    pub fn razor_duplicate(&mut self) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        let insert = self.razor_insert;
        self.begin_gesture();
        let mut ok = false;
        for area in &areas {
            let width = area.width();
            ok |= razor::move_contents(&mut self.doc, *area, width, 0, true, insert);
        }
        for a in &mut self.razor.areas {
            let width = a.width();
            *a = a.translated(width, 0);
        }
        ok
    }

    /// Take the notes inside the areas out of the selection.
    pub fn razor_unselect_contents(&mut self) -> bool {
        if self.razor.is_empty() || self.selection.is_empty() {
            return false;
        }
        let inside: Vec<NoteId> = self
            .razor
            .areas
            .iter()
            .flat_map(|a| razor::peek(&self.doc, *a))
            .collect();
        let before = self.selection.notes.len();
        self.selection.notes.retain(|id| !inside.contains(id));
        before != self.selection.notes.len()
    }

    /// Grow the areas to cover every row.
    ///
    /// MRE's "full-lane area". The rectangle stops being about *which*
    /// pitches and becomes purely about a span of time, which is what
    /// you want when the edit is rhythmic and the pitch range was only
    /// ever an accident of how far you dragged.
    pub fn razor_full_lane(&mut self) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        for a in &mut self.razor.areas {
            a.row_lo = 0;
            a.row_hi = 127;
        }
        true
    }

    /// Erase the active expression lane across the areas.
    pub fn razor_clear_lane(&mut self) -> bool {
        let dimension = self.dimension;
        self.razor_op(move |doc, area| razor::clear_lane(doc, area, dimension))
    }

    /// Select the notes inside the areas, keeping the areas.
    ///
    /// The bridge between the two kinds of selection. A razor says
    /// "this rectangle" and a selection says "these notes"; carving
    /// first is what makes the second answer exact, because afterwards
    /// there are no partial overlaps left to round one way or the other.
    ///
    /// The areas stay, following MRE: selecting is usually a step
    /// towards another razor operation on the same region, not a way of
    /// finishing with it.
    pub fn razor_select_contents(&mut self) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        self.begin_gesture();
        let mut ids = Vec::new();
        for area in areas {
            ids.extend(razor::carve(&mut self.doc, area));
        }
        ids.sort_unstable();
        ids.dedup();
        let found = !ids.is_empty();
        self.selection.notes = ids;
        found
    }

    /// Move the areas by `(dt, drows)`, carrying their contents.
    pub fn razor_nudge(&mut self, dt: f64, drows: i32) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        let insert = self.razor_insert;
        self.begin_gesture();
        let mut ok = false;
        for area in &areas {
            ok |= razor::move_contents(&mut self.doc, *area, dt, drows, false, insert);
        }
        for a in &mut self.razor.areas {
            *a = a.translated(dt, drows);
        }
        ok || dt != 0.0 || drows != 0
    }

    /// Move the areas' right edge by `dt`, leaving the contents alone.
    ///
    /// Resize rather than stretch: this changes what the rectangle
    /// *covers*, which is the thing you adjust when the sweep came out a
    /// grid line short. Scaling the material is [`Editor::razor_scale`].
    pub fn razor_resize(&mut self, dt: f64) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let mut ok = false;
        for a in &mut self.razor.areas {
            // Never past its own start: an inverted area is not a
            // smaller one, it is a broken one.
            let t1 = (a.t1 + dt).max(a.t0 + f64::EPSILON);
            ok |= t1 != a.t1;
            a.t1 = t1;
        }
        ok
    }

    /// Grow or shrink the areas by whole rows, from the top.
    pub fn razor_resize_rows(&mut self, drows: i32) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        for a in &mut self.razor.areas {
            a.row_hi = (a.row_hi + drows).max(a.row_lo);
        }
        true
    }
}
