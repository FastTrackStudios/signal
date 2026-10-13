//! A track's own markers and regions ([`crate::track_regions`]) in the
//! ruler while it is selected.
//!
//! With its folders' (a kit piece's track shows the kit's grid and
//! no-quantize regions): the marks of what is being edited, where the
//! song's own are read.
//!
//! The window draws from a picture of them read off its thread (as the hit
//! markers are), read again whenever they change.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyrender::PaintScene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use crate::track_regions::{Marks, Origin, RegionKind};

/// The tracks' marks as the window draws them.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub marks: HashMap<String, Marks>,
}

static SHOWN: Mutex<Option<Arc<Snapshot>>> = Mutex::new(None);
static VERSION: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
static AGAIN: AtomicBool = AtomicBool::new(false);

/// The picture, once one has been read.
#[must_use]
pub fn current() -> Option<Arc<Snapshot>> {
    SHOWN.lock().ok().and_then(|s| s.clone())
}

/// Goes up on every change: a window compares it with the one it drew.
#[must_use]
pub fn version() -> u64 {
    VERSION.load(Ordering::Relaxed)
}

/// Read the marks again, off the window's thread.
pub fn refresh() {
    #[cfg(feature = "native")]
    {
        AGAIN.store(true, Ordering::Release);
        if BUSY.swap(true, Ordering::AcqRel) {
            return;
        }
        let Some(runtime) = crate::open::runtime() else {
            BUSY.store(false, Ordering::Release);
            return;
        };
        runtime.spawn_blocking(|| {
            loop {
                AGAIN.store(false, Ordering::Release);
                if let Ok((daw, project)) = crate::actions::song() {
                    publish(Snapshot {
                        marks: crate::track_regions::all(daw, &project)
                            .into_iter()
                            .collect(),
                    });
                }
                BUSY.store(false, Ordering::Release);
                if !AGAIN.load(Ordering::Acquire) || BUSY.swap(true, Ordering::AcqRel) {
                    break;
                }
            }
        });
    }
}

/// Show `snapshot`.
pub fn publish(snapshot: Snapshot) {
    if let Ok(mut shown) = SHOWN.lock() {
        *shown = Some(Arc::new(snapshot));
    }
    VERSION.fetch_add(1, Ordering::Relaxed);
}

/// A region's colour by what it says: a grid teal, as played red, a fill
/// amber, a note grey.
const fn tint(kind: RegionKind) -> Color {
    match kind {
        RegionKind::Grid => Color::from_rgba8(0x2d, 0xd4, 0xbf, 0xff),
        RegionKind::NoQuantize => Color::from_rgba8(0xef, 0x44, 0x44, 0xff),
        RegionKind::Fill => Color::from_rgba8(0xf5, 0x9e, 0x0b, 0xff),
        RegionKind::Note => Color::from_rgba8(0x9c, 0xa3, 0xaf, 0xff),
    }
}

/// The Grid regions over `tracks` (a track and the folders above it) that
/// hold: what the grid is drawn by.
#[must_use]
pub fn grid_regions(tracks: &[String]) -> Vec<crate::ruler::GridRegion> {
    let Some(snapshot) = current() else {
        return Vec::new();
    };
    let mut out: Vec<crate::ruler::GridRegion> = tracks
        .iter()
        .filter_map(|t| snapshot.marks.get(t))
        .flat_map(|m| m.regions.iter())
        .filter(|r| r.kind == RegionKind::Grid && r.counts())
        .filter_map(|r| Some((r.start, r.end, r.division?)))
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// The grids `tracks`' regions quantize by — each Grid region's and each
/// fill's with a grid of its own — for the grid in force
/// ([`crate::grid_in_force`]).
#[must_use]
pub fn quantize_regions(tracks: &[String]) -> Vec<crate::ruler::GridRegion> {
    let Some(snapshot) = current() else {
        return Vec::new();
    };
    tracks
        .iter()
        .filter_map(|t| snapshot.marks.get(t))
        .flat_map(|m| m.regions.iter())
        .filter(|r| matches!(r.kind, RegionKind::Grid | RegionKind::Fill) && r.counts())
        .filter_map(|r| Some((r.start, r.end, r.division?)))
        .collect()
}

/// The spans of `tracks`' regions that hold `time` — a Grid region's
/// (`grid`) or every other's — for a double-click to select.
#[must_use]
pub fn spans_at(tracks: &[String], time: f64, grid: bool) -> Vec<(f64, f64)> {
    let Some(snapshot) = current() else {
        return Vec::new();
    };
    tracks
        .iter()
        .filter_map(|t| snapshot.marks.get(t))
        .flat_map(|m| m.regions.iter())
        .filter(|r| (r.kind == RegionKind::Grid) == grid && time >= r.start && time < r.end)
        .map(|r| (r.start, r.end))
        .collect()
}

/// The selected track's own markers and regions, and its folders', in the
/// ruler's MARKS lane.
///
/// What is known about the stretch being edited, read where the song's
/// marks are, taking nothing from the rows. `tracks` the
/// selected track and the folders above it; `origin` as the ruler is drawn.
pub fn paint(
    painter: &mut impl PaintScene,
    (palette, font): (&crate::arrangement::Palette, &crate::text::Font),
    view: crate::arrangement::Viewport,
    origin: (f64, f64),
    tracks: &[String],
) {
    /// The names' size, as the ruler's lanes write theirs.
    const SIZE: f32 = 8.0;
    let Some(snapshot) = current() else {
        return;
    };
    let row = crate::ruler::MARKS_ROW;
    if !crate::ruler::lane_shown(row) {
        return;
    }
    let (ox, oy) = origin;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    let x_of = |t: f64| t.mul_add(view.pps, left - view.scroll_x);
    let y0 = oy + crate::ruler::row_top(row) + 2.0;
    let y1 = y0 + crate::ruler::LANE_H - 4.0;
    let baseline = y1 - 2.0;
    for marks in tracks.iter().filter_map(|t| snapshot.marks.get(t)) {
        // A grid region is the GRID lane's (`crate::ruler::grid_lane`).
        for region in marks.regions.iter().filter(|r| r.kind != RegionKind::Grid) {
            let (a, b) = (x_of(region.start).max(left), x_of(region.end).min(right));
            if b <= a {
                continue;
            }
            let color = if region.origin == Origin::Rejected {
                palette.text_faint
            } else {
                tint(region.kind)
            };
            let fill = if region.origin == Origin::Auto {
                0.3
            } else {
                0.55
            };
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color.multiply_alpha(fill),
                None,
                &Rect::new(a, y0, b, y1),
            );
            for x in [a, b - 1.0] {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color,
                    None,
                    &Rect::new(x, y0, x + 1.0, y1),
                );
            }
            let label = region.label();
            if !label.is_empty() && b - a > f64::from(SIZE) * 2.0 {
                crate::tcp::glyphs(painter, font, palette.text, &label, a + 3.0, baseline, SIZE);
            }
        }
        for marker in &marks.markers {
            let x = x_of(marker.at);
            if x < left || x > right {
                continue;
            }
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.text,
                None,
                &Rect::new(x, y0, x + 1.5, y1),
            );
            crate::tcp::glyphs(
                painter,
                font,
                palette.text,
                &marker.name,
                x + 3.0,
                baseline,
                SIZE,
            );
        }
    }
}
