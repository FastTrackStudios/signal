//! The automation lanes on screen: where each is, what is drawn in it,
//! and what a point in it is over.
//!
//! One geometry ([`on_screen`]) for the drawing and the hit test, so a
//! point is grabbed where it is drawn. The lanes are drawn live every
//! frame rather than recorded with the items: a lane holds a handful of
//! points, and a drag reshapes it every frame.
//!
//! Each lane has two parts: its header in the track panel's column (its
//! name, the value playing under the cursor, its mode, a button that
//! hides it) and its body under the track's items (the curve as it plays,
//! the envelope's own points, its automation items as labelled windows
//! with their curves' points).

use anyrender::PaintScene;
use vello::kurbo::{Affine, BezPath, Circle, Line, Rect, Stroke};
use vello::peniko::{Color, Fill};

use daw_proto::automation::{EnvelopePoint, EnvelopeRef};

use crate::arrangement::{Arrangement, Palette, Viewport};
use crate::automation::{Kind, Lane};
use crate::text::Font;

/// The room above and below a lane's curve, in screen pixels, so a point
/// at the top or the bottom is still a whole square.
const PAD: f64 = 4.0;
/// Half a point's square.
const POINT: f64 = 3.0;
/// How near a point the pointer must be to take it.
const REACH: f64 = 6.0;
/// The hide button's width at the right of a header.
const HIDE_W: f64 = 16.0;
/// The strip along an automation item's top that carries its name and
/// moves it.
const ITEM_BAR: f64 = 11.0;
/// How far from a lane's bottom edge a press resizes it.
const EDGE: f64 = 3.0;
/// How far from an automation item's side a press resizes it.
const EDGE_REACH: f64 = 4.0;
/// A leveler stage's dial in its header: its radius, and the width of the
/// strip it is turned in, left of the switch.
const KNOB_R: f64 = 8.0;
const KNOB_W: f64 = 30.0;
/// How far a dial turns per pixel dragged: its whole travel in 200.
pub const KNOB_PER_PX: f64 = crate::leveler::AMOUNT_MAX / 200.0;

/// A lane where it is on screen.
#[derive(Clone, Debug)]
pub struct Placed {
    /// The arrangement row of its track.
    pub row: usize,
    pub lane: Lane,
    /// Its top and height on screen.
    pub top: f64,
    pub height: f64,
}

impl Placed {
    /// The y of `value` in this lane.
    #[must_use]
    pub fn y_of(&self, value: f64) -> f64 {
        let span = 2.0f64.mul_add(-PAD, self.height).max(1.0);
        (1.0 - self.lane.kind.y_of(value)).mul_add(span, self.top + PAD)
    }

    /// The value at screen `y` in this lane.
    #[must_use]
    pub fn value_at_y(&self, y: f64) -> f64 {
        let span = 2.0f64.mul_add(-PAD, self.height).max(1.0);
        self.lane.kind.value_of(1.0 - (y - self.top - PAD) / span)
    }
}

/// Every lane of the rows on screen, top to bottom. `below` is the screen
/// y of content y 0 (the ruler's foot less the scroll).
#[must_use]
pub fn on_screen(
    scene: &Arrangement,
    tracks: &[daw_proto::Track],
    view: Viewport,
    below: f64,
) -> Vec<Placed> {
    let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
    let mut out = Vec::new();
    for row in scene.visible_rows(view) {
        let Some(track) = tracks.get(row) else {
            continue;
        };
        let Some((top, _)) = scene.lanes_box(row) else {
            continue;
        };
        let mut y = top.mul_add(zoom, below);
        for lane in crate::automation::lanes_of(&track.guid) {
            let height = lane.height * zoom;
            out.push(Placed {
                row,
                lane,
                top: y,
                height,
            });
            y += height;
        }
    }
    out
}

/// What a point on screen is over in a lane.
#[derive(Clone, Debug, PartialEq)]
pub enum Spot {
    /// The header's hide button.
    Hide,
    /// The header's mode, which cycles — or, in a leveler stage's lane,
    /// its switch, which turns the stage on and off.
    Mode,
    /// A leveler stage's dial, which a drag turns.
    Knob,
    /// The rest of the header.
    Header,
    /// The lane's bottom edge, which resizes it.
    Edge,
    /// An automation item's name bar, which moves it.
    ItemBar { item: usize, t: f64 },
    /// An automation item's left or right (`right`) edge, which resizes it.
    ItemEdge { item: usize, right: bool },
    /// The body: at `t` seconds and `value`, over the envelope's point
    /// `point` or, inside automation item `item`, over its curve's point
    /// `item_point`.
    Body {
        t: f64,
        value: f64,
        point: Option<usize>,
        item: Option<usize>,
        item_point: Option<usize>,
    },
}

/// A lane and what in it is under the pointer.
#[derive(Clone, Debug)]
pub struct Hit {
    pub placed: Placed,
    pub spot: Spot,
}

/// What is under `(x, y)`, if a lane is. `x` is from the window's left,
/// `y` from its top; `below` as for [`on_screen`].
#[must_use]
pub fn hit(
    scene: &Arrangement,
    tracks: &[daw_proto::Track],
    view: Viewport,
    below: f64,
    x: f64,
    y: f64,
) -> Option<Hit> {
    let placed = on_screen(scene, tracks, view, below)
        .into_iter()
        .find(|p| y >= p.top && y < p.top + p.height)?;
    let panel = view.panel_w;
    if x < panel {
        let spot = if x >= panel - HIDE_W {
            Spot::Hide
        } else if x >= panel - HIDE_W - 22.0 {
            Spot::Mode
        } else if placed.lane.kind.is_leveler() && x >= panel - HIDE_W - 22.0 - KNOB_W {
            Spot::Knob
        } else {
            Spot::Header
        };
        return Some(Hit { placed, spot });
    }
    if y >= placed.top + placed.height - EDGE {
        return Some(Hit {
            placed,
            spot: Spot::Edge,
        });
    }
    let x0 = panel - view.scroll_x;
    let t = (x - x0) / view.pps.max(1e-9);
    let x_of = |t: f64| t.mul_add(view.pps, x0);
    let lane = &placed.lane;
    if let Kind::Leveler(stage) = lane.kind {
        // The leveled item under the pointer, and (in the ride's lane, the
        // one drawn by hand) the ride's point near it.
        let item = lane.leveled.iter().position(|l| l.holds(t));
        let item_point = item
            .and_then(|i| lane.leveled.get(i))
            .filter(|_| stage == crate::leveler::Stage::Ride)
            .and_then(|l| {
                l.curves.ride.iter().position(|p| {
                    (x_of(l.at(p.t_s)) - x).abs() <= REACH && (placed.y_of(p.db) - y).abs() <= REACH
                })
            });
        let value = placed.value_at_y(y);
        return Some(Hit {
            placed,
            spot: Spot::Body {
                t,
                value,
                point: None,
                item,
                item_point,
            },
        });
    }
    // An item's edges, before its bar and its curve: a hair's width
    // either side.
    let edge = lane
        .items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, (ai, _))| {
            let start = ai.position.as_seconds();
            let end = start + ai.length.as_seconds();
            if (x_of(end) - x).abs() <= EDGE_REACH {
                Some((i, true))
            } else if (x_of(start) - x).abs() <= EDGE_REACH {
                Some((i, false))
            } else {
                None
            }
        });
    if let Some((item, right)) = edge {
        return Some(Hit {
            placed,
            spot: Spot::ItemEdge { item, right },
        });
    }
    let item = lane.item_at(t);
    if let Some(i) = item
        && y < placed.top + ITEM_BAR
    {
        return Some(Hit {
            placed,
            spot: Spot::ItemBar { item: i, t },
        });
    }
    let near = |at: f64, value: f64| {
        (x_of(at) - x).abs() <= REACH && (placed.y_of(value) - y).abs() <= REACH
    };
    let (point, item_point) = match item.and_then(|i| lane.items.get(i)) {
        Some((ai, curve)) => {
            let found = curve
                .iter()
                .enumerate()
                .find(|(_, p)| near(item_time(ai, p), scaled(ai, p.value)))
                .map(|(k, _)| k);
            (None, found)
        }
        None => (
            lane.points
                .iter()
                .enumerate()
                .find(|(_, p)| near(p.time.as_seconds(), p.value))
                .map(|(k, _)| k),
            None,
        ),
    };
    let value = placed.value_at_y(y);
    Some(Hit {
        placed,
        spot: Spot::Body {
            t,
            value,
            point,
            item,
            item_point,
        },
    })
}

/// Where an automation item's source point lands on the timeline (its
/// first lap).
#[must_use]
pub fn item_time(item: &daw_proto::automation::AutomationItem, p: &EnvelopePoint) -> f64 {
    let rate = if item.play_rate > 0.0 {
        item.play_rate
    } else {
        1.0
    };
    item.position.as_seconds() + (p.time.as_seconds() - item.start_offset.as_seconds()) / rate
}

/// A source time for timeline `t` in an item (the inverse of
/// [`item_time`]).
#[must_use]
pub fn source_time(item: &daw_proto::automation::AutomationItem, t: f64) -> f64 {
    let rate = if item.play_rate > 0.0 {
        item.play_rate
    } else {
        1.0
    };
    (t - item.position.as_seconds()).mul_add(rate, item.start_offset.as_seconds())
}

/// A source value as an item plays it.
fn scaled(item: &daw_proto::automation::AutomationItem, v: f64) -> f64 {
    if (item.amplitude - 1.0).abs() < f64::EPSILON && (item.baseline - 0.5).abs() < f64::EPSILON {
        return v;
    }
    (v - 0.5).mul_add(item.amplitude, item.baseline)
}

/// A lane's colour, by what it automates.
#[must_use]
pub const fn color(palette: &Palette, kind: Kind) -> Color {
    match kind {
        Kind::Volume => palette.meter_safe,
        Kind::Pan => palette.pan,
        Kind::Mute => palette.mute,
        Kind::Width => palette.solo,
        Kind::Param => palette.accent,
        Kind::Leveler(crate::leveler::Stage::DeEss) => palette.solo,
        Kind::Leveler(crate::leveler::Stage::DeBreath) => palette.pan,
        Kind::Leveler(crate::leveler::Stage::Ride) => palette.meter_safe,
        Kind::Leveler(crate::leveler::Stage::Gate) => palette.mute,
    }
}

/// What is under the pointer, for the drawing to light up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lit {
    /// The lane (track, envelope) and the point in it, if one.
    pub lane: Option<(String, EnvelopeRef)>,
    pub point: Option<usize>,
    pub item: Option<usize>,
    pub item_point: Option<usize>,
    /// Where across, in seconds, the pointer is.
    pub t: Option<f64>,
}

/// The lanes' bodies: drawn after the items, before the panel.
pub fn bodies(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    placed: &[Placed],
    view: Viewport,
    lit: &Lit,
) {
    let x0 = view.panel_w - view.scroll_x;
    let x_of = |t: f64| t.mul_add(view.pps, x0);
    let left = view.panel_w;
    let right = view.width;
    for p in placed {
        if p.top > view.height || p.top + p.height < 0.0 {
            continue;
        }
        let lane = &p.lane;
        let ink = color(palette, lane.kind);
        let bottom = p.top + p.height;
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.row_b.multiply_alpha(0.92),
            None,
            &Rect::new(left, p.top, right, bottom),
        );
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.divider,
            None,
            &Rect::new(left, bottom - 1.0, right, bottom),
        );
        // The neutral value, faint: where the curve rests with nothing
        // drawn (0 dB, the centre).
        let rest_y = p.y_of(crate::automation::neutral(lane.kind));
        painter.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            palette.grid.multiply_alpha(0.6),
            None,
            &Line::new((left, rest_y), (right, rest_y)),
        );
        let lit_here = lit
            .lane
            .as_ref()
            .is_some_and(|(track, target)| lane.is(track, target));
        if let Kind::Leveler(stage) = lane.kind {
            leveler(painter, palette, (p, stage), view, lit_here.then_some(lit));
            // The value under the pointer, beside it.
            if lit_here && let Some(t) = lit.t {
                let value = lane.value_at(t);
                let x = x_of(t) + 8.0;
                let y = p.y_of(value).clamp(p.top + 12.0, bottom - 3.0);
                crate::tcp::glyphs(painter, font, palette.text, &lane.say(value), x, y, 10.0);
            }
            continue;
        }
        windows(painter, palette, font, p, view, lit_here.then_some(lit));
        // The curve as it plays, every few pixels and at every point (a
        // jump is drawn as one).
        let path = curve(p, view, left, right);
        let mut under = path.clone();
        under.line_to((right, bottom - 1.0));
        under.line_to((left, bottom - 1.0));
        under.close_path();
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink.multiply_alpha(0.12),
            None,
            &under,
        );
        painter.stroke(
            &Stroke::new(if lit_here { 2.0 } else { 1.5 }),
            Affine::IDENTITY,
            ink,
            None,
            &path,
        );
        points(painter, palette, p, view, lit_here.then_some(lit));
        // The value under the pointer, beside it.
        if lit_here && let Some(t) = lit.t {
            let value = lane.value_at(t);
            let text = lane.say(value);
            let x = x_of(t) + 8.0;
            let y = p.y_of(value).clamp(p.top + 12.0, bottom - 3.0);
            crate::tcp::glyphs(painter, font, palette.text, &text, x, y, 10.0);
        }
    }
}

/// A lane's automation items: their windows and their names, under the
/// curve. `lit` when the pointer is in this lane.
fn windows(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    p: &Placed,
    view: Viewport,
    lit: Option<&Lit>,
) {
    let x0 = view.panel_w - view.scroll_x;
    let x_of = |t: f64| t.mul_add(view.pps, x0);
    let (left, right) = (view.panel_w, view.width);
    let lane = &p.lane;
    let ink = color(palette, lane.kind);
    let bottom = p.top + p.height;
    // Automation items: their windows, their names, under the curve.
    for (i, (item, _)) in lane.items.iter().enumerate() {
        let (a, b) = (
            x_of(item.position.as_seconds()),
            x_of(item.position.as_seconds() + item.length.as_seconds()),
        );
        if b < left || a > right {
            continue;
        }
        let (a, b) = (a.max(left), b.min(right));
        let lit_item = lit.is_some_and(|lit| lit.item == Some(i));
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink.multiply_alpha(if lit_item { 0.22 } else { 0.13 }),
            None,
            &Rect::new(a, p.top + 1.0, b, bottom - 1.0),
        );
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink.multiply_alpha(0.35),
            None,
            &Rect::new(a, p.top + 1.0, b, p.top + ITEM_BAR),
        );
        let label = if item.pool_id < -1 {
            "New".to_owned()
        } else if item.name.is_empty() {
            format!("Pool {}", item.pool_id)
        } else {
            item.name.clone()
        };
        let label = font.elide(&label, 9.0, (b - a - 6.0).max(0.0));
        crate::tcp::glyphs(
            painter,
            font,
            palette.text,
            &label,
            a + 3.0,
            p.top + 9.0,
            9.0,
        );
    }
}

/// A lane's points: the envelope's own (not under an item, where the item
/// plays), and the items' curves'. `lit` when the pointer is in this lane.
fn points(
    painter: &mut impl PaintScene,
    palette: &Palette,
    p: &Placed,
    view: Viewport,
    lit: Option<&Lit>,
) {
    let x0 = view.panel_w - view.scroll_x;
    let x_of = |t: f64| t.mul_add(view.pps, x0);
    let (left, right) = (view.panel_w, view.width);
    let lane = &p.lane;
    let ink = color(palette, lane.kind);
    // The envelope's own points (not under an item, where the item
    // plays), and the items' curves' points.
    for (k, point) in lane.points.iter().enumerate() {
        let t = point.time.as_seconds();
        if lane.item_at(t).is_some() {
            continue;
        }
        let x = x_of(t);
        if x < left - POINT || x > right + POINT {
            continue;
        }
        let lit_point = lit.is_some_and(|lit| lit.point == Some(k));
        square(painter, (x, p.y_of(point.value)), lit_point, ink, palette);
    }
    for (i, (item, curve)) in lane.items.iter().enumerate() {
        let end = item.position.as_seconds() + item.length.as_seconds();
        for (k, point) in curve.iter().enumerate() {
            let t = item_time(item, point);
            if t < item.position.as_seconds() || t > end {
                continue;
            }
            let x = x_of(t);
            if x < left - POINT || x > right + POINT {
                continue;
            }
            let lit_point = lit.is_some_and(|lit| lit.item == Some(i) && lit.item_point == Some(k));
            let r = if lit_point { POINT + 1.5 } else { POINT };
            let centre = (x, p.y_of(scaled(item, point.value)));
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                if lit_point { palette.text } else { ink },
                None,
                &Circle::new(centre, r),
            );
        }
    }
}

/// A vocal leveler's stage lane: under each leveled item, the stage's
/// curve as it plays (through its dial), filled to 0 dB so what it takes
/// away or adds is a shape; in the ride's lane its points, which a hand
/// moves. A stage switched off is drawn faint and flat. `lit` when the
/// pointer is in this lane.
fn leveler(
    painter: &mut impl PaintScene,
    palette: &Palette,
    (placed, stage): (&Placed, crate::leveler::Stage),
    view: Viewport,
    lit: Option<&Lit>,
) {
    const STEP: f64 = 2.0;
    let x0 = view.panel_w - view.scroll_x;
    let pps = view.pps.max(1e-9);
    let (left, right) = (view.panel_w, view.width);
    let bottom = placed.top + placed.height;
    let ink = color(palette, placed.lane.kind);
    let zero = placed.y_of(0.0);
    for (i, item) in placed.lane.leveled.iter().enumerate() {
        let (from_x, to_x) = (
            item.position.mul_add(pps, x0),
            (item.position + item.length).mul_add(pps, x0),
        );
        if to_x < left || from_x > right {
            continue;
        }
        let (from_x, to_x) = (from_x.max(left), to_x.min(right));
        let lit_item = lit.is_some_and(|lit| lit.item == Some(i));
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette
                .text
                .multiply_alpha(if lit_item { 0.07 } else { 0.035 }),
            None,
            &Rect::new(from_x, placed.top + 1.0, to_x, bottom - 1.0),
        );
        let on = item.stages.get(stage).on;
        let points = item.curves.of(stage);
        let columns = crate::num::index(((to_x - from_x) / STEP).ceil());
        let width = (to_x - from_x) / crate::num::coord(columns.max(1));
        let span = (
            item.lane_time((from_x - x0) / pps),
            item.lane_time((to_x - x0) / pps),
        );
        let mut path = BezPath::new();
        for (k, (column, db)) in crate::leveler::trace(points, span, columns)
            .into_iter()
            .enumerate()
        {
            let at = (column.mul_add(width, from_x), placed.y_of(db));
            if k == 0 {
                path.move_to(at);
            } else {
                path.line_to(at);
            }
        }
        let mut under = path.clone();
        under.line_to((to_x, zero));
        under.line_to((from_x, zero));
        under.close_path();
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink.multiply_alpha(if on { 0.18 } else { 0.05 }),
            None,
            &under,
        );
        painter.stroke(
            &Stroke::new(if lit_item { 1.8 } else { 1.4 }),
            Affine::IDENTITY,
            ink.multiply_alpha(if on { 0.95 } else { 0.35 }),
            None,
            &path,
        );
        if stage != crate::leveler::Stage::Ride || !on {
            continue;
        }
        // The ride's points, where there is room between them to take
        // one (or the pointer is on the item): what a hand moves.
        let xs: Vec<f64> = points
            .iter()
            .map(|p| item.at(p.t_s).mul_add(pps, x0))
            .collect();
        let spaced = xs.windows(2).all(|w| match w {
            [a, b] => b - a >= 2.0 * POINT,
            _ => true,
        });
        if !spaced && !lit_item {
            continue;
        }
        for (k, (point, x)) in points.iter().zip(&xs).enumerate() {
            if *x < from_x - POINT || *x > to_x + POINT {
                continue;
            }
            let lit_point = lit_item && lit.is_some_and(|lit| lit.item_point == Some(k));
            square(
                painter,
                (*x, placed.y_of(point.db)),
                lit_point,
                ink,
                palette,
            );
        }
    }
}

/// The vocal leveler's sum on each leveled item on screen, over its waveform.
///
/// A line across the item at the gain it plays at — 0 dB
/// across its middle, ±24 dB at its top and foot — the way the item's
/// take volume envelope plays it, and the waveform is drawn through it.
pub fn leveler_lines(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    below: f64,
) {
    const STEP: f64 = 2.0;
    if !crate::leveler::any() {
        return;
    }
    let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
    let pps = view.pps.max(1e-9);
    let x0 = view.panel_w - view.scroll_x;
    for item in scene.item_boxes() {
        let (a, b) = (item.x0.mul_add(pps, x0), item.x1.mul_add(pps, x0));
        if b < view.panel_w || a > view.width {
            continue;
        }
        let Some((top, height)) = scene.item_box(item) else {
            continue;
        };
        let (top, height) = (top.mul_add(zoom, below), height * zoom);
        if top > view.height || top + height < 0.0 || height < 8.0 {
            continue;
        }
        let (left, right) = (a.max(view.panel_w), b.min(view.width));
        let span = 2.0f64.mul_add(-PAD, height).max(1.0);
        let y_of = |db: f64| (1.0 - crate::leveler::item_y(db)).mul_add(span, top + PAD);
        let drawn = crate::leveler::with_item(&item.guid, |l| {
            let columns = crate::num::index(((right - left) / STEP).ceil());
            let width = (right - left) / crate::num::coord(columns.max(1));
            let times = (
                l.lane_time((left - x0) / pps),
                l.lane_time((right - x0) / pps),
            );
            let mut path = BezPath::new();
            for (k, (column, db)) in crate::leveler::trace(&l.composite, times, columns)
                .into_iter()
                .enumerate()
            {
                let at = (column.mul_add(width, left), y_of(db));
                if k == 0 {
                    path.move_to(at);
                } else {
                    path.line_to(at);
                }
            }
            path
        });
        let Some(path) = drawn else {
            continue;
        };
        // 0 dB, faint: where the leveler leaves the vocal as sung.
        painter.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            palette.text.multiply_alpha(0.18),
            None,
            &Line::new((left, y_of(0.0)), (right, y_of(0.0))),
        );
        painter.stroke(
            &Stroke::new(2.4),
            Affine::IDENTITY,
            palette.surface.multiply_alpha(0.6),
            None,
            &path,
        );
        painter.stroke(
            &Stroke::new(1.3),
            Affine::IDENTITY,
            palette.text.multiply_alpha(0.9),
            None,
            &path,
        );
    }
}

/// A point's square.
fn square(
    painter: &mut impl PaintScene,
    (x, y): (f64, f64),
    lit: bool,
    ink: Color,
    palette: &Palette,
) {
    let r = if lit { POINT + 1.5 } else { POINT };
    let shape = Rect::new(x - r, y - r, x + r, y + r);
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        if lit { palette.text } else { ink },
        None,
        &shape,
    );
    painter.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        palette.surface,
        None,
        &shape,
    );
}

/// A lane's curve across `left..right`: sampled every few pixels, with a
/// vertex at each point it plays through, so a step is a step.
fn curve(p: &Placed, view: Viewport, left: f64, right: f64) -> BezPath {
    const STEP: f64 = 3.0;
    let x0 = view.panel_w - view.scroll_x;
    let pps = view.pps.max(1e-9);
    let t_of = |x: f64| (x - x0) / pps;
    let lane = &p.lane;
    let (from, to) = (t_of(left), t_of(right));
    let steps = num_traits::ToPrimitive::to_u32(&((right - left) / STEP).ceil()).unwrap_or(0);
    let mut times: Vec<f64> = (0..=steps)
        .map(|k| t_of(f64::from(k).mul_add(STEP, left).min(right)))
        .collect();
    times.push(to);
    for point in &lane.played {
        let t = point.time.as_seconds();
        if t > from && t < to {
            // Just before and at: a jump draws straight up.
            times.push(t - 1e-6);
            times.push(t);
        }
    }
    times.sort_by(f64::total_cmp);
    let mut path = BezPath::new();
    for (i, t) in times.iter().enumerate() {
        let at = (t.mul_add(pps, x0), p.y_of(lane.value_at(*t)));
        if i == 0 {
            path.move_to(at);
        } else {
            path.line_to(at);
        }
    }
    path
}

/// The lanes' headers, in the track panel's column: drawn after the panel.
pub fn headers(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    placed: &[Placed],
    view: Viewport,
    lit: &Lit,
    at: f64,
) {
    let width = view.panel_w;
    for p in placed {
        if p.top > view.height || p.top + p.height < 0.0 {
            continue;
        }
        let lane = &p.lane;
        let ink = color(palette, lane.kind);
        let bottom = p.top + p.height;
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.tcp_gutter,
            None,
            &Rect::new(0.0, p.top, width, bottom),
        );
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.tcp_rule,
            None,
            &Rect::new(0.0, bottom - 1.0, width, bottom),
        );
        // The rule in the lane's colour, down its left, after the track's
        // indent: whose lane it is, at a glance.
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            ink,
            None,
            &Rect::new(14.0, p.top + 3.0, 17.0, bottom - 4.0),
        );
        let lit_here = lit
            .lane
            .as_ref()
            .is_some_and(|(track, target)| lane.is(track, target));
        // A leveler stage's dial, if this is one's lane.
        let dial = match lane.kind {
            Kind::Leveler(stage) => lane.leveled.first().map(|l| l.stages.get(stage)),
            _ => None,
        };
        let knob_room = if lane.kind.is_leveler() { KNOB_W } else { 0.0 };
        let room = (width - 24.0 - HIDE_W - 26.0 - knob_room).max(0.0);
        let name = font.elide(&lane.name, 10.5, room);
        let name_ink = if lit_here {
            palette.text
        } else {
            palette.text_dim
        };
        crate::tcp::glyphs(painter, font, name_ink, &name, 24.0, p.top + 14.0, 10.5);
        if p.height >= 30.0 {
            let value = lane.say(lane.value_at(lit.t.filter(|_| lit_here).unwrap_or(at)));
            crate::tcp::glyphs(
                painter,
                font,
                palette.text_faint,
                &value,
                24.0,
                p.top + 28.0,
                10.0,
            );
        }
        // Its mode, which a click cycles — a leveler stage's switch in its
        // place (its lanes have no mode: they are the items' volume) —
        // and the hide button.
        let mode = match (lane.kind, dial) {
            (Kind::Leveler(_), Some(d)) if d.on => "On",
            (Kind::Leveler(_), _) => "Off",
            _ => mode_letters(lane.mode),
        };
        let mx = width - HIDE_W - 22.0;
        if let Some(d) = dial {
            // The dial: how far the stage goes, 0 to twice as made, and
            // under it the amount.
            let centre = (mx - KNOB_W / 2.0 - 1.0, p.top + 5.0 + KNOB_R);
            crate::tone::knob_arc(
                painter,
                palette,
                centre,
                KNOB_R,
                d.amount / crate::leveler::AMOUNT_MAX,
                if d.on { ink } else { palette.text_faint },
                lit_here,
            );
            if p.height >= 2.0f64.mul_add(KNOB_R, 20.0) {
                let text = d.text();
                let w = font.width(&text, 8.5);
                crate::tcp::glyphs(
                    painter,
                    font,
                    palette.text_faint,
                    &text,
                    centre.0 - w / 2.0,
                    2.0f64.mul_add(KNOB_R, p.top + 15.0),
                    8.5,
                );
            }
        }
        if !mode.is_empty() {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.tcp_field,
                None,
                &Rect::new(mx, p.top + 4.0, mx + 20.0, p.top + 17.0),
            );
        }
        let w = font.width(mode, 9.0);
        crate::tcp::glyphs(
            painter,
            font,
            palette.text_dim,
            mode,
            mx + (20.0 - w) / 2.0,
            p.top + 14.0,
            9.0,
        );
        let hx = width - HIDE_W + 4.0;
        let cross = |a: (f64, f64), b: (f64, f64)| Line::new(a, b);
        for line in [
            cross((hx, p.top + 6.0), (hx + 7.0, p.top + 13.0)),
            cross((hx + 7.0, p.top + 6.0), (hx, p.top + 13.0)),
        ] {
            painter.stroke(
                &Stroke::new(1.2),
                Affine::IDENTITY,
                palette.text_faint,
                None,
                &line,
            );
        }
    }
}

/// A mode as two letters for the header.
#[must_use]
pub const fn mode_letters(mode: daw_proto::primitives::AutomationMode) -> &'static str {
    use daw_proto::primitives::AutomationMode as M;
    match mode {
        M::Off => "off",
        M::TrimRead => "Trm",
        M::Read => "Rd",
        M::Touch => "Tch",
        M::Write => "Wr",
        M::Latch => "Lt",
        M::LatchPreview => "LP",
    }
}

/// The mode a click on a lane's mode moves to: read, off, trim/read —
/// what is chosen while editing by hand (the writing modes are a fader's).
#[must_use]
pub const fn next_mode(
    mode: daw_proto::primitives::AutomationMode,
) -> daw_proto::primitives::AutomationMode {
    use daw_proto::primitives::AutomationMode as M;
    match mode {
        M::TrimRead => M::Read,
        M::Read => M::Off,
        _ => M::TrimRead,
    }
}

/// An item's take volume curve on screen: the item's index, its box (left,
/// top, right, height) and its curve.
/// An item on screen with a take volume curve: its index, its box (left,
/// top, right, height) and the curve.
type TakeBox = (usize, (f64, f64, f64, f64), crate::automation::TakeCurve);

fn take_boxes(scene: &Arrangement, view: Viewport, below: f64) -> Vec<TakeBox> {
    if !crate::automation::any_take_curves() {
        return Vec::new();
    }
    let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
    let x0 = view.panel_w - view.scroll_x;
    scene
        .item_boxes()
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let (a, b) = (item.x0.mul_add(view.pps, x0), item.x1.mul_add(view.pps, x0));
            if b < view.panel_w || a > view.width {
                return None;
            }
            let (top, height) = scene.item_box(item)?;
            let (top, height) = (top.mul_add(zoom, below), height * zoom);
            if top > view.height || top + height < 0.0 || height < 8.0 {
                return None;
            }
            // A leveled vocal's volume is drawn as the leveler's sum, on
            // its own scale: one line for one curve.
            if crate::leveler::with_item(&item.guid, |_| ()).is_some() {
                return None;
            }
            let curve = crate::automation::take_curve(&item.guid)?;
            Some((i, (a, top, b, height), curve))
        })
        .collect()
}

/// The y of take gain `gain` in an item `top`, `height` tall.
fn take_y(top: f64, height: f64, gain: f64) -> f64 {
    let span = 2.0f64.mul_add(-PAD, height).max(1.0);
    (1.0 - crate::automation::take_y(gain)).mul_add(span, top + PAD)
}

/// The take volume curves of the items on screen, over them: the curve as
/// the item plays it and its points. `lit`: the item and point under the
/// pointer.
pub fn take_lines(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    below: f64,
    lit: Option<(usize, Option<usize>)>,
) {
    const STEP: f64 = 3.0;
    for (i, (a, top, b, height), curve) in take_boxes(scene, view, below) {
        let start = scene.item(i).map_or(0.0, |item| item.x0);
        let (left, right) = (a.max(view.panel_w), b.min(view.width));
        let steps = num_traits::ToPrimitive::to_u32(&((right - left) / STEP).ceil()).unwrap_or(0);
        let mut path = BezPath::new();
        for k in 0..=steps {
            let x = f64::from(k).mul_add(STEP, left).min(right);
            let t = (x - a) / view.pps.max(1e-9);
            let gain = daw_proto::automation::value_at(&curve.points, t).unwrap_or(1.0);
            let at = (x, take_y(top, height, gain));
            if k == 0 {
                path.move_to(at);
            } else {
                path.line_to(at);
            }
        }
        let lit_item = lit.is_some_and(|(at, _)| at == i);
        painter.stroke(
            &Stroke::new(if lit_item { 1.8 } else { 1.2 }),
            Affine::IDENTITY,
            palette.text.multiply_alpha(0.85),
            None,
            &path,
        );
        // Points only where there is room between them to take one.
        let x_of = |t: f64| (start + t).mul_add(view.pps, view.panel_w - view.scroll_x);
        let spaced = curve.points.windows(2).all(|w| match w {
            [p, q] => x_of(q.time.as_seconds()) - x_of(p.time.as_seconds()) >= 2.0 * POINT,
            _ => true,
        });
        if !spaced && !lit_item {
            continue;
        }
        for (k, point) in curve.points.iter().enumerate() {
            let x = x_of(point.time.as_seconds());
            if x < left - POINT || x > right + POINT {
                continue;
            }
            let lit_point = lit.is_some_and(|(at, p)| at == i && p == Some(k));
            square(
                painter,
                (x, take_y(top, height, point.value)),
                lit_point,
                palette.text,
                palette,
            );
        }
    }
}

/// What a press at `(x, y)` is on in an item's take volume curve.
#[derive(Clone, Debug)]
pub struct TakeHit {
    /// The item's index in the arrangement, and its guid.
    pub index: usize,
    pub item: String,
    /// The curve's point under the pointer, if one.
    pub point: Option<usize>,
    /// Seconds into the item, and the gain at the pointer's height.
    pub t: f64,
    pub gain: f64,
    /// Where the item is on screen: its left, top and height.
    pub left: f64,
    pub top: f64,
    pub height: f64,
}

/// The take volume curve under `(x, y)`: on one of its points, or near its
/// line — anywhere else in the item is the item's.
#[must_use]
pub fn take_hit(
    scene: &Arrangement,
    view: Viewport,
    below: f64,
    x: f64,
    y: f64,
) -> Option<TakeHit> {
    take_boxes(scene, view, below)
        .into_iter()
        .rev()
        .find_map(|(i, (a, top, b, height), curve)| {
            if x < a || x > b || y < top || y > top + height {
                return None;
            }
            let t = (x - a) / view.pps.max(1e-9);
            let point = curve.points.iter().position(|p| {
                (p.time.as_seconds().mul_add(view.pps, a) - x).abs() <= REACH
                    && (take_y(top, height, p.value) - y).abs() <= REACH
            });
            let on_line = daw_proto::automation::value_at(&curve.points, t)
                .is_some_and(|g| (take_y(top, height, g) - y).abs() <= REACH);
            if point.is_none() && !on_line {
                return None;
            }
            let span = 2.0f64.mul_add(-PAD, height).max(1.0);
            let gain = crate::automation::take_gain(1.0 - (y - top - PAD) / span);
            Some(TakeHit {
                index: i,
                item: scene.item(i)?.guid.clone(),
                point,
                t,
                gain,
                left: a,
                top,
                height,
            })
        })
}

/// The gain at screen `y` in an item at `top`, `height` tall.
#[must_use]
pub fn take_gain_at(top: f64, height: f64, y: f64) -> f64 {
    let span = 2.0f64.mul_add(-PAD, height).max(1.0);
    crate::automation::take_gain(1.0 - (y - top - PAD) / span)
}
