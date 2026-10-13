//! Contextual zoom — "show me about N notes around here".
//!
//! Ported from FeedTheCat's *MIDI Editor Magic* (`FTC_MeMagic.lua`,
//! MIT, Ilias-Timon Poulakis), which solves the problem every
//! measure-based zoom has: four bars is the wrong amount of screen for
//! a whole note and for a 32nd-note run, so a fixed zoom is always
//! wrong somewhere in the part.
//!
//! Instead, zoom to a **note count**. The span is derived from a
//! locally-weighted average note length using Shepard's inverse
//! distance weighting: notes near the cursor count for more, so the
//! view adapts to the density of the passage you are actually looking
//! at rather than to the average of the whole item.
//!
//! Gaps between notes are folded into the length (a sparse passage
//! should zoom out), but capped — otherwise one long rest would blow
//! the view open.

use crate::camera::{Camera, Content, Viewport};

/// Tuning for the contextual zoom modes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmartZoom {
    /// Roughly how many notes to fit horizontally.
    pub notes_visible: f64,
    /// How strongly nearby notes outweigh distant ones. 0 makes
    /// distance irrelevant (a plain average over the item); higher
    /// values make the zoom hug the local passage.
    pub smoothing: f64,
    /// Where the anchor lands in the resulting view.
    /// 0 = left edge, 0.5 = centred, 1 = right edge.
    pub cursor_alignment: f64,
    /// Never show fewer rows than this, so a single-note passage does
    /// not zoom to one enormous row.
    pub min_rows: f64,
    /// Row-height ceiling. Also a performance bound — huge rows mean
    /// huge note rectangles for no extra information.
    pub max_px_per_row: f64,
    /// Row to centre on when the area contains no notes at all.
    pub base_row: i32,
}

impl Default for SmartZoom {
    fn default() -> Self {
        Self {
            notes_visible: 20.0,
            smoothing: 0.75,
            cursor_alignment: 0.5,
            min_rows: 8.0,
            max_px_per_row: 32.0,
            base_row: 60,
        }
    }
}

/// Horizontal zoom behaviours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HorizontalMode {
    Keep,
    /// Frame the whole item.
    Item,
    /// A fixed number of bars around the anchor.
    Measures(u8),
    /// A fixed number of bars, clamped to the item.
    MeasuresInItem(u8),
    /// Density-aware: about `notes_visible` notes around the anchor.
    SmartNotes,
    /// Same, clamped to the item.
    SmartNotesInItem,
    /// Keep the zoom, just bring the anchor into view.
    ScrollToAnchor,
}

/// Vertical zoom behaviours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VerticalMode {
    Keep,
    /// Fit the notes currently on screen horizontally.
    NotesInView,
    /// Fit every note in the item.
    AllNotes,
    /// Keep the zoom, scroll the anchor row into view.
    ScrollToRow,
    CenterOfNotesInView,
    CenterOfAllNotes,
    LowestInView,
    HighestInView,
    LowestInItem,
    HighestInItem,
}

/// One note reduced to what zoom cares about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: f64,
    pub end: f64,
    pub row: i32,
}

impl Span {
    fn len(&self) -> f64 {
        (self.end - self.start).max(1e-9)
    }
    fn center(&self) -> f64 {
        (self.start + self.end) * 0.5
    }
}

/// Locally-weighted average note length at `at`, in document units.
///
/// Shepard's inverse distance weighting: each note contributes its own
/// length plus the (capped) gap that follows it, weighted by
/// `1 / distance^smoothing`.
///
/// Returns `None` when nothing is in range — the caller then falls back
/// to a measure-based zoom rather than inventing a span.
pub fn weighted_note_length(spans: &[Span], at: f64, cfg: SmartZoom) -> Option<f64> {
    if spans.is_empty() {
        return None;
    }
    let mut sorted: Vec<Span> = spans.to_vec();
    sorted.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(core::cmp::Ordering::Equal)
    });

    let mut length_sum = 0.0;
    let mut weight_sum = 0.0;
    let mut nearest = f64::INFINITY;
    let mut prev_end: Option<f64> = None;

    for s in &sorted {
        let len = s.len();
        // Fold the preceding gap in, so a sparse passage zooms out —
        // but cap it, or one long rest blows the whole view open.
        let gap = match prev_end {
            Some(pe) if s.start >= pe => (s.start - pe).min(len * cfg.notes_visible),
            _ => 0.0,
        };
        prev_end = Some(prev_end.map_or(s.end, |pe: f64| pe.max(s.end)));

        let mut distance = (s.center() - at).abs();
        // A note we are inside of must not get an unbounded weight.
        if distance < len {
            distance = len;
        }
        nearest = nearest.min(distance);

        let weight = 1.0 / distance.powf(cfg.smoothing.max(0.0));
        length_sum += weight * (len + gap);
        weight_sum += weight;
    }

    if weight_sum <= 0.0 {
        return None;
    }
    let avg = length_sum / weight_sum;
    let mut span = avg * cfg.notes_visible;

    // Standing in a wide empty stretch: widen enough to reveal the
    // nearest note, or the view shows nothing at all.
    if nearest.is_finite() && span / 2.5 < nearest {
        span = nearest * 2.5;
    }
    Some(span)
}

/// Place a span of `length` around `anchor` per `cursor_alignment`.
pub fn align(anchor: f64, length: f64, alignment: f64) -> (f64, f64) {
    let a = alignment.clamp(0.0, 1.0);
    (anchor - length * a, anchor + length * (1.0 - a))
}

/// Apply a horizontal mode, returning the new camera.
// Eight camera/zoom inputs with no natural grouping — bundling them
// into a struct would only move the argument list to the call sites.
#[allow(clippy::too_many_arguments)]
pub fn apply_horizontal(
    camera: Camera,
    mode: HorizontalMode,
    spans: &[Span],
    anchor: f64,
    content: Content,
    vp: Viewport,
    units_per_bar: f64,
    cfg: SmartZoom,
) -> Camera {
    let mut cam = camera;
    let visible = vp.w * camera.units_per_px;

    let target = match mode {
        HorizontalMode::Keep => None,
        HorizontalMode::Item => {
            return Camera {
                t0: content.t_start,
                units_per_px: ((content.t_end - content.t_start).max(1e-6)) / vp.w,
                ..cam
            };
        }
        HorizontalMode::Measures(n) | HorizontalMode::MeasuresInItem(n) => {
            Some(units_per_bar * n.max(1) as f64)
        }
        HorizontalMode::SmartNotes | HorizontalMode::SmartNotesInItem => {
            // No notes in range is not a failure — fall back to a
            // measure zoom rather than leaving the view unchanged.
            Some(weighted_note_length(spans, anchor, cfg).unwrap_or(units_per_bar * 4.0))
        }
        HorizontalMode::ScrollToAnchor => Some(visible),
    };

    let Some(length) = target else { return cam };
    let (mut t0, mut t1) = align(anchor, length, cfg.cursor_alignment);

    if matches!(
        mode,
        HorizontalMode::MeasuresInItem(_) | HorizontalMode::SmartNotesInItem
    ) {
        // Slide, do not squash: a view clamped by narrowing would
        // change zoom level as you approach the item edge.
        let width = t1 - t0;
        if t0 < content.t_start {
            t0 = content.t_start;
            t1 = t0 + width;
        }
        if t1 > content.t_end {
            t1 = content.t_end;
            t0 = (t1 - width).max(content.t_start);
        }
    }

    cam.t0 = t0;
    cam.units_per_px = ((t1 - t0).max(1e-6)) / vp.w;
    cam
}

/// Apply a vertical mode.
pub fn apply_vertical(
    camera: Camera,
    mode: VerticalMode,
    spans: &[Span],
    anchor_row: f64,
    vp: Viewport,
    view_span: (f64, f64),
    cfg: SmartZoom,
) -> Camera {
    let mut cam = camera;

    let in_view: Vec<&Span> = spans
        .iter()
        .filter(|s| s.end >= view_span.0 && s.start <= view_span.1)
        .collect();

    let extent = |set: &[&Span]| -> Option<(f64, f64)> {
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for s in set {
            lo = lo.min(s.row as f64);
            hi = hi.max(s.row as f64);
        }
        (lo <= hi).then_some((lo, hi))
    };
    let all: Vec<&Span> = spans.iter().collect();

    match mode {
        VerticalMode::Keep => {}
        VerticalMode::NotesInView | VerticalMode::AllNotes => {
            let set = if mode == VerticalMode::AllNotes {
                &all
            } else {
                &in_view
            };
            let (lo, hi) = extent(set).unwrap_or((cfg.base_row as f64, cfg.base_row as f64));
            let rows = (hi - lo + 1.0).max(cfg.min_rows);
            cam.vertical.px_per_row = (vp.h / rows).min(cfg.max_px_per_row);
            cam.vertical.center = (lo + hi) * 0.5;
        }
        VerticalMode::ScrollToRow => cam.vertical.center = anchor_row,
        VerticalMode::CenterOfNotesInView => {
            if let Some((lo, hi)) = extent(&in_view) {
                cam.vertical.center = (lo + hi) * 0.5;
            }
        }
        VerticalMode::CenterOfAllNotes => {
            if let Some((lo, hi)) = extent(&all) {
                cam.vertical.center = (lo + hi) * 0.5;
            }
        }
        VerticalMode::LowestInView => {
            if let Some((lo, _)) = extent(&in_view) {
                cam.vertical.center = lo + vp.h * 0.5 / cam.vertical.px_per_row - 1.0;
            }
        }
        VerticalMode::HighestInView => {
            if let Some((_, hi)) = extent(&in_view) {
                cam.vertical.center = hi - vp.h * 0.5 / cam.vertical.px_per_row + 1.0;
            }
        }
        VerticalMode::LowestInItem => {
            if let Some((lo, _)) = extent(&all) {
                cam.vertical.center = lo + vp.h * 0.5 / cam.vertical.px_per_row - 1.0;
            }
        }
        VerticalMode::HighestInItem => {
            if let Some((_, hi)) = extent(&all) {
                cam.vertical.center = hi - vp.h * 0.5 / cam.vertical.px_per_row + 1.0;
            }
        }
    }
    cam
}

/// A horizontal + vertical pair, chosen by where the pointer is.
///
/// This is MeMagic's real contribution: one key does the *right* zoom
/// depending on which part of the editor you are pointing at, so a
/// single shortcut covers what would otherwise be a dozen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomModes {
    pub horizontal: HorizontalMode,
    pub vertical: VerticalMode,
}

impl ZoomModes {
    /// Pointer over the note area: adapt to local density, and fit the
    /// notes actually in view.
    pub const NOTE_AREA: ZoomModes = ZoomModes {
        horizontal: HorizontalMode::SmartNotesInItem,
        vertical: VerticalMode::NotesInView,
    };
    /// Pointer over the piano keys: the vertical axis is what you are
    /// pointing at, so frame the whole item and all its notes.
    pub const KEYS: ZoomModes = ZoomModes {
        horizontal: HorizontalMode::Item,
        vertical: VerticalMode::AllNotes,
    };
    /// Pointer over the ruler: time is what you are pointing at.
    pub const RULER: ZoomModes = ZoomModes {
        horizontal: HorizontalMode::SmartNotes,
        vertical: VerticalMode::Keep,
    };
    /// Pointer over a CC dimension: keep pitch alone entirely.
    pub const CC_LANE: ZoomModes = ZoomModes {
        horizontal: HorizontalMode::SmartNotes,
        vertical: VerticalMode::Keep,
    };
}
