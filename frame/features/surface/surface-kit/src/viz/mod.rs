//! Kit family: visualizers — graphs that draw what an engine is doing and
//! let you grab it: a spectrum, a gain-reduction history, a compressor's
//! transfer curve and history, an EQ graph with a handle per band, an ADSR
//! envelope, a scope.
//!
//! Each is ordinary frame nodes: a panel, a plot area with a soft grid and
//! printed labels (text nodes in the scene), and inside it `Plot` controls
//! (a stream's latest frame), `Curve` controls (a function of params, from
//! the runtime's curve registry) and `Handle` controls (draggable points
//! bound to params). Traces are stroked vectors that give off light;
//! regions under them are gradient-filled vectors — restyle any of it.
//!
//! The multi-param visualizers find their params by a namespace
//! (`Place::options` `ns=<ns>`, default the place's name lowercased) and a
//! fixed pattern, which [`catalog`] also builds:
//!
//! | component | params | streams |
//! |---|---|---|
//! | `viz.eq_graph` | `<ns>/b{n}_{freq,gain,q,shape,on}` | `<ns>/spectrum` (or the binding / `spectrum=`) |
//! | `viz.comp` | `<ns>/{threshold,ratio,knee,makeup}` | `<ns>/in` (level), `<ns>/in_history`, `<ns>/gr_history` (histories, or any reading) |
//! | `viz.adsr` | `<ns>/{attack,decay,sustain,release}` | `<ns>/voices` (an `Envelope` feed: playheads; optional) |
//!
//! `Place::size` is the width; `h=` sets the height.

pub mod catalog;
mod comp;
/// `viz.gate`.
mod gate;
/// `viz.adsr`.
mod eq;
/// `viz.eq_graph`.
mod eq_graph;
/// `viz.spectrum`, `viz.gr_history`, `viz.scope`.
mod simple;
/// The lit visualizers: processor's panel shaders with their vector
/// fallbacks (`viz.comp_scope`, `viz.delay_taps`, `viz.reverb_space`,
/// `viz.mod_lfo`, `viz.eq_glow`).
pub mod lit;
pub mod lit_catalog;

use document_proto::{Node, NodeId, NodeKind, Stroke, StrokeAlign, StrokeCap, StrokeJoin, TextAlign, TextData};
use surface_widgets::Builder;

use crate::controls::parts::alpha;
use crate::draw;
use crate::registry::{ComponentDef, Place};

/// Panel and grid ink.
pub(crate) const PANEL: &str = "#101319";
pub(crate) const RIM: &str = "#232a35";
pub(crate) const GRID: &str = "#ffffff0f";
pub(crate) const GRID_MAJOR: &str = "#ffffff21";
pub(crate) const LABEL: &str = "#8e98a8";
/// Label size, px.
pub(crate) const LABEL_PX: f64 = 9.0;

/// One colour per EQ band / envelope stage, round the wheel.
pub const BAND_COLORS: [&str; 8] = ["#ff6b6b", "#ffa94d", "#ffd43b", "#69db7c", "#3bc9db", "#4dabf7", "#9775fa", "#f783ac"];

pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut all = simple::defs();
    all.extend(comp::defs());
    all.extend(gate::defs());
    all.extend(eq_graph::defs());
    all.extend(eq::defs());
    all.extend(lit::defs());
    all
}

/// The namespace a multi-param visualizer binds under.
pub(crate) fn ns(p: &Place) -> String {
    p.opt("ns").map_or_else(|| p.name.to_lowercase().replace(' ', "_"), ToString::to_string)
}

pub(crate) fn opt_f(p: &Place, key: &str, default: f64) -> f64 {
    p.opt(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// A plot area: where its traces are drawn and its handles move.
pub(crate) struct Area {
    /// Holds the grid and the handles (unclipped, so a handle at an edge
    /// shows whole).
    pub area: NodeId,
    /// Holds traces and regions, clipped to the area.
    pub clip: NodeId,
    pub w: f64,
    pub h: f64,
}

/// The panel every visualizer sits on: a dark, softly lit plate.
pub(crate) fn panel(b: &mut Builder<'_>, parent: NodeId, name: &str, w: f64, h: f64) -> NodeId {
    let mut n = draw::frame(name, 0.0, 0.0, w, h);
    n.fills = vec![draw::solid(PANEL)];
    let n = draw::rounded(draw::stroked(n, RIM, 1.0), 10.0);
    b.add(parent, draw::with_effects(n, vec![draw::drop_shadow("#00000073", 0.0, 6.0, 18.0), draw::inner_shadow("#ffffff0a", 0.0, 1.0, 0.0)]))
}

/// A plot area at (`x`, `y`) of `w × h` in `panel`: a slightly lifted,
/// vignetted well.
pub(crate) fn area(b: &mut Builder<'_>, panel: NodeId, x: f64, y: f64, w: f64, h: f64) -> Area {
    let mut well = draw::frame("well", x, y, w, h);
    well.fills = vec![
        draw::linear(180.0, &[(0.0, "#161b24"), (1.0, "#0c0f14")]),
        draw::radial_at(0.5, 0.35, 0.8, &[(0.0, "#2a3448"), (1.0, "#2a344800")]),
    ];
    let well = draw::with_effects(draw::rounded(draw::stroked(well, "#ffffff0d", 1.0), 5.0), vec![draw::inner_shadow("#000000b3", 0.0, 2.0, 10.0)]);
    b.add(panel, well);
    let area = b.add(panel, draw::frame("plot area", x, y, w, h));
    let mut clip = draw::frame("traces", 0.0, 0.0, w, h);
    clip.clips_content = true;
    let clip = b.add(area, clip);
    Area { area, clip, w, h }
}

/// Grid lines at unit positions across (`xs`) and up (`ys`) the area;
/// `true` = a major line.
pub(crate) fn grid(b: &mut Builder<'_>, a: &Area, xs: &[(f64, bool)], ys: &[(f64, bool)]) {
    for major in [false, true] {
        let mut d = String::new();
        for (u, _) in xs.iter().filter(|(_, m)| *m == major) {
            let x = (u * a.w).round() + 0.5;
            d.push_str(&format!("M {x:.1} 0 V {:.1} ", a.h));
        }
        for (v, _) in ys.iter().filter(|(_, m)| *m == major) {
            let y = ((1.0 - v) * a.h).round() + 0.5;
            d.push_str(&format!("M 0 {y:.1} H {:.1} ", a.w));
        }
        if d.is_empty() {
            continue;
        }
        let mut n = draw::path(if major { "grid major" } else { "grid" }, 0.0, 0.0, a.w, a.h, d.trim_end().to_string(), draw::solid("#00000000"));
        n.fills.clear();
        n.stroke = Stroke { paints: vec![draw::solid(if major { GRID_MAJOR } else { GRID })], weight: 1.0, align: StrokeAlign::Center, ..Stroke::default() };
        b.add(a.area, n);
    }
}

/// A printed label, `size` px, its box centred on (`cx`, `cy`) — or set
/// against `cx` on the given side.
pub(crate) fn label(b: &mut Builder<'_>, parent: NodeId, s: &str, (cx, cy): (f64, f64), align: TextAlign, color: &str) -> NodeId {
    let w = draw::text_width(s, LABEL_PX) + 8.0;
    let x = match align {
        TextAlign::Right => cx - w,
        TextAlign::Left => cx,
        _ => cx - w / 2.0,
    };
    let mut n = draw::text("label", s, x, cy - LABEL_PX * 0.68, w, LABEL_PX, color, align);
    if let NodeKind::Text(TextData { font_weight, letter_spacing, .. }) = &mut n.kind {
        *font_weight = Some(500.0);
        *letter_spacing = 0.2;
    }
    b.add(parent, n)
}

/// A trace's look: its line, the region under it, and its light.
#[derive(Clone, Copy)]
pub(crate) struct Look<'a> {
    pub line: Option<(&'a str, f64)>,
    /// Region fill, top → bottom of the area.
    pub fill: Option<(&'a str, &'a str)>,
    /// Light the line gives off: colour, intensity, radius.
    pub glow: Option<(&'a str, f64, f64)>,
    pub dash: bool,
}

/// A trace frame the size of the area holding the region (`Area` slot)
/// and the line (`Trace` slot) vectors, in that paint order. Returns the
/// frame and the slots it drew.
pub(crate) fn trace(b: &mut Builder<'_>, a: &Area, name: &str, look: Look<'_>) -> (NodeId, Vec<(surface_proto::SlotRole, NodeId)>) {
    let frame = b.add(a.clip, draw::frame(name, 0.0, 0.0, a.w, a.h));
    let mut slots = Vec::new();
    if let Some((top, bottom)) = look.fill {
        let mut n = Node::new(NodeKind::Vector, "fill");
        n.size = document_proto::Size { width: a.w, height: a.h };
        n.fills = vec![draw::linear(180.0, &[(0.0, top), (1.0, bottom)])];
        slots.push((surface_proto::SlotRole::Area, b.add(frame, n)));
    }
    if let Some((color, weight)) = look.line {
        let mut n = Node::new(NodeKind::Vector, "line");
        n.size = document_proto::Size { width: a.w, height: a.h };
        n.stroke = Stroke {
            paints: vec![draw::solid(color)],
            weight,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            align: StrokeAlign::Center,
            dash_pattern: if look.dash { vec![3.0, 3.0] } else { Vec::new() },
            ..Stroke::default()
        };
        if let Some((c, i, r)) = look.glow {
            n = draw::emissive(n, draw::hex(c), i, r);
        }
        slots.push((surface_proto::SlotRole::Trace, b.add(frame, n)));
    }
    (frame, slots)
}

/// A glowing dot of diameter `d` centred in a `box_px` square frame at
/// the area's centre (a handle or a marker moves the frame): a solid
/// `color` disc with a light rim, `text` printed on it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dot(b: &mut Builder<'_>, parent: NodeId, name: &str, box_px: f64, d: f64, color: &str, text: &str, area: &Area) -> NodeId {
    let root = b.add(parent, draw::frame(name, (area.w - box_px) / 2.0, (area.h - box_px) / 2.0, box_px, box_px));
    let o = (box_px - d) / 2.0;
    b.add(root, draw::circle("halo", box_px / 2.0, box_px / 2.0, d * 0.95, draw::radial(&[(0.0, &alpha(color, 0.35)), (1.0, &alpha(color, 0.0))])));
    let mut disc = draw::frame("disc", o, o, d, d);
    disc.fills = vec![draw::solid(color)];
    let disc = draw::emissive(draw::rounded(draw::stroked(disc, "#ffffffd9", 1.5), d / 2.0), draw::hex(color), 0.7, d * 0.7);
    let disc = b.add(root, draw::with_effects(disc, vec![draw::drop_shadow("#00000099", 0.0, 1.0, 4.0)]));
    if !text.is_empty() {
        let mut t = draw::text("number", text, 0.0, d / 2.0 - d * 0.36, d, (d * 0.56).round(), "#0b0d10", TextAlign::Center);
        if let NodeKind::Text(TextData { font_weight, .. }) = &mut t.kind {
            *font_weight = Some(700.0);
        }
        b.add(disc, t);
    }
    root
}
