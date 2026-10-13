//! Panel-building helpers shared by the VCA-compressor and EQ faces: placing
//! kit components by the centre of their shaft, silkscreen, a panel lamp,
//! readout windows, filter-shape glyphs, and the catalog entries the faces
//! bind.
//!
//! Everything is plain frame nodes plus `surface_proto::Control`s, the same
//! as the kit; these only save each face from repeating the arithmetic.

use document_proto::{Node, NodeId, NodeKind, StrokeAlign, TextAlign, TextCase, Transform};
use surface_kit::{Place, draw};
use surface_proto::{Binding, Curve, CurvePoint, EntryId, ParamInfo, Range, SlotRole, StreamInfo, StreamKind, Unit};
use surface_widgets::Builder;

// ── placing ──────────────────────────────────────────────────────────────

/// Move `id` so its top-left corner is at (`x`, `y`) in its parent.
pub fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

/// The point a component is placed by, in its own box: the centre of its
/// shaft for knobs and rotary switches (`knob` / `dial` child), otherwise
/// the centre of the box.
fn anchor(b: &Builder<'_>, root: NodeId) -> (f64, f64) {
    let Some(n) = b.scene.node(root) else { return (0.0, 0.0) };
    for c in &n.children {
        if let Some(k) = b.scene.node(*c)
            && matches!(k.name.as_str(), "knob" | "dial")
        {
            return (k.transform.tx + k.size.width / 2.0, k.transform.ty + k.size.height / 2.0);
        }
    }
    (n.size.width / 2.0, n.size.height / 2.0)
}

/// Place kit component `id` under `parent` with its shaft (or centre) at
/// (`cx`, `cy`).
///
/// # Panics
/// On an unknown component id or variant — faces are literals.
pub fn put(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, cx: f64, cy: f64) -> NodeId {
    let root = surface_kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"));
    let (ax, ay) = anchor(b, root);
    move_to(b, root, cx - ax, cy - ay);
    root
}

/// Place kit component `id` with its box's top-left at (`x`, `y`).
///
/// # Panics
/// On an unknown component id or variant.
pub fn put_at(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, x: f64, y: f64) -> NodeId {
    let root = surface_kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"));
    move_to(b, root, x, y);
    root
}

/// A kit panel of `w × h` in `variant`, at the origin of `parent`.
///
/// # Panics
/// On an unknown panel id or variant.
pub fn panel(b: &mut Builder<'_>, parent: NodeId, id: &str, variant: &str, w: f64, h: f64, ends: &str) -> NodeId {
    let opts = [format!("w={w}"), format!("h={h}"), format!("ends={ends}")];
    let p = Place { options: Some(opts.to_vec()), ..Place::new("Faceplate").variant(variant) };
    put_at(b, parent, id, &p, 0.0, 0.0)
}

/// Size the canvas's root frame to its panel, so the face renders at the
/// panel's size.
pub fn fit_root(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64) {
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size { width: w, height: h };
    }
}

/// Take a placed control's legend (and its `Label` slot) off: the face
/// prints the name itself, somewhere the kit doesn't (a header above a
/// dial, over a band's column). An empty legend isn't enough — the runtime
/// prints the param's name into an empty `Label`.
pub fn strip_legend(b: &mut Builder<'_>, control: NodeId) {
    let ids: Vec<NodeId> = b.controls.get(&control).map(|c| c.slots_of(SlotRole::Label).map(NodeId).collect()).unwrap_or_default();
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
    }
    for id in ids {
        let _ = b.scene.apply(document_proto::Op::Remove { id });
    }
}

/// Labels set round a shaft at (`cx`, `cy`) over `sweep`, each pushed out
/// past radius `r` by its own width, so long ones (`100mV`) clear the
/// knob's ticks at the sides. The knob prints the ticks (a blank scale of
/// as many positions).
#[allow(clippy::too_many_arguments)]
pub fn scale_labels(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, r: f64, labels: &[&str], sweep: f64, size: f64, color: &str) {
    let n = labels.len();
    for (i, l) in labels.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
        let a = if n < 2 { 0.0 } else { -sweep / 2.0 + sweep * i as f64 / (n - 1) as f64 };
        let w = l.chars().count() as f64 * size * 0.6;
        let push = 0.5 * (w * a.to_radians().sin().abs() + size * a.to_radians().cos().abs());
        let (lx, ly) = draw::polar(cx, cy, r + push, a);
        b.add(parent, text("scale label", l, lx - w, ly, 2.0 * w, size, color, TextAlign::Center, 600.0, false));
    }
}

/// A filter-shape glyph (what a Neve prints over a band instead of its
/// name): a stroked curve `w` wide centred on (`cx`, `cy`).
pub fn glyph(b: &mut Builder<'_>, parent: NodeId, shape: Glyph, cx: f64, cy: f64, w: f64, color: &str) {
    let h = w * 0.42;
    let (x0, y0) = (cx - w / 2.0, cy - h / 2.0);
    let ease = |t: f64| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // The response, y (0 = top) against x, both 0..1.
    let curve = |u: f64| match shape {
        Glyph::HighShelf => 0.8 - 0.6 * ease((u - 0.3) / 0.4),
        Glyph::LowShelf => 0.2 + 0.6 * ease((u - 0.3) / 0.4),
        Glyph::Bell => 0.82 - 0.7 * (-((u - 0.5) / 0.13).powi(2)).exp(),
        Glyph::HighPass => 0.2 + 0.8 * (1.0 - (u / 0.42).min(1.0)).powi(2),
    };
    let n = 48;
    let mut d = String::new();
    for i in 0..=n {
        let u = f64::from(i) / f64::from(n);
        d.push_str(&format!("{} {:.3} {:.3} ", if i == 0 { "M" } else { "L" }, u * w, curve(u) * h));
    }
    let mut node = draw::stroked(draw::path("glyph", x0, y0, w, h, d.trim_end().to_string(), draw::solid("#00000000")), color, (w * 0.06).max(1.4));
    node.fills.clear();
    node.stroke.align = StrokeAlign::Center;
    b.add(parent, node);
}

/// Which filter shape [`glyph`] draws.
#[derive(Clone, Copy)]
pub enum Glyph {
    HighShelf,
    LowShelf,
    Bell,
    HighPass,
}

/// A range whose evenly spaced printed marks are exactly `values` (a dial
/// printed unevenly — a console's frequency pots, a ratio scale): the
/// control position follows the print, piecewise between marks.
///
/// # Panics
/// With fewer than two values.
#[must_use]
pub fn printed(values: &[f64], unit: Unit) -> Range {
    assert!(values.len() >= 2, "a printed scale needs two marks");
    let last = (values.len() - 1) as f64;
    let (lo, hi) = (values[0], values[values.len() - 1]);
    let points = values.iter().enumerate().map(|(i, v)| CurvePoint { x: i as f64 / last, y: (v - lo) / (hi - lo) }).collect();
    Range { min: lo, max: hi, scale: surface_proto::Scale::Curve { curve: Curve::Points { points } }, unit, steps: None }
}

/// Add a live value readout (`Value` slot) under a placed knob's legend —
/// or, with none, under everything it draws (its printed scale too).
pub fn add_value(b: &mut Builder<'_>, control: NodeId, size: f64, color: &str) -> NodeId {
    let w = b.scene.node(control).map_or(0.0, |n| n.size.width);
    let label = b.controls.get(&control).and_then(|c| c.slots_of(SlotRole::Label).next());
    let below = label.and_then(|l| b.scene.node(NodeId(l))).map_or_else(
        || {
            let (_, y, _, h) = surface_kit::drawn_bounds(b.scene, control);
            y + h
        },
        |n| n.transform.ty + n.size.height,
    );
    let mut t = text("value", "", -w, below + size * 0.72, 3.0 * w, size, color, TextAlign::Center, 500.0, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.02;
    }
    let id = b.add(control, t);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.push(surface_proto::Slot { role: SlotRole::Value, node: id.0 });
    }
    id
}

// ── silkscreen ───────────────────────────────────────────────────────────

/// A text node whose box is `w` wide from `x`, vertically centred on `cy`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn text(name: &str, s: &str, x: f64, cy: f64, w: f64, size: f64, color: &str, align: TextAlign, weight: f64, upper: bool) -> Node {
    let mut n = draw::text(name, s, x, cy - size * 0.68, w, size, color, align);
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(weight);
        if upper {
            t.text_case = TextCase::Upper;
            t.letter_spacing = size * 0.08;
        }
    }
    n
}

/// Rough width of `s` at `size`, uppercase with the silkscreen's tracking.
#[must_use]
pub fn text_w(s: &str, size: f64) -> f64 {
    s.chars().count() as f64 * size * 0.72
}

/// Silkscreen: uppercase, tracked, centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn silk(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("silk", s, cx - w / 2.0, cy, w, size, color, TextAlign::Center, weight, true))
}

/// Silkscreen set from its left edge.
#[allow(clippy::too_many_arguments)]
pub fn silk_left(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("silk", s, x, cy, w, size, color, TextAlign::Left, weight, true))
}

/// Silkscreen set to end at `x`.
#[allow(clippy::too_many_arguments)]
pub fn silk_right(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("silk", s, x - w, cy, w, size, color, TextAlign::Right, weight, true))
}

/// Mixed-case text (a model line in script-less caps is too loud).
#[allow(clippy::too_many_arguments)]
pub fn plain(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, size: f64, color: &str, weight: f64) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(parent, text("print", s, cx - w / 2.0, cy, w, size, color, TextAlign::Center, weight, false))
}

/// A hairline rule.
pub fn rule(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, w: f64, h: f64, color: &str) -> NodeId {
    b.add(parent, draw::rect("rule", x, y, w, h, draw::solid(color)))
}

/// A rounded outline box (section frame) with an optional title set into
/// its top edge on a patch of `panel` colour.
#[allow(clippy::too_many_arguments)]
pub fn section(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, w: f64, h: f64, color: &str, weight: f64) -> NodeId {
    let n = draw::stroked(draw::rounded(draw::frame("section", x, y, w, h), 6.0), color, weight);
    b.add(parent, n)
}

// ── lamps & windows ──────────────────────────────────────────────────────

fn mixed(a: &str, b: &str, t: f64) -> String {
    let c = draw::mix(draw::hex(a), draw::hex(b), t);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b))
}

/// A lit lens: hot core, the colour, a dark rim, and the light it gives off.
#[must_use]
pub fn lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let hot = [
        (0.0, "#ffffffee".to_string()),
        (0.2, mixed(color, "#ffffff", 0.55)),
        (0.55, color.to_string()),
        (1.0, mixed(color, "#000000", 0.5)),
    ];
    let stops: Vec<(f64, &str)> = hot.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::emissive(draw::circle("lamp", cx, cy, r, draw::radial_at(0.38, 0.32, 0.9, &stops)), draw::hex(color), 0.8, r * 2.4)
}

/// An unlit lens in its bezel.
#[must_use]
pub fn dark_lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let dark = [(0.0, "#ffffff22".to_string()), (0.5, mixed(color, "#000000", 0.62)), (1.0, "#000000d0".to_string())];
    let stops: Vec<(f64, &str)> = dark.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::with_effects(draw::circle("lens", cx, cy, r, draw::radial_at(0.38, 0.3, 0.95, &stops)), vec![draw::inner_shadow("#000000b0", 0.0, 0.0, r * 0.7)])
}

/// A panel lamp in a black bezel, lit or not (a pilot light). Decoration,
/// not a control.
pub fn lamp(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, d: f64, color: &str, lit: bool) {
    let r = d / 2.0;
    b.add(parent, draw::with_effects(draw::circle("bezel", cx, cy, r + (d * 0.16).max(1.2), draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")])), vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)]));
    b.add(parent, dark_lens(cx, cy, r, color));
    if lit {
        b.add(parent, lens(cx, cy, r, color));
    }
}

// ── catalog entries ──────────────────────────────────────────────────────

/// A continuous param with a real range; `default` in real units.
#[must_use]
pub fn param(addr: &str, name: &str, range: Range, default: f64) -> ParamInfo {
    ParamInfo::new(addr, name, range).with_default_real(default)
}

/// A dB param over `min..max`.
#[must_use]
pub fn db(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    let p = param(addr, name, Range::linear(min, max, Unit::Db), default);
    if min < 0.0 && max > 0.0 { p.bipolar() } else { p }
}

/// A time in ms swept logarithmically.
#[must_use]
pub fn ms(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    param(addr, name, Range::log(min, max, Unit::Milliseconds), default)
}

/// 0–100 %.
#[must_use]
pub fn percent(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(addr, name, Range::linear(0.0, 100.0, Unit::Percent), default)
}

/// A stepped selector over named positions; `default` is an index.
#[must_use]
pub fn choice(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).choice(options);
    let last = options.len().saturating_sub(1).max(1) as f64;
    p.default = default as f64 / last;
    p
}

/// An on/off param.
#[must_use]
pub fn toggle(addr: &str, name: &str, on: bool) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).toggle();
    p.default = if on { 1.0 } else { 0.0 };
    p
}

/// A gain-reduction stream.
#[must_use]
pub fn gr_stream(id: &str, name: &str) -> StreamInfo {
    StreamInfo { id: EntryId::new(id), name: name.into(), kind: StreamKind::GainReduction, rate_hz: 30.0 }
}

/// A level stream of `channels`.
#[must_use]
pub fn level_stream(id: &str, name: &str, channels: u8) -> StreamInfo {
    StreamInfo { id: EntryId::new(id), name: name.into(), kind: StreamKind::Level { channels }, rate_hz: 30.0 }
}

/// A param binding.
#[must_use]
pub fn bind(addr: &str) -> Binding {
    Binding::param(addr)
}

