//! Panel-building helpers shared by the tube faces: placing kit components
//! by their centre, silkscreen that survives both 1× and 2×, readout
//! windows, and the catalog entries every face declares.

use document_proto::{Node, NodeId, NodeKind, Paint, TextAlign, TextCase, TextData, Transform};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{Binding, Control, ControlKind, ParamInfo, Range, SlotRole, StreamInfo, StreamKind, Unit, ValueFormat};
use surface_widgets::Builder;

// ── placing ─────────────────────────────────────────────────────────────

/// Place kit component `id`; panics on a bad id/variant (faces are literals).
pub fn put(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place) -> NodeId {
    surface_kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"))
}

pub fn size_of(b: &Builder<'_>, id: NodeId) -> (f64, f64) {
    b.scene.node(id).map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
}

/// Size the face's root frame to its panel (the root has no layout to hug
/// with, and a renderer sizes the page from it).
pub fn fit_root(b: &mut Builder<'_>, root: NodeId, panel: NodeId) {
    let (w, h) = size_of(b, panel);
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size { width: w, height: h };
    }
}

pub fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

/// A rotary (knob, rotary switch) with its shaft at (`cx`, `cy`): every kit
/// rotary is drawn in a square the width of its root, centred on the shaft.
pub fn rotary_at(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, cx: f64, cy: f64) -> NodeId {
    let n = put(b, parent, id, p);
    let (w, _) = size_of(b, n);
    move_to(b, n, cx - w / 2.0, cy - w / 2.0);
    n
}

/// Anything with its box centred on (`cx`, `cy`).
pub fn centred_at(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, cx: f64, cy: f64) -> NodeId {
    let n = put(b, parent, id, p);
    let (w, h) = size_of(b, n);
    move_to(b, n, cx - w / 2.0, cy - h / 2.0);
    n
}

/// Restyle a component's caption (its `Label` text) — size, weight, colour.
pub fn caption_style(b: &mut Builder<'_>, root: NodeId, size: f64, weight: f64, color: &str) {
    let label = b.controls.get(&root).and_then(|c| c.slots_of(SlotRole::Label).next());
    let Some(label) = label else { return };
    if let Some(n) = b.scene.node_mut(NodeId(label))
        && let NodeKind::Text(t) = &mut n.kind
    {
        let grow = size / t.font_size;
        t.font_size = size;
        t.font_weight = Some(weight);
        t.letter_spacing = size * 0.12;
        n.size.height = (n.size.height * grow).ceil();
        n.fills = vec![draw::solid(color)];
    }
}

/// Move a component's caption so its line is centred on panel `y` (to line
/// a switch's caption up with the knobs' beside it). The component's root
/// must already be in place.
pub fn caption_y(b: &mut Builder<'_>, root: NodeId, y: f64) {
    let label = b.controls.get(&root).and_then(|c| c.slots_of(SlotRole::Label).next());
    let top = b.scene.node(root).map_or(0.0, |n| n.transform.ty);
    if let Some(label) = label
        && let Some(n) = b.scene.node_mut(NodeId(label))
    {
        let h = n.size.height;
        n.transform.ty = y - top - h / 2.0;
    }
}

/// Hide the numerals (knob) or option legends (rotary switch) a component
/// prints round itself, for a face that engraves its own — the ticks and
/// detent dots stay.
pub fn drop_numerals(b: &mut Builder<'_>, root: NodeId) {
    for id in b.scene.walk(root) {
        if let Some(n) = b.scene.node_mut(id)
            && (n.name == "numeral" || n.name == "option")
        {
            n.visible = false;
        }
    }
}

/// Engrave `labels` evenly over `sweep` degrees (centred on straight up)
/// round a shaft at (`cx`, `cy`), each set *outward* from radius `r` — a
/// label on the right starts at the radius, one on the left ends at it — so
/// no legend ever runs back into the knob, whatever its length.
pub fn dial_numerals(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, sweep: f64, labels: &[&str], ink: Ink<'_>) {
    let n = labels.len();
    for (i, s) in labels.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let t = if n < 2 { 0.5 } else { i as f64 / (n - 1) as f64 };
        let a = (t - 0.5) * sweep;
        let (x, y) = draw::polar(cx, cy, r, a);
        let (sin, cos) = a.to_radians().sin_cos();
        let tw = s.chars().count() as f64 * ink.size * (0.6 + ink.track);
        silk(b, parent, s, x + sin * tw / 2.0, y - cos * ink.size * 0.35, ink);
    }
}

/// A knob's scale, re-engraved: the kit's numerals hidden, `labels` set
/// outward from radius `r` over the knob's `sweep`.
#[allow(clippy::too_many_arguments)]
pub fn knob_scale(b: &mut Builder<'_>, parent: NodeId, knob: NodeId, at: (f64, f64), r: f64, sweep: f64, labels: &[&str], ink: Ink<'_>) {
    drop_numerals(b, knob);
    dial_numerals(b, parent, at, r, sweep, labels, ink);
}

/// A detented rotary selector over a choice param: the kit's chicken-head
/// stepped through `labels` (≤ 60° a detent), which it prints round itself
/// clear of its ticks, in the panel's ink, at a size the panel can read; its
/// hit box tight to the pointer. `p` carries the moulding variant, legend
/// and binding. Returns the control's root.
pub fn selector(b: &mut Builder<'_>, parent: NodeId, p: &Place, at: (f64, f64), labels: &[&str], ink: Ink<'_>) -> NodeId {
    let place = p.clone().options(labels).ink(surface_kit::Ink::Color(ink.color.to_string())).touch(TOUCH_MIN);
    rotary_at(b, parent, "knob.chicken_head", &place, at.0, at.1)
}

/// Repaint a panel with a straight top-to-bottom sheen (`stops` top first).
pub fn repaint(b: &mut Builder<'_>, panel: NodeId, stops: &[(f64, &str)]) {
    if let Some(n) = b.scene.node_mut(panel) {
        n.fills = vec![draw::linear(180.0, stops)];
        n.stroke.weight = 1.0;
    }
}

// ── silkscreen ──────────────────────────────────────────────────────────

/// How a line of silkscreen is set.
#[derive(Clone, Copy)]
pub struct Ink<'a> {
    pub size: f64,
    pub color: &'a str,
    pub weight: f64,
    /// Letter spacing, em.
    pub track: f64,
    pub upper: bool,
}

impl<'a> Ink<'a> {
    pub const fn new(size: f64, color: &'a str) -> Self {
        Self { size, color, weight: 600.0, track: 0.08, upper: true }
    }
    pub const fn weight(mut self, w: f64) -> Self {
        self.weight = w;
        self
    }
    pub const fn track(mut self, t: f64) -> Self {
        self.track = t;
        self
    }
    pub const fn plain(mut self) -> Self {
        self.upper = false;
        self
    }
    pub const fn color(mut self, c: &'a str) -> Self {
        self.color = c;
        self
    }
    pub const fn size(mut self, s: f64) -> Self {
        self.size = s;
        self
    }
}

/// Generous box width for `s` at `ink`, so a line never wraps.
fn width_for(s: &str, ink: &Ink<'_>) -> f64 {
    s.chars().count() as f64 * ink.size * (0.72 + ink.track) + ink.size * 2.0
}

fn text_node(s: &str, x: f64, cy: f64, w: f64, ink: &Ink<'_>, align: TextAlign) -> Node {
    let mut n = draw::text("silk", s, x, cy - ink.size * 0.68, w, ink.size, ink.color, align);
    if let NodeKind::Text(t) = &mut n.kind {
        *t = TextData {
            characters: s.to_string(),
            font_size: ink.size,
            font_weight: Some(ink.weight),
            letter_spacing: ink.size * ink.track,
            text_case: if ink.upper { TextCase::Upper } else { TextCase::Original },
            align,
            ..TextData::default()
        };
    }
    n.name = format!("silk {s}");
    n
}

/// Silkscreen centred on (`cx`, `cy`).
pub fn silk(b: &mut Builder<'_>, parent: NodeId, s: &str, cx: f64, cy: f64, ink: Ink<'_>) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, cx - w / 2.0, cy, w, &ink, TextAlign::Center))
}

/// Silkscreen starting at `x`, vertically centred on `cy`.
pub fn silk_left(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, ink: Ink<'_>) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, x, cy, w, &ink, TextAlign::Left))
}

/// Silkscreen ending at `x`, vertically centred on `cy`.
pub fn silk_right(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, ink: Ink<'_>) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, x - w, cy, w, &ink, TextAlign::Right))
}

/// A hairline rule from (`x0`, `y`) to (`x1`, `y`).
pub fn rule(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64, weight: f64, color: &str) -> NodeId {
    b.add(parent, draw::rect("rule", x0, y - weight / 2.0, x1 - x0, weight, draw::solid(color)))
}

/// An engraved plate: a rounded rectangle in `fill` with a fine bevel.
#[allow(clippy::too_many_arguments)]
pub fn plate(b: &mut Builder<'_>, parent: NodeId, name: &str, x: f64, y: f64, w: f64, h: f64, fill: Paint, radius: f64) -> NodeId {
    let n = draw::rounded(draw::rect(name, x, y, w, h, fill), radius);
    let n = draw::stroked(n, "#00000066", 1.0);
    b.add(parent, draw::with_effects(n, vec![draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0), draw::drop_shadow("#00000040", 0.0, 1.0, 1.5)]))
}

// ── hardware the faces draw themselves ──────────────────────────────────

/// A cross-head panel screw, head `d` across, in `chrome`.
pub fn phillips(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), d: f64, chrome: &str) {
    let head = draw::circle("screw", cx, cy, d / 2.0, draw::radial_at(0.36, 0.30, 0.85, &[(0.0, "#ffffffd9"), (0.35, chrome), (1.0, "#00000099")]));
    let head = draw::with_effects(draw::stroked(head, "#00000066", 0.6), vec![draw::drop_shadow("#00000059", 0.4, 0.8, 1.2)]);
    b.add(parent, head);
    let t = (d * 0.13).max(0.7);
    let arm = d * 0.62;
    for deg in [20.0, 110.0] {
        b.add(parent, draw::rotated(draw::rect("screw slot", cx - arm / 2.0, cy - t / 2.0, arm, t, draw::solid("#0000008c")), deg));
    }
}

/// A slotted-oval rack-ear hole (the mounting slot, dark behind).
pub fn rack_slot(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), w: f64, h: f64) {
    let n = draw::rounded(draw::rect("rack slot", cx - w / 2.0, cy - h / 2.0, w, h, draw::solid("#0d0d0f")), h / 2.0);
    b.add(parent, draw::with_effects(n, vec![draw::inner_shadow("#000000cc", 0.0, 1.0, 2.0), draw::drop_shadow("#ffffff26", 0.0, 0.8, 0.0)]));
}

/// A rack screw through an ear: a dark oval-head bolt with a cross.
pub fn rack_bolt(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), d: f64) {
    let head = draw::circle("rack bolt", cx, cy, d / 2.0, draw::radial_at(0.38, 0.32, 0.8, &[(0.0, "#6a6a66"), (0.5, "#262624"), (1.0, "#0b0b0a")]));
    b.add(parent, draw::with_effects(head, vec![draw::drop_shadow("#00000080", 0.5, 1.2, 2.0)]));
    let t = (d * 0.12).max(0.7);
    for deg in [0.0, 90.0] {
        b.add(parent, draw::rotated(draw::rect("rack bolt slot", cx - d * 0.3, cy - t / 2.0, d * 0.6, t, draw::solid("#000000b3")), deg));
    }
}

/// A plain strip down one edge of a panel (a rack ear folded from the
/// same sheet): its paint, and the seam where it meets the face.
pub fn ear_strip(b: &mut Builder<'_>, panel: NodeId, x: f64, w: f64, h: f64, fill: Paint, seam_x: f64) {
    b.add(panel, draw::rect("ear", x, 0.0, w, h, fill));
    b.add(panel, draw::rect("ear seam", seam_x - 0.5, 0.0, 1.0, h, draw::solid("#00000059")));
    b.add(panel, draw::rect("ear seam light", seam_x + 0.5, 0.0, 1.0, h, draw::solid("#ffffff26")));
}

/// Numerals round a shaft at (`cx`, `cy`), each turned to the circle so its
/// top faces out (the way a vintage panel engraves them), centred on
/// radius `r` and spread evenly over `sweep` degrees.
pub fn ring_numerals(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, sweep: f64, labels: &[&str], ink: Ink<'_>) {
    let n = labels.len();
    for (i, s) in labels.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let t = if n < 2 { 0.5 } else { i as f64 / (n - 1) as f64 };
        let a = (t - 0.5) * sweep;
        let (x, y) = draw::polar(cx, cy, r, a);
        let w = width_for(s, &ink);
        let mut node = text_node(s, 0.0, 0.0, w, &ink, TextAlign::Center);
        node.transform = document_proto::Transform::IDENTITY;
        // A frame turned about the glyphs' middle carries the text round
        // (a renderer turns a frame's subtree, not a text node's own box).
        let h = node.size.height;
        let holder = b.add(parent, draw::rotated(draw::frame("silk turn", x - w / 2.0, y - h / 2.0, w, h), a));
        b.add(holder, node);
    }
}

/// A panel's own marks at the angles it really prints them (degrees from
/// up, clockwise): a dot `d` across on radius `r` for each, and its label
/// set outward from `r_label`, upright.
#[allow(clippy::too_many_arguments)]
pub fn marks_at(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, r_label: f64, marks: &[(f64, &str)], d: f64, ink: Ink<'_>) {
    for (a, s) in marks {
        let (x, y) = draw::polar(cx, cy, r, *a);
        b.add(parent, draw::circle("scale dot", x, y, d / 2.0, draw::solid(ink.color)));
        if s.is_empty() {
            continue;
        }
        let (lx, ly) = draw::polar(cx, cy, r_label, *a);
        let (sin, cos) = a.to_radians().sin_cos();
        let tw = s.chars().count() as f64 * ink.size * (0.6 + ink.track);
        silk(b, parent, s, lx + sin * tw / 2.0, ly - cos * ink.size * 0.35, ink);
    }
}

/// A range whose knob travel lands on a panel's unevenly printed marks:
/// `marks` are `(angle, value)` from the start of travel to its end over
/// `sweep` degrees; between them the value runs linearly.
pub fn printed_range(marks: &[(f64, f64)], sweep: f64, unit: Unit) -> Range {
    let (lo, hi) = (marks[0].1, marks[marks.len() - 1].1);
    let points = marks
        .iter()
        .map(|(a, v)| surface_proto::CurvePoint { x: (a + sweep / 2.0) / sweep, y: (v - lo) / (hi - lo) })
        .collect();
    Range { scale: surface_proto::Scale::Curve { curve: surface_proto::Curve::Points { points } }, ..Range::linear(lo, hi, unit) }
}

/// A dot at each of `n` evenly spaced positions over `sweep` degrees.
#[allow(clippy::too_many_arguments)]
pub fn ring_dots(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, sweep: f64, n: usize, d: f64, color: &str) {
    for i in 0..n {
        let t = if n < 2 { 0.5 } else { i as f64 / (n - 1) as f64 };
        let (x, y) = draw::polar(cx, cy, r, (t - 0.5) * sweep);
        b.add(parent, draw::circle("scale dot", x, y, d / 2.0, draw::solid(color)));
    }
}

/// `n` radial ticks from `r0` to `r1` over `sweep` degrees, `w` wide.
#[allow(clippy::too_many_arguments)]
pub fn ring_ticks(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), (r0, r1): (f64, f64), sweep: f64, n: usize, w: f64, color: &str) {
    let mut d = String::new();
    for i in 0..n {
        let t = if n < 2 { 0.5 } else { i as f64 / (n - 1) as f64 };
        let a = (t - 0.5) * sweep;
        let (x0, y0) = draw::polar(cx, cy, r0, a);
        let (x1, y1) = draw::polar(cx, cy, r1, a);
        let (dx, dy) = ((x1 - x0) / (r1 - r0) * w / 2.0, (y1 - y0) / (r1 - r0) * w / 2.0);
        d.push_str(&draw::polygon(&[(x0 - dy, y0 + dx), (x1 - dy, y1 + dx), (x1 + dy, y1 - dx), (x0 + dy, y0 - dx)]));
        d.push(' ');
    }
    b.add(parent, draw::path("scale ticks", 0.0, 0.0, cx + r1 + 2.0, cy + r1 + 2.0, d, draw::solid(color)));
}

/// An arc of radius `r`, `w` wide, over `sweep` degrees centred on up.
pub fn ring_arc(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, sweep: f64, w: f64, color: &str) {
    let d = draw::sector(cx, cy, r - w / 2.0, r + w / 2.0, -sweep / 2.0, sweep / 2.0);
    b.add(parent, draw::path("scale arc", 0.0, 0.0, cx + r + w, cy + r + w, d, draw::solid(color)));
}

/// Hide everything a kit component prints round itself (its ring, ticks,
/// dots and numerals), for a face that engraves its own.
pub fn hide_ring(b: &mut Builder<'_>, root: NodeId) {
    for id in b.scene.walk(root) {
        if let Some(n) = b.scene.node_mut(id)
            && (n.name.starts_with("scale") || n.name == "numeral" || n.name == "option" || n.name == "detent" || n.name == "tick")
        {
            n.visible = false;
        }
    }
}

/// Silkscreen leaning like an italic, starting at `x` and centred on `cy`:
/// the text in a frame sheared by `k` about its middle.
pub fn silk_slanted(b: &mut Builder<'_>, parent: NodeId, s: &str, x: f64, cy: f64, ink: Ink<'_>, k: f64) -> NodeId {
    let w = width_for(s, &ink);
    let mut t = text_node(s, 0.0, 0.0, w, &ink, TextAlign::Left);
    t.transform = document_proto::Transform::IDENTITY;
    let h = t.size.height;
    let mut holder = draw::frame("silk slant", x, cy - h / 2.0, w, h);
    holder.transform.b = -k;
    holder.transform.tx += k * h / 2.0;
    let holder = b.add(parent, holder);
    b.add(holder, t);
    holder
}

// ── readouts ────────────────────────────────────────────────────────────

/// A small readout window: a dark recessed glass with the bound value
/// printed in it (the runtime writes the `Value` slot, in `format`), its
/// figures lit. `placeholder` is what a still render shows before the
/// engine speaks.
#[allow(clippy::too_many_arguments)]
pub fn readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    binding: Binding,
    (cx, cy): (f64, f64),
    (w, h): (f64, f64),
    placeholder: &str,
    glow: &str,
    format: Option<ValueFormat>,
) -> NodeId {
    let mut n = draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h);
    n.fills = vec![draw::solid("#0b0c0d")];
    let n = draw::stroked(draw::rounded(n, h * 0.22), "#000000cc", 1.0);
    let n = draw::with_effects(
        n,
        vec![draw::inner_shadow("#000000e6", 0.0, 1.5, 3.0), draw::drop_shadow("#ffffff2e", 0.0, 1.0, 0.0)],
    );
    let root = b.add(parent, n);
    // The glass: a faint sheen over the top half.
    b.add(root, draw::rounded(draw::rect("glass", 1.0, 1.0, w - 2.0, h * 0.45, draw::linear(180.0, &[(0.0, "#ffffff14"), (1.0, "#ffffff00")])), h * 0.2));
    let size = h * 0.56;
    let ink = Ink::new(size, glow).weight(600.0).track(0.06).plain();
    let mut t = text_node(placeholder, 0.0, h / 2.0, w, &ink, TextAlign::Center);
    t.name = "value".into();
    let value = b.add(root, draw::emissive(t, draw::hex(glow), 0.45, size * 0.6));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    c.format = format;
    b.controls.insert(root, c);
    root
}

// ── catalog ─────────────────────────────────────────────────────────────

/// A continuous, featured param.
pub fn param(addr: &str, name: &str, range: Range, default_real: f64) -> ParamInfo {
    ParamInfo::new(addr, name, range).featured().with_default_real(default_real)
}

/// A dial printed 0–100 in whole numbers.
pub fn dial_0_100(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(addr, name, Range { steps: Some(101), ..Range::linear(0.0, 100.0, Unit::None) }, default)
}

/// A switch over named positions (`default` = index).
pub fn choice(addr: &str, name: &str, options: &[&str], default: usize) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).featured().choice(options);
    let last = options.len().saturating_sub(1).max(1);
    p.default = default as f64 / last as f64;
    p
}

/// A two-position switch.
pub fn toggle(addr: &str, name: &str, on: bool) -> ParamInfo {
    let mut p = ParamInfo::new(addr, name, Range::default()).featured().toggle();
    p.default = if on { 1.0 } else { 0.0 };
    p
}

/// The gain-reduction stream every face meters, and an output level.
pub fn streams(face: &str, name: &str) -> Vec<StreamInfo> {
    vec![
        StreamInfo { id: surface_proto::EntryId::new(format!("{face}/gr")), name: format!("{name} Gain Reduction"), kind: StreamKind::GainReduction, rate_hz: 30.0 },
        StreamInfo { id: surface_proto::EntryId::new(format!("{face}/out")), name: format!("{name} Output"), kind: StreamKind::Level { channels: 2 }, rate_hz: 30.0 },
    ]
}

pub fn bind(addr: &str) -> Binding {
    Binding::param(addr)
}

pub fn stream(id: &str) -> Binding {
    Binding::stream(id)
}
