//! What a press on the ruler lands on: which lane, which band or mark,
//! and which edge of it.

use super::*;

/// Which part of a region the pointer is over.
///
/// The same three the arrangement uses for an item, and for the same
/// reason: a band's ends mean "change where it stops" and its middle
/// means "move the whole thing", and a user expects that everywhere
/// something has a start and an end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Start,
    Body,
    End,
}

/// A ruler mark's edge sitting at a time.
///
/// The ruler's answer to [`crate::arrangement::ItemZone`] in
/// `edges_at`: what else is at this moment, and which of its own ends
/// it is, so a drag can carry all of them and still write each one the
/// edit it needs.
///
/// Each variant carries the bound that is NOT moving, captured at the
/// press. A region edit sets both bounds whatever the drag touched —
/// REAPER's setter takes both — and by release the edge has moved, so
/// reading the other bound then would be reading it off a region that
/// has already changed underneath.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MarkEdge {
    /// A region whose start sits here, and where it ends.
    RegionStart { id: u32, end: f64 },
    /// A region whose end sits here, and where it starts.
    RegionEnd { id: u32, start: f64 },
    /// A marker, which is a position and has no other bound.
    Marker { id: u32 },
}

/// What a press on the ruler landed on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum On {
    /// A marker's flag, by REAPER's marker number.
    Marker { id: u32 },
    /// A region band, by REAPER's region number.
    Region { id: u32, zone: Zone },
    /// A lane with nothing on it at that time. The LANE is what a press
    /// here will create in: the marks lane makes a marker, a region
    /// lane makes a region. That is the whole reason there is no tool
    /// to choose — the ruler already says what you meant.
    Lane { row: usize },
    /// The bars, under the lanes: the timeline itself.
    Bars,
}

/// How near a region's edge takes hold of the edge rather than the body.
///
/// In pixels, not seconds, because it is a question about the pointer
/// and not about the music — at a far zoom a whole bar can be a pixel,
/// and a grip measured in time would swallow the band.
pub(super) const EDGE_GRIP: f64 = 4.0;

/// The box an inline rename occupies for a mark at `at` in lane `row`.
///
/// Placed where the mark is drawn, not in a dialog: the name you are
/// typing has to be next to the thing it names, or two marks a bar
/// apart are indistinguishable while you rename one of them.
///
/// Clamped to the visible timeline at both ends, so a band starting off
/// screen is still renamed somewhere you can see.
#[must_use]
pub fn field(view: Viewport, origin: (f64, f64), row: usize, at: f64) -> Rect {
    const WIDTH: f64 = 140.0;
    let (ox, oy) = origin;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    let top = oy + row_top(row);
    let x0 = at
        .mul_add(view.pps, left - view.scroll_x)
        .clamp(left, (right - WIDTH).max(left));
    Rect::new(x0, top + 1.0, (x0 + WIDTH).min(right), top + LANE_H - 1.0)
}

/// The lane a y falls in, or `None` if it is in the bars.
#[must_use]
///
/// The CHORDS lane is not one: a press there falls to the bars, like a
/// press on the tempo, rather than making a marker in a lane of chords.
pub fn lane_at(y: f64, top: f64) -> Option<usize> {
    let down = y - top;
    (0..LANES).filter(|&row| lane_shown(row)).find(|&row| {
        let from = row_top(row);
        down >= from && down < from + LANE_H
    })
}

/// What is under a point on the ruler.
///
/// Takes the same lists the drawing takes, so what you click is what
/// you see — a second geometry would drift from the first the day
/// either changed.
///
/// A marker is a flag, so it is hit by nearness in PIXELS rather than
/// by containing the time: it has no width to be inside of.
#[must_use]
pub fn on(
    x: f64,
    y: f64,
    top: f64,
    left: f64,
    pps: f64,
    scroll_x: f64,
    sections: &[daw_ui::studio::project::Section],
    markers: &[daw_ui::studio::project::Marker],
) -> On {
    let Some(row) = lane_at(y, top) else {
        return On::Bars;
    };
    let x_of = |t: f64| t.mul_add(pps, left - scroll_x);

    // Markers first: a flag drawn over a band is a flag you can take
    // hold of, and the drawing puts them on top.
    let mut nearest: Option<(f64, u32)> = None;
    for marker in markers {
        if lane_row(marker.lane) != row {
            continue;
        }
        let away = (x_of(marker.at) - x).abs();
        if away <= MARKER_GRIP && nearest.is_none_or(|(best, _)| away < best) {
            nearest = Some((away, marker.idx));
        }
    }
    if let Some((_, id)) = nearest {
        return On::Marker { id };
    }

    for section in sections {
        if lane_row(section.lane) != row {
            continue;
        }
        let (x0, x1) = (x_of(section.start), x_of(section.end));
        if x < x0 || x > x1 {
            continue;
        }
        // A band too narrow to have a middle is all body: offering an
        // edge grip on something four pixels wide means the user can
        // never move it.
        let zone = if x1 - x0 < EDGE_GRIP * 3.0 {
            Zone::Body
        } else if x - x0 <= EDGE_GRIP {
            Zone::Start
        } else if x1 - x <= EDGE_GRIP {
            Zone::End
        } else {
            Zone::Body
        };
        return On::Region {
            id: section.id,
            zone,
        };
    }

    On::Lane { row }
}

/// How near a marker's flag counts as being on it, in pixels.
///
/// Wider than the flag is drawn. A marker is a position, and a position
/// has no width — asking the user to hit two pixels is asking them to
/// miss.
pub(super) const MARKER_GRIP: f64 = 6.0;

/// Which lane makes markers, by the FTS convention.
///
/// The third: SONG, SECTIONS, MARKS. A press in it creates a marker;
/// a press in either of the others creates a region.
pub const MARKS_ROW: usize = 2;

/// Which lane makes a song section.
///
/// The second. A region here IS a section — that is the rule the song
/// model reads the arrangement by, so it is the one this file has to
/// agree with.
pub const SECTIONS_ROW: usize = 1;

/// The lane number to store for a row.
///
/// The inverse of `lane_row`: rows and lanes both count from 0.
#[must_use]
pub const fn lane_of(row: usize) -> u32 {
    row as u32
}

// ─── Counting through tempo and signature changes ───────────────────
