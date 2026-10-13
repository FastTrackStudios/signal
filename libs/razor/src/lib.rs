//! Razor edit areas — rectangles of time by row.
//!
//! A razor is the answer to "I want *this rectangle*", independent of
//! what is selected and independent of where the things inside it
//! begin and end. Two surfaces need exactly that and need to agree
//! about it: the expression editor over notes, and the arrangement
//! over items.
//!
//! What lives here is the rectangle and the set algebra over it —
//! merging, hit testing, translation. What does NOT live here is any
//! operation on contents, because the contents are the one thing the
//! two surfaces do not share. The expression editor slices notes at an
//! area's edges; the arrangement will split items at them. Same
//! rectangle, different verb.
//!
//! The property that makes a razor different from a marquee, and the
//! reason both surfaces want one: an area **slices** at its edges
//! rather than selecting whole things. Dragging a razor over the middle
//! of a held note, or the middle of a take, takes the middle with it.
//! That is what makes it usable for comping, and it is why the AREA is
//! the unit of operation rather than the note or the item.

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
    #[must_use]
    pub fn new(t0: f64, t1: f64, row_a: i32, row_b: i32) -> Self {
        Self {
            t0: t0.min(t1),
            t1: t0.max(t1),
            row_lo: row_a.min(row_b),
            row_hi: row_a.max(row_b),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.t1 - self.t0 <= 0.0
    }

    #[must_use]
    pub fn width(&self) -> f64 {
        self.t1 - self.t0
    }

    #[must_use]
    pub fn rows(&self) -> i32 {
        self.row_hi - self.row_lo + 1
    }

    #[must_use]
    pub fn contains(&self, t: f64, row: i32) -> bool {
        t >= self.t0 && t <= self.t1 && row >= self.row_lo && row <= self.row_hi
    }

    /// Does this area overlap any part of a thing spanning `start`
    /// to `end` on `row`?
    ///
    /// Takes the span rather than the thing, because the thing is a
    /// note on one surface and a take on the other and this crate
    /// knows about neither. Half-open in time on purpose: something
    /// that merely touches the edge is not inside.
    #[must_use]
    pub fn touches(&self, row: i32, start: f64, end: f64) -> bool {
        row >= self.row_lo && row <= self.row_hi && start < self.t1 && end > self.t0
    }

    #[must_use]
    pub fn translated(&self, dt: f64, drows: i32) -> Self {
        Self {
            t0: self.t0 + dt,
            t1: self.t1 + dt,
            row_lo: self.row_lo + drows,
            row_hi: self.row_hi + drows,
        }
    }
}

/// The razor areas currently active, plus which one the pointer grabbed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RazorSet {
    pub areas: Vec<RazorArea>,
}

impl RazorSet {
    #[must_use]
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
    #[must_use]
    pub fn total_span(&self) -> f64 {
        self.areas.iter().map(|a| a.width()).sum()
    }
}

/// What a razor gesture took hold of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// Nothing: a new area, drawn from the press.
    Draw,
    /// The area at `index` in the set, whole.
    Move { index: usize },
    /// One edge of the area at `index` — its start, or its end.
    Edge { index: usize, start: bool },
}

/// How a razor gesture ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Released {
    /// It never became anything: a press that drew nothing.
    Nothing,
    /// The set changed: an area drawn, moved or resized.
    Changed,
    /// An area was taken out: a grab that went nowhere is a click, and a
    /// click on an area removes it.
    Removed,
}

/// A razor gesture: drawing, moving or resizing an area — the same hand
/// over notes and over items.
///
/// Geometry only. What happens to whatever is inside the area as it
/// moves is the surface's: the arrangement carries the rectangle, the
/// Editor can carry its notes, copy them, stretch them. Each asks this
/// where the area is now, and does its own verb with the answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RazorGesture {
    /// Where the press landed: the anchored corner when drawing, the grab
    /// point when moving.
    pub from: (f64, i32),
    /// The area as it stands now.
    pub area: RazorArea,
    /// The area as it was at the press: a move and a resize are relative
    /// to it, so a slow drag and a fast one land in the same place.
    pub was: RazorArea,
    pub grab: Grab,
    /// The narrowest an edge drag may leave the area.
    min_width: f64,
}

impl RazorGesture {
    /// Drawing a new area from `(t, row)`. Zero-wide until the pointer
    /// moves: an area that sprang into existence on a press would be a
    /// click that edits, and a razor never is.
    #[must_use]
    pub fn draw(t: f64, row: i32) -> Self {
        let area = RazorArea::new(t, t, row, row);
        Self {
            from: (t, row),
            area,
            was: area,
            grab: Grab::Draw,
            min_width: 0.0,
        }
    }

    /// Moving the area `index` (`area` as it is) from `(t, row)`.
    #[must_use]
    pub const fn grab(index: usize, area: RazorArea, t: f64, row: i32) -> Self {
        Self {
            from: (t, row),
            area,
            was: area,
            grab: Grab::Move { index },
            min_width: 0.0,
        }
    }

    /// Resizing the area `index` by its start or end edge, never narrower
    /// than `min_width`.
    #[must_use]
    pub const fn edge(index: usize, area: RazorArea, start: bool, min_width: f64) -> Self {
        let t = if start { area.t0 } else { area.t1 };
        Self {
            from: (t, area.row_lo),
            area,
            was: area,
            grab: Grab::Edge { index, start },
            min_width,
        }
    }

    /// The edge nearer `t` of `area`: `true` for its start.
    #[must_use]
    pub fn nearer_start(area: RazorArea, t: f64) -> bool {
        (t - area.t0).abs() <= (t - area.t1).abs()
    }

    /// The pointer at `(t, row)`, every time snapped by `snap`. `true`
    /// when the area changed.
    pub fn moved(&mut self, t: f64, row: i32, snap: impl Fn(f64) -> f64) -> bool {
        let before = self.area;
        let was = self.was;
        self.area = match self.grab {
            // Drawing: the press is one corner, the pointer the other.
            Grab::Draw => RazorArea::new(snap(self.from.0), snap(t), self.from.1, row),
            // Moving: the whole rectangle travels, keeping its size.
            Grab::Move { .. } => {
                let dt = snap(t) - snap(self.from.0);
                was.translated(dt, row.saturating_sub(self.from.1))
            }
            // Resizing: the dragged edge, which may not cross the fixed
            // one — an inverted area is not a smaller area, it is a
            // broken one.
            Grab::Edge { start: true, .. } => RazorArea::new(
                snap(t).min(was.t1 - self.min_width),
                was.t1,
                was.row_lo,
                was.row_hi,
            ),
            Grab::Edge { start: false, .. } => RazorArea::new(
                was.t0,
                snap(t).max(was.t0 + self.min_width),
                was.row_lo,
                was.row_hi,
            ),
        };
        self.area != before
    }

    /// How far the area has travelled from where it was: `(time, rows)`.
    #[must_use]
    pub const fn delta(&self) -> (f64, i32) {
        (
            self.area.t0 - self.was.t0,
            self.area.row_lo.saturating_sub(self.was.row_lo),
        )
    }

    /// Let go, into `set`.
    pub fn release(self, set: &mut RazorSet) -> Released {
        match self.grab {
            Grab::Draw if self.area.is_empty() => Released::Nothing,
            Grab::Draw => {
                set.add(self.area);
                Released::Changed
            }
            Grab::Move { index } | Grab::Edge { index, .. } => {
                // Out before in, so `add`'s merging sees the set WITHOUT
                // the area being moved; left in, it would merge with the
                // hole it came from and undo the move.
                if index < set.areas.len() {
                    set.areas.remove(index);
                }
                if matches!(self.grab, Grab::Move { .. }) && self.area == self.was {
                    return Released::Removed;
                }
                set.add(self.area);
                Released::Changed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Grab, RazorArea, RazorGesture, RazorSet, Released};

    #[test]
    fn an_area_is_normalised_whichever_way_it_was_drawn() {
        let up = RazorArea::new(4.0, 1.0, 7, 2);
        assert!((up.t0 - 1.0).abs() < f64::EPSILON);
        assert!((up.t1 - 4.0).abs() < f64::EPSILON);
        assert_eq!((up.row_lo, up.row_hi), (2, 7));
        // Rows are inclusive on both ends: one row is one row, not a
        // rectangle of no height.
        assert_eq!(RazorArea::new(0.0, 1.0, 3, 3).rows(), 1);
    }

    #[test]
    fn touching_is_half_open_in_time() {
        let area = RazorArea::new(2.0, 4.0, 0, 1);
        assert!(area.touches(0, 3.0, 3.5), "inside");
        assert!(area.touches(1, 1.0, 9.0), "straddling is touching");
        assert!(
            !area.touches(2, 3.0, 3.5),
            "on a row the area does not cover"
        );
        // Abutting is not overlapping, or every razor would take the
        // thing that merely ends where it begins.
        assert!(!area.touches(0, 0.0, 2.0));
        assert!(!area.touches(0, 4.0, 6.0));
    }

    #[test]
    fn areas_on_the_same_rows_merge_and_others_do_not() {
        let mut set = RazorSet::default();
        set.add(RazorArea::new(0.0, 2.0, 0, 0));
        set.add(RazorArea::new(1.0, 4.0, 0, 0));
        assert_eq!(
            set.areas.len(),
            1,
            "overlapping on the same rows is one area"
        );
        assert!((set.areas[0].t1 - 4.0).abs() < f64::EPSILON);

        // The same span on different rows is a different area — merging
        // it would silently widen what an operation covers.
        set.add(RazorArea::new(1.0, 4.0, 1, 1));
        assert_eq!(set.areas.len(), 2);

        // And an empty one is not an area at all.
        set.add(RazorArea::new(5.0, 5.0, 0, 0));
        assert_eq!(set.areas.len(), 2);
    }

    #[test]
    fn the_last_area_drawn_is_the_one_under_the_pointer() {
        let mut set = RazorSet::default();
        set.add(RazorArea::new(0.0, 4.0, 0, 0));
        set.add(RazorArea::new(0.0, 4.0, 1, 1));
        let (index, _) = set.at(2.0, 1).expect("an area");
        assert_eq!(index, 1, "later areas win, matching draw order");
        assert!(set.at(2.0, 9).is_none());
        assert!(set.remove_at(2.0, 1));
        assert_eq!(set.areas.len(), 1);
    }

    #[test]
    fn the_span_is_what_an_operation_would_cover() {
        let mut set = RazorSet::default();
        set.add(RazorArea::new(0.0, 2.0, 0, 0));
        set.add(RazorArea::new(0.0, 3.0, 1, 1));
        assert!((set.total_span() - 5.0).abs() < f64::EPSILON);
    }

    fn on_the_beat(t: f64) -> f64 {
        (t / 0.5).round() * 0.5
    }

    /// Drawing anchors the press, snaps both ends, spans the rows.
    #[test]
    fn a_draw_spans_from_the_press_to_the_pointer() {
        let mut g = RazorGesture::draw(1.1, 2);
        assert!(g.area.is_empty(), "nothing until it moves");
        assert!(g.moved(2.9, 4, on_the_beat));
        assert_eq!(g.area, RazorArea::new(1.0, 3.0, 2, 4));
        let mut set = RazorSet::default();
        assert_eq!(g.release(&mut set), Released::Changed);
        assert_eq!(set.areas, vec![RazorArea::new(1.0, 3.0, 2, 4)]);
        let still = RazorGesture::draw(1.0, 0);
        assert_eq!(still.release(&mut set), Released::Nothing);
    }

    /// A move carries the rectangle, relative to where it was; a grab
    /// that went nowhere takes the area out.
    #[test]
    fn a_move_carries_the_area_and_a_still_grab_removes_it() {
        let area = RazorArea::new(2.0, 4.0, 1, 1);
        let mut set = RazorSet { areas: vec![area] };
        let mut g = RazorGesture::grab(0, area, 3.0, 1);
        g.moved(4.0, 3, |t| t);
        assert_eq!(g.area, RazorArea::new(3.0, 5.0, 3, 3));
        assert_eq!(g.delta(), (1.0, 2));
        assert_eq!(g.release(&mut set), Released::Changed);
        assert_eq!(set.areas, vec![RazorArea::new(3.0, 5.0, 3, 3)]);
        let still = RazorGesture::grab(0, set.areas[0], 4.0, 3);
        assert_eq!(still.release(&mut set), Released::Removed);
        assert!(set.is_empty());
    }

    /// An edge moves alone and never crosses the other.
    #[test]
    fn an_edge_resizes_and_never_crosses() {
        let area = RazorArea::new(2.0, 4.0, 0, 1);
        let mut end = RazorGesture::edge(0, area, false, 0.25);
        end.moved(6.2, 9, on_the_beat);
        assert_eq!(end.area, RazorArea::new(2.0, 6.0, 0, 1), "rows stay");
        end.moved(0.0, 0, on_the_beat);
        assert_eq!(
            end.area,
            RazorArea::new(2.0, 2.25, 0, 1),
            "not past the start"
        );
        let mut start = RazorGesture::edge(0, area, true, 0.25);
        start.moved(1.0, 0, |t| t);
        assert_eq!(start.area, RazorArea::new(1.0, 4.0, 0, 1));
        assert!(RazorGesture::nearer_start(area, 2.4));
        assert!(!RazorGesture::nearer_start(area, 3.6));
        let mut set = RazorSet { areas: vec![area] };
        assert_eq!(start.release(&mut set), Released::Changed);
        assert_eq!(set.areas, vec![RazorArea::new(1.0, 4.0, 0, 1)]);
        let _ = Grab::Draw;
    }
}
