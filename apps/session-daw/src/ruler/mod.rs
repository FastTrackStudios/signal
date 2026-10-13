//! The ruler, and the grid under it.
//!
//! # Why this is not in the recorded scene
//!
//! Everything else the window draws is recorded once in content space
//! and replayed under a transform, which is what makes scrolling and
//! zooming free. The grid cannot work that way: which lines exist
//! depends on the zoom. At one bar to the screen it draws sixteenths; at
//! five hundred bars it draws every sixteen bars, and a recorded scene
//! would have to hold every division at once and hide most of them.
//!
//! So this is built per frame — but only for what is on screen, which is
//! at most a screen's width divided by the closest two lines may sit,
//! and in practice a hundred or so lines. That is nothing next to the
//! twenty thousand items the recorded scene holds.
//!
//! # The division is not ours
//!
//! [`adaptive_grid`] picks it, and it already exists in this repo
//! because the expression editor needed exactly this. It follows Ilias
//! Poulakis's Adaptive Grid scripts for REAPER, so the grid coarsens and
//! refines the way a REAPER user expects it to rather than the way this
//! file would have invented.

use adaptive_grid::Adaptive;
use anyrender::PaintScene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use crate::arrangement::{Palette, TCP_WIDTH, Viewport};
use crate::text::Font;

mod bars;
mod chords;
mod hit;
mod lanes;
mod tempo;
#[cfg(test)]
mod tests;
mod timeline;

pub use bars::*;
pub use chords::*;
pub use hit::*;
pub use lanes::*;
pub use tempo::*;
pub use timeline::*;

/// The bar strip's height. REAPER's, measured.
pub const BARS_H: f64 = 28.0;

/// One ruler lane's height.
pub const LANE_H: f64 = 15.0;

/// How many ruler lanes there are, over the bars.
///
/// The FTS convention (REAPER 7.62's ruler lanes, numbered from 0 as
/// REAPER's API and the daw service do): lane 0 is the SONG, one region
/// over the whole song; lane 1 the SECTIONS, a region per verse and
/// chorus; lane 2 the MARKS — SONGSTART, SONGEND and the like. Always three, so a session with fewer still lays its rows out
/// where every other session does.
pub const LANES: usize = 3;

/// How wide the strip of lane names is, against the ruler's left edge:
/// the track panel's mute/solo column, so the names line up over the
/// buttons below them.
///
/// The names used to start at the corner's left edge and their rules ran
/// across it; the corner is the DAW view's main toolbar now, so the names
/// sit in this column and the rules stop at it.
pub const LABEL_W: f64 = TCP_WIDTH - daw_ui::studio::panel::TINT_W;

/// A lane's name in the label column, shrunk to fit it — centred, as the
/// mute and solo buttons under it are.
fn label(
    painter: &mut impl PaintScene,
    font: &Font,
    color: Color,
    // Where the lanes start: the label column is the LABEL_W before it.
    left: f64,
    name: &str,
    baseline: f64,
    size: f32,
) {
    let (name, size) = font.fit(name, size, 5.0, LABEL_W - 4.0);
    let x = left - LABEL_W / 2.0 - font.width(&name, size) / 2.0;
    crate::tcp::glyphs(painter, font, color, &name, x, baseline, size);
}

/// What the lanes are called, top to bottom.
pub const LANE_NAMES: [&str; LANES] = ["SONG", "SECTIONS", "MARKS"];

/// How tall the tempo strip is, above the bar numbers.
///
/// Its own row rather than a flag among the marks: a tempo change is
/// not a place in the song, it is a change to what every number below
/// it MEANS. Putting it in the marks lane would file it with the things
/// it reinterprets.
pub const TEMPO_H: f64 = 13.0;

/// The CHORDS lane's height: the song's key changes and its chords,
/// read off the Keyflow folder's KEY and CHORD tracks. A lane like the
/// others, so the ruler reads as one set of rows; the chords are lettered
/// smaller to fit it.
pub const CHORD_H: f64 = LANE_H;

/// Whether the ruler carries the CHORDS lane: unset, off, on.
static CHORDS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether the ruler shows the CHORDS lane. On unless
/// `FTS_RULER_CHORDS=0` — it is where a prepared session keeps its key
/// and chords once the Keyflow folder is out of the panel.
///
/// Process-wide rather than per view because the ruler's height is: the
/// hit tests, the lanes' offset and the corner beside the ruler all ask
/// it, and a height that two of them disagreed about would put every
/// click one lane off.
#[must_use]
pub fn chords_shown() -> bool {
    use std::sync::atomic::Ordering;
    match CHORDS.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = std::env::var("FTS_RULER_CHORDS").map_or(true, |v| v != "0");
            CHORDS.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Show or hide the CHORDS lane.
pub fn show_chords(on: bool) {
    CHORDS.store(if on { 2 } else { 1 }, std::sync::atomic::Ordering::Relaxed);
}

/// The CHORDS lane's height as the ruler has it now: nothing when it is
/// hidden.
#[must_use]
pub fn chords_h() -> f64 {
    if chords_shown() { CHORD_H } else { 0.0 }
}

/// How far down the ruler a lane row starts.
///
/// SONG and SECTIONS stack from the top; the CHORDS lane, when shown,
/// sits under SECTIONS and pushes MARKS below it. The chords follow the
/// sections they are the harmony of, and the marks — the sparsest lane —
/// sit nearest the bars, where a SONGSTART flag is read against its bar.
#[must_use]
pub fn row_top(row: usize) -> f64 {
    let above = (0..row).filter(|&r| lane_shown(r)).count();
    let lanes = LANE_H * crate::num::coord(above);
    if row >= MARKS_ROW {
        lanes + chords_h()
    } else {
        lanes
    }
}

/// The whole top strip: the lanes, the chords, then the tempo and the
/// bars under them.
#[must_use]
pub fn ruler_h() -> f64 {
    let lanes = (0..LANES).filter(|&r| lane_shown(r)).count();
    bars_h() + tempo_h() + LANE_H * crate::num::coord(lanes) + chords_h()
}

/// The bar numbers' row on a slim ruler: small numbers, a short tick.
pub const SLIM_BARS_H: f64 = 13.0;

/// The bar numbers' row as the ruler has it now.
#[must_use]
pub fn bars_h() -> f64 {
    if slim() { SLIM_BARS_H } else { BARS_H }
}

/// The slim ruler, for a touchscreen: the sections and small bar numbers
/// (the chords on request), and not the song band, the marks or the
/// tempo. A finger's ruler
/// is drawn a third bigger, and six lanes of it took a sixth of an iPad
/// from the tracks; Logic's is one line.
static SLIM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Make the ruler slim, or full. A slim ruler starts without its CHORDS
/// lane — the sections and the bars are the whole of it — unless the
/// chords were asked for (the toolbar's switch, `FTS_RULER_CHORDS=1`).
pub fn set_slim(on: bool) {
    use std::sync::atomic::Ordering;
    SLIM.store(on, Ordering::Relaxed);
    if on && CHORDS.load(Ordering::Relaxed) == 0 {
        let asked = std::env::var("FTS_RULER_CHORDS").is_ok_and(|v| v == "1");
        CHORDS.store(if asked { 2 } else { 1 }, Ordering::Relaxed);
    }
}

/// Whether the ruler is slim ([`set_slim`]).
#[must_use]
pub fn slim() -> bool {
    SLIM.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether a lane row is on the ruler: every one, or on a slim ruler the
/// sections alone.
#[must_use]
pub fn lane_shown(row: usize) -> bool {
    !slim() || row == SECTIONS_ROW
}

/// The TEMPO lane's height: nothing on a slim ruler.
#[must_use]
pub fn tempo_h() -> f64 {
    if slim() { 0.0 } else { TEMPO_H }
}

/// As much of a name as fits a width, or nothing.
fn fit<'a>(font: &Font, name: &'a str, size: f32, room: f64) -> Option<&'a str> {
    let mut name = name;
    while !name.is_empty() && font.width(name, size) > room {
        let mut end = name.len().saturating_sub(1);
        while end > 0 && !name.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        name = name.get(..end).unwrap_or("");
    }
    (!name.is_empty()).then_some(name)
}

/// A `#rrggbb` string as a colour — what the studio project resolves
/// region and marker colours to.
fn css_hex(css: &str) -> Option<Color> {
    let digits = css.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(digits.get(at..at.saturating_add(2))?, 16).ok();
    Some(Color::from_rgba8(
        channel(0)?,
        channel(2)?,
        channel(4)?,
        0xff,
    ))
}

/// `0.0, 1.0, 2.0, …`, up to `max`.
///
/// A float counter compared against a float bound is how a grid loop
/// becomes either an infinite one or a drifting one; this keeps the
/// counting in integers and converts once. `u32` because a count that
/// did not fit one would have stopped at `max` long before.
fn counts(max: usize) -> impl Iterator<Item = f64> {
    (0..max).map_while(|i| u32::try_from(i).ok().map(f64::from))
}

/// One flat rectangle.
fn fill(painter: &mut impl PaintScene, color: Color, rect: Rect) {
    painter.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
}

/// Vertical lines every `step` seconds across the lane area.
fn line_every(
    painter: &mut impl PaintScene,
    view: Viewport,
    (step, from, to): (f64, f64, f64),
    (color, width): (Color, f64),
    origin: (f64, f64),
    skip: &[GridRegion],
) {
    let (ox, oy) = origin;
    // A screen of lines is the most that can ever be visible; anything
    // beyond that is a division that should have coarsened, and drawing
    // it would cost a frame to produce a grey band.
    const MAX: usize = 2048;

    let first = (from / step).floor();
    for n in counts(MAX) {
        let t = (first + n) * step;
        if t > to {
            break;
        }
        if t < from - 1e-9 || in_grid_region(skip, t) {
            continue;
        }
        let x = t.mul_add(view.pps, view.panel_w - view.scroll_x);
        if x >= view.panel_w {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color,
                None,
                &Rect::new(ox + x, oy + ruler_h(), ox + x + width, oy + view.height),
            );
        }
    }
}

/// A stretch on a grid of its own (a Grid region of the edited track's):
/// `(start, end, division)`, seconds and a whole note's division.
pub type GridRegion = (f64, f64, u32);

thread_local! {
    /// The grid regions the ruler and the grid draw by this frame.
    static GRID_REGIONS: std::cell::RefCell<Vec<GridRegion>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Draw the grid by `regions` from now on: inside each, its own lines in
/// place of the beats and the subdivisions.
pub fn set_grid_regions(regions: Vec<GridRegion>) {
    GRID_REGIONS.with(|r| *r.borrow_mut() = regions);
}

/// The grid regions in force.
fn grid_regions() -> Vec<GridRegion> {
    GRID_REGIONS.with(|r| r.borrow().clone())
}

/// Whether `t` is inside one of `regions` (its end not).
fn in_grid_region(regions: &[GridRegion], t: f64) -> bool {
    regions
        .iter()
        .any(|(a, b, _)| t >= *a - 1e-9 && t < *b - 1e-9)
}

// ─── What the pointer is on ─────────────────────────────────────────
