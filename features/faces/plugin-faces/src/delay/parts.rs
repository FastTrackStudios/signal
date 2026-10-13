//! Panel-building helpers for the delay and saturation faces: placing kit
//! components by their shaft, taking their printed parts apart, silkscreen,
//! lamps and lit windows, LED meters, and the catalog entries every face
//! declares.
//!
//! Placing is by shaft, with the kit's own tight boxes (`Place::touch`),
//! ink and stepped sweeps; what the kit doesn't print (scales with marks
//! where the value really lands, legends on one baseline) the face prints.
//! Lines are open stroked paths; lit things (a display's phosphor, a lamp's
//! lens, glowing text) *emit* rather than wear a drop-shadow glow.

use std::collections::BTreeMap;

use document_proto::{
    Node, NodeId, NodeKind, Paint, ShaderFx, Stroke, StrokeAlign, StrokeCap, StrokeJoin, TextAlign,
    TextCase, TextData, Transform,
};
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_proto::{
    Binding, ButtonMode, Control, ControlKind, EntryId, ParamInfo, Range, SlotRole, Source,
    StreamInfo, StreamKind, Unit, ValueFormat,
};
use surface_widgets::Builder;

// ── placing ──────────────────────────────────────────────────────────────

/// Move `id` so its top-left corner is at (`x`, `y`) in its parent.
pub fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

#[must_use]
pub fn size_of(b: &Builder<'_>, id: NodeId) -> (f64, f64) {
    b.scene
        .node(id)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
}

/// The point a component is placed by, in its own box: the centre of its
/// shaft for knobs and rotary switches (`knob` / `dial` child), otherwise
/// the centre of the box.
fn anchor(b: &Builder<'_>, root: NodeId) -> (f64, f64) {
    let Some(n) = b.scene.node(root) else {
        return (0.0, 0.0);
    };
    for c in &n.children {
        if let Some(k) = b.scene.node(*c)
            && matches!(k.name.as_str(), "knob" | "dial")
        {
            return (
                k.transform.tx + k.size.width / 2.0,
                k.transform.ty + k.size.height / 2.0,
            );
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
pub fn panel(
    b: &mut Builder<'_>,
    parent: NodeId,
    id: &str,
    variant: &str,
    w: f64,
    h: f64,
    ends: &str,
) -> NodeId {
    let opts = [format!("w={w}"), format!("h={h}"), format!("ends={ends}")];
    let p = Place {
        options: Some(opts.to_vec()),
        ..Place::new("Faceplate").variant(variant)
    };
    put_at(b, parent, id, &p, 0.0, 0.0)
}

/// Size the canvas's root frame to its panel, so the face renders at the
/// panel's size.
pub fn fit_root(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64) {
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size {
            width: w,
            height: h,
        };
    }
}

/// Paint a placed panel (its own fill) straight top to bottom in `stops`:
/// a unit's own colour on the kit's finish.
pub fn repaint(b: &mut Builder<'_>, panel: NodeId, stops: &[(f64, &str)]) {
    if let Some(n) = b.scene.node_mut(panel) {
        n.fills = vec![draw::linear(180.0, stops)];
    }
}

fn children_named(b: &Builder<'_>, id: NodeId, names: &[&str]) -> Vec<NodeId> {
    b.scene
        .walk(id)
        .into_iter()
        .filter(|c| {
            *c != id
                && b.scene
                    .node(*c)
                    .is_some_and(|n| names.contains(&n.name.as_str()))
        })
        .collect()
}

fn remove(b: &mut Builder<'_>, ids: Vec<NodeId>) {
    for id in ids {
        if b.scene.node(id).is_some() {
            let _ = b.scene.apply(document_proto::Op::Remove { id });
        }
    }
}

/// Take a placed knob's own printed scale off — the face prints its own.
pub fn strip_scale(b: &mut Builder<'_>, knob: NodeId) {
    let ids = children_named(
        b,
        knob,
        &[
            "scale dots",
            "scale arc",
            "scale majors",
            "scale minors",
            "numeral",
        ],
    );
    remove(b, ids);
}

/// Take a placed control's legend (and its `Label` slot) off: the face
/// prints the legend itself.
pub fn strip_legend(b: &mut Builder<'_>, control: NodeId) {
    let ids: Vec<NodeId> = b
        .controls
        .get(&control)
        .map(|c| c.slots_of(SlotRole::Label).map(NodeId).collect())
        .unwrap_or_default();
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
    }
    remove(b, ids);
}

/// Set how a placed control's readout prints its value.
pub fn format(b: &mut Builder<'_>, control: NodeId, f: ValueFormat) {
    if let Some(c) = b.controls.get_mut(&control) {
        c.format = Some(f);
    }
}

/// Restyle a component's caption (its `Label` text).
pub fn caption_style(b: &mut Builder<'_>, root: NodeId, ink: Ink<'_>) {
    let label = b
        .controls
        .get(&root)
        .and_then(|c| c.slots_of(SlotRole::Label).next());
    let Some(label) = label else { return };
    if let Some(n) = b.scene.node_mut(NodeId(label))
        && let NodeKind::Text(t) = &mut n.kind
    {
        let grow = ink.size / t.font_size;
        t.font_size = ink.size;
        t.font_weight = Some(ink.weight);
        t.letter_spacing = ink.size * ink.track;
        n.size.height = (n.size.height * grow).ceil();
        n.fills = vec![draw::solid(ink.color)];
    }
}

/// A knob as these faces use it: kit style `id` of diameter `d` with its
/// shaft at (`cx`, `cy`), its own legend and printed scale off (the face
/// prints both: marks where the value lands, legends on one baseline so a
/// row of knobs of different sizes reads as a row), and a tight box — the
/// body, at least a fingertip.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    id: &str,
    variant: &str,
    addr: &str,
    legend: &str,
    (cx, cy): (f64, f64),
    d: f64,
) -> NodeId {
    let mut p = Place::new(legend)
        .size(d)
        .bind(bind(addr))
        .legend(legend)
        .touch(TOUCH_MIN);
    if !variant.is_empty() {
        p = p.variant(variant);
    }
    let k = put(b, parent, id, &p, cx, cy);
    strip_legend(b, k);
    strip_scale(b, k);
    k
}

/// A placed knob's sweep in degrees.
#[must_use]
pub fn sweep_of(b: &Builder<'_>, control: NodeId) -> f64 {
    match b.controls.get(&control).map(|c| &c.kind) {
        Some(ControlKind::Knob { sweep_degrees }) => *sweep_degrees,
        _ => 300.0,
    }
}

/// A live value line (`Value` slot) for a placed control, centred under it
/// at panel `cy` — outside the control's box, so the box stays the body.
pub fn value_at(b: &mut Builder<'_>, control: NodeId, cy: f64, ink: Ink<'_>) -> NodeId {
    let ty = b.scene.node(control).map_or(0.0, |n| n.transform.ty);
    let (w, _) = size_of(b, control);
    let bw = 90.0;
    let mut t = text_node("", w / 2.0 - bw / 2.0, cy - ty, bw, &ink, TextAlign::Center);
    t.name = "value".into();
    let id = b.add(control, t);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.push(surface_proto::Slot {
            role: SlotRole::Value,
            node: id.0,
        });
    }
    id
}

/// The printed legend of a placed control, centred under it at panel
/// `cy`, as the control's `Label` slot — so the control is labelled (for
/// lint, for a screen reader) by the word printed on the panel.
pub fn legend_at(b: &mut Builder<'_>, control: NodeId, s: &str, cy: f64, ink: Ink<'_>) -> NodeId {
    let (tx, (w, _)) = (
        b.scene.node(control).map_or(0.0, |n| n.transform.tx),
        size_of(b, control),
    );
    label_at(b, control, s, (tx + w / 2.0, cy), ink)
}

/// A control's printed legend centred on panel (`cx`, `cy`) — wherever the
/// panel prints it (beside a footswitch's LED, under a selector) — as its
/// `Label` slot.
pub fn label_at(
    b: &mut Builder<'_>,
    control: NodeId,
    s: &str,
    (cx, cy): (f64, f64),
    ink: Ink<'_>,
) -> NodeId {
    let (tx, ty) = b
        .scene
        .node(control)
        .map_or((0.0, 0.0), |n| (n.transform.tx, n.transform.ty));
    let bw = width_for(s, &ink);
    let t = text_node(s, cx - tx - bw / 2.0, cy - ty, bw, &ink, TextAlign::Center);
    let id = b.add(control, t);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
        c.slots.push(surface_proto::Slot {
            role: SlotRole::Label,
            node: id.0,
        });
    }
    id
}

/// A scale printed round a shaft at (`cx`, `cy`) with marks where the
/// value really lands: `marks` are (position 0..1, label); `minors` evenly
/// spaced minor ticks across the travel. Labels are set outward.
#[allow(clippy::too_many_arguments)]
pub fn marks(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy): (f64, f64),
    r: f64,
    sweep: f64,
    marks: &[(f64, &str)],
    minors: usize,
    ink: Ink<'_>,
) {
    let side = 2.0 * (r + 10.0);
    let (ox, oy) = (cx - side / 2.0, cy - side / 2.0);
    let angle = |p: f64| (p - 0.5) * sweep;
    let tick = |a: f64, len: f64| vec![draw::polar(cx, cy, r, a), draw::polar(cx, cy, r + len, a)];
    if minors >= 2 {
        let minor: Vec<Vec<(f64, f64)>> = (0..minors)
            .map(|i| tick(angle(i as f64 / (minors - 1) as f64), 2.6))
            .collect();
        let mut n = stroke_path(
            "scale minors",
            (ox, oy, side, side),
            &minor,
            0.8,
            ink.color,
            false,
        );
        n.stroke.cap = StrokeCap::None;
        b.add(parent, draw::with_opacity(n, 0.6));
    }
    let major: Vec<Vec<(f64, f64)>> = marks.iter().map(|(p, _)| tick(angle(*p), 4.2)).collect();
    let mut n = stroke_path(
        "scale majors",
        (ox, oy, side, side),
        &major,
        1.3,
        ink.color,
        false,
    );
    n.stroke.cap = StrokeCap::None;
    b.add(parent, n);
    for (p, l) in marks {
        if l.is_empty() {
            continue;
        }
        let a = angle(*p);
        let tw = l.chars().count() as f64 * ink.size * 0.62;
        let push = 0.5 * (tw * a.to_radians().sin().abs() + ink.size * a.to_radians().cos().abs());
        let (lx, ly) = draw::polar(cx, cy, r + 7.0 + push, a);
        silk(b, parent, l, lx, ly, ink);
    }
}

/// Where `v` lands on a log sweep `min..max` (0..1).
#[must_use]
pub fn log_pos(v: f64, min: f64, max: f64) -> f64 {
    (v / min).ln() / (max / min).ln()
}

/// A detented rotary selector over a choice param: the kit's chicken-head
/// given its positions (`Place::options` — stepped, ≤ 60° a detent, the
/// positions printed round it in `ink`, as written). `variant` is the
/// moulding (`cream`, `black`, `red`). The face prints the legend.
#[allow(clippy::too_many_arguments)]
pub fn selector(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    variant: &str,
    (cx, cy): (f64, f64),
    d: f64,
    labels: &[&str],
    ink: &str,
) -> NodeId {
    let p = Place::new(name)
        .size(d)
        .variant(variant)
        .legend(name)
        .options(labels)
        .bind(bind(addr))
        .ink(surface_kit::Ink::Color(ink.into()))
        .keep_case()
        .touch(TOUCH_MIN);
    let k = put(b, parent, "knob.chicken_head", &p, cx, cy);
    strip_legend(b, k);
    k
}

/// A stomp footswitch (the kit's), its actuator centred on (`cx`, `cy`).
pub fn footswitch(b: &mut Builder<'_>, parent: NodeId, p: &Place, (cx, cy): (f64, f64)) -> NodeId {
    let root =
        surface_kit::place(b, parent, "switch.footswitch", p).unwrap_or_else(|e| panic!("{e}"));
    let cap = b.scene.node(root).and_then(|n| {
        n.children.iter().find_map(|c| {
            b.scene.node(*c).filter(|k| k.name == "cap").map(|k| {
                (
                    k.transform.tx + k.size.width / 2.0,
                    k.transform.ty + k.size.height / 2.0,
                )
            })
        })
    });
    let (ax, ay) = cap.unwrap_or_else(|| {
        let (w, h) = size_of(b, root);
        (w / 2.0, h / 2.0)
    });
    move_to(b, root, cx - ax, cy - ay);
    root
}

/// A vertical LED bar meter (the kit's `meter.led_column`), `thick × len`
/// with its top-left at (`x`, `y`): `segments` over `floor..ceiling` dB, lit
/// from the bottom by channel `channel` of `stream`.
#[allow(clippy::too_many_arguments)]
pub fn led_column(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    stream: &str,
    channel: u8,
    (x, y, thick, len): (f64, f64, f64, f64),
    segments: usize,
    (floor, ceiling): (f64, f64),
) -> NodeId {
    let binding = Binding {
        source: Source::Stream {
            id: EntryId::new(stream),
            channel: Some(channel),
        },
        ..Binding::stream(stream)
    };
    let opts = [
        format!("floor={floor}"),
        format!("ceiling={ceiling}"),
        format!("segments={segments}"),
        format!("thick={thick}"),
    ];
    let p = Place {
        options: Some(opts.to_vec()),
        ..Place::new(name).size(len).bind(binding)
    };
    put_at(b, parent, "meter.led_column", &p, x, y)
}

/// Put a shader post-effect on `id` (GPU renderers only — the face reads
/// the same without it).
pub fn shader(b: &mut Builder<'_>, id: NodeId, name: &str, params: &[(&str, f64)]) {
    if let Some(n) = b.scene.node_mut(id) {
        n.shader = Some(ShaderFx {
            shader: name.into(),
            params: params
                .iter()
                .map(|(k, v)| ((*k).to_string(), *v))
                .collect::<BTreeMap<_, _>>(),
            inputs: Vec::new(),
        });
    }
}

/// Glass over a placed VU's window (its `card`).
pub fn glass(b: &mut Builder<'_>, vu: NodeId) {
    let card = b
        .scene
        .walk(vu)
        .into_iter()
        .find(|c| b.scene.node(*c).is_some_and(|n| n.name == "card"));
    if let Some(card) = card {
        shader(b, card, "glass", &[("strength", 0.45)]);
    }
}

// ── silkscreen ───────────────────────────────────────────────────────────

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
    #[must_use]
    pub const fn new(size: f64, color: &'a str) -> Self {
        Self {
            size,
            color,
            weight: 700.0,
            track: 0.1,
            upper: true,
        }
    }
    #[must_use]
    pub const fn weight(mut self, w: f64) -> Self {
        self.weight = w;
        self
    }
    #[must_use]
    pub const fn track(mut self, t: f64) -> Self {
        self.track = t;
        self
    }
    #[must_use]
    pub const fn plain(mut self) -> Self {
        self.upper = false;
        self
    }
    #[must_use]
    pub const fn color(mut self, c: &'a str) -> Self {
        self.color = c;
        self
    }
    #[must_use]
    pub const fn size(mut self, s: f64) -> Self {
        self.size = s;
        self
    }
}

/// Generous box width for `s` at `ink`, so a line never wraps.
fn width_for(s: &str, ink: &Ink<'_>) -> f64 {
    s.chars().count() as f64 * ink.size * (0.72 + ink.track) + ink.size * 2.0
}

/// A text node whose box is `w` wide from `x`, vertically centred on `cy`.
#[must_use]
pub fn text_node(s: &str, x: f64, cy: f64, w: f64, ink: &Ink<'_>, align: TextAlign) -> Node {
    let mut n = draw::text(
        "silk",
        s,
        x,
        cy - ink.size * 0.68,
        w,
        ink.size,
        ink.color,
        align,
    );
    if let NodeKind::Text(t) = &mut n.kind {
        *t = TextData {
            characters: s.to_string(),
            font_size: ink.size,
            font_weight: Some(ink.weight),
            letter_spacing: ink.size * ink.track,
            text_case: if ink.upper {
                TextCase::Upper
            } else {
                TextCase::Original
            },
            align,
            ..TextData::default()
        };
    }
    n.name = format!("silk {s}");
    n
}

/// Silkscreen centred on (`cx`, `cy`).
pub fn silk(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    ink: Ink<'_>,
) -> NodeId {
    let w = width_for(s, &ink);
    b.add(
        parent,
        text_node(s, cx - w / 2.0, cy, w, &ink, TextAlign::Center),
    )
}

/// Silkscreen starting at `x`, vertically centred on `cy`.
pub fn silk_left(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    ink: Ink<'_>,
) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, x, cy, w, &ink, TextAlign::Left))
}

/// Silkscreen ending at `x`, vertically centred on `cy`.
pub fn silk_right(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    ink: Ink<'_>,
) -> NodeId {
    let w = width_for(s, &ink);
    b.add(parent, text_node(s, x - w, cy, w, &ink, TextAlign::Right))
}

// ── lines ────────────────────────────────────────────────────────────────

/// A stroked path node: each of `lines` a polyline (open, or `closed`),
/// stroked `weight` wide in `color` with round joins, centred on the line.
/// Geometry is in the coordinates of the node's parent; the node's box is
/// `bx, by, bw, bh` there (geometry is drawn as written — it may reach past
/// the box).
#[must_use]
pub fn stroke_path(
    name: &str,
    (bx, by, bw, bh): (f64, f64, f64, f64),
    lines: &[Vec<(f64, f64)>],
    weight: f64,
    color: &str,
    closed: bool,
) -> Node {
    let mut d = String::new();
    for pts in lines {
        for (i, (x, y)) in pts.iter().enumerate() {
            d.push_str(&format!(
                "{} {:.3} {:.3} ",
                if i == 0 { "M" } else { "L" },
                x - bx,
                y - by
            ));
        }
        if closed {
            d.push_str("Z ");
        }
    }
    let mut n = draw::path(
        name,
        bx,
        by,
        bw.max(1.0),
        bh.max(1.0),
        d,
        draw::solid(color),
    );
    n.fills = Vec::new();
    n.stroke = Stroke {
        paints: vec![draw::solid(color)],
        weight,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        align: StrokeAlign::Center,
        ..Stroke::default()
    };
    n
}

/// The box round `lines`, padded by `pad`.
fn bounds(lines: &[Vec<(f64, f64)>], pad: f64) -> (f64, f64, f64, f64) {
    let pts = lines.iter().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in pts {
        (x0, y0, x1, y1) = (x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y));
    }
    if x0 > x1 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    (x0 - pad, y0 - pad, x1 - x0 + 2.0 * pad, y1 - y0 + 2.0 * pad)
}

/// [`stroke_path`] boxed to its own lines.
#[must_use]
pub fn lines_node(name: &str, lines: &[Vec<(f64, f64)>], weight: f64, color: &str) -> Node {
    stroke_path(
        name,
        bounds(lines, weight / 2.0),
        lines,
        weight,
        color,
        false,
    )
}

/// A polyline through `pts` in `parent`'s coordinates.
pub fn line(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    pts: &[(f64, f64)],
    weight: f64,
    color: &str,
) -> NodeId {
    b.add(parent, lines_node(name, &[pts.to_vec()], weight, color))
}

/// A horizontal hairline from `x0` to `x1` at `y`.
pub fn rule(
    b: &mut Builder<'_>,
    parent: NodeId,
    x0: f64,
    x1: f64,
    y: f64,
    weight: f64,
    color: &str,
) -> NodeId {
    let mut n = lines_node("rule", &[vec![(x0, y), (x1, y)]], weight, color);
    n.stroke.cap = StrokeCap::None;
    b.add(parent, n)
}

/// A vertical hairline from `y0` to `y1` at `x`.
pub fn vrule(
    b: &mut Builder<'_>,
    parent: NodeId,
    x: f64,
    y0: f64,
    y1: f64,
    weight: f64,
    color: &str,
) -> NodeId {
    let mut n = lines_node("rule", &[vec![(x, y0), (x, y1)]], weight, color);
    n.stroke.cap = StrokeCap::None;
    b.add(parent, n)
}

/// An engraved groove: a dark line with a light one under it.
pub fn groove(b: &mut Builder<'_>, parent: NodeId, x0: f64, x1: f64, y: f64) {
    rule(b, parent, x0, x1, y, 1.0, "#00000080");
    rule(b, parent, x0, x1, y + 1.0, 1.0, "#ffffff1c");
}

/// A vertical engraved groove.
pub fn vgroove(b: &mut Builder<'_>, parent: NodeId, x: f64, y0: f64, y1: f64) {
    vrule(b, parent, x, y0, y1, 1.0, "#00000080");
    vrule(b, parent, x + 1.0, y0, y1, 1.0, "#ffffff1c");
}

/// A rounded plate in `fill` with a fine bevel.
#[allow(clippy::too_many_arguments)]
pub fn plate(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    fill: Paint,
    radius: f64,
) -> NodeId {
    let n = draw::stroked(
        draw::rounded(draw::rect(name, x, y, w, h, fill), radius),
        "#00000066",
        1.0,
    );
    b.add(
        parent,
        draw::with_effects(
            n,
            vec![
                draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0),
                draw::drop_shadow("#00000040", 0.0, 1.0, 1.5),
            ],
        ),
    )
}

/// A recessed well: a darker rounded pocket sunk into the panel.
pub fn well(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    fill: Paint,
    radius: f64,
) -> NodeId {
    let mut n = draw::frame(name, x, y, w, h);
    n.fills = vec![fill];
    let n = draw::stroked(draw::rounded(n, radius), "#00000099", 1.0);
    b.add(
        parent,
        draw::with_effects(
            n,
            vec![
                draw::inner_shadow("#000000c0", 0.0, 2.0, 5.0),
                draw::drop_shadow("#ffffff1f", 0.0, 1.0, 0.0),
            ],
        ),
    )
}

// ── light ────────────────────────────────────────────────────────────────

/// `#rrggbbaa` of `c` at alpha `a`.
#[must_use]
pub fn hexa(c: &str, a: f64) -> String {
    let col = draw::with_alpha(draw::hex(c), a);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        q(col.r),
        q(col.g),
        q(col.b),
        q(col.a)
    )
}

/// `a` mixed toward `b` by `t`, as `#rrggbb`.
#[must_use]
pub fn mixed(a: &str, b: &str, t: f64) -> String {
    let c = draw::mix(draw::hex(a), draw::hex(b), t);
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b))
}

/// Make `n` give off light: a bloom on the GPU renderer, a soft glow from
/// the fallback — the only glow a lit thing needs.
#[must_use]
pub fn emit(n: Node, color: &str, intensity: f64, radius: f64) -> Node {
    draw::emissive(n, draw::hex(color), intensity, radius)
}

/// A lit lens: hot core, the colour, a dark rim, and its glow.
#[must_use]
pub fn lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let hot = [
        (0.0, "#ffffffee".to_string()),
        (0.2, mixed(color, "#ffffff", 0.55)),
        (0.55, color.to_string()),
        (1.0, mixed(color, "#000000", 0.5)),
    ];
    let stops: Vec<(f64, &str)> = hot.iter().map(|(p, c)| (*p, c.as_str())).collect();
    emit(
        draw::circle("lamp", cx, cy, r, draw::radial_at(0.38, 0.32, 0.9, &stops)),
        color,
        1.2,
        r * 3.0,
    )
}

/// An unlit lens.
#[must_use]
pub fn dark_lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let dark = [
        (0.0, "#ffffff22".to_string()),
        (0.5, mixed(color, "#000000", 0.62)),
        (1.0, "#000000d0".to_string()),
    ];
    let stops: Vec<(f64, &str)> = dark.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::with_effects(
        draw::circle("lens", cx, cy, r, draw::radial_at(0.38, 0.3, 0.95, &stops)),
        vec![draw::inner_shadow("#000000b0", 0.0, 0.0, r * 0.7)],
    )
}

/// A panel lamp in a black bezel, lit or not. Decoration, not a control.
pub fn lamp(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy): (f64, f64),
    d: f64,
    color: &str,
    lit: bool,
) {
    let r = d / 2.0;
    b.add(
        parent,
        draw::with_effects(
            draw::circle(
                "bezel",
                cx,
                cy,
                r + (d * 0.18).max(1.2),
                draw::linear(180.0, &[(0.0, "#3a3b3e"), (1.0, "#060607")]),
            ),
            vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)],
        ),
    );
    b.add(parent, dark_lens(cx, cy, r, color));
    if lit {
        b.add(parent, lens(cx, cy, r, color));
    }
}

/// A lamp that *is* a toggle: its lens lights while `addr` is on (a head
/// select, a pattern step). The control's box is `hit` square around it.
#[allow(clippy::too_many_arguments)]
pub fn lamp_toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
    d: f64,
    hit: f64,
    color: &str,
) -> NodeId {
    let root = b.add(
        parent,
        draw::frame(name, cx - hit / 2.0, cy - hit / 2.0, hit, hit),
    );
    let c = hit / 2.0;
    lamp(b, root, (c, c), d, color, false);
    let lit = b.add(root, lens(c, c, d / 2.0, color));
    let mut ctl = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, lit.0);
    ctl.binding = Some(bind(addr));
    b.controls.insert(root, ctl);
    root
}

// ── windows ──────────────────────────────────────────────────────────────

/// How a lit display reads: its glass, its phosphor, and how it glows.
#[derive(Clone, Copy)]
pub struct Screen<'a> {
    /// Glass top and bottom.
    pub glass: (&'a str, &'a str),
    /// The lit characters.
    pub ink: &'a str,
    /// Draw a VFD mesh over the glass.
    pub mesh: bool,
}

/// A dark display window (`x, y, w, h`), recessed, with an optional VFD
/// mesh. Returns the window frame, a parent for readouts.
pub fn screen(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    s: Screen<'_>,
) -> NodeId {
    // The bezel the glass sits in.
    let bezel = draw::rounded(
        draw::rect(
            "bezel",
            x - 5.0,
            y - 5.0,
            w + 10.0,
            h + 10.0,
            draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1c1d20")]),
        ),
        6.0,
    );
    b.add(
        parent,
        draw::with_effects(
            bezel,
            vec![
                draw::drop_shadow("#ffffff1a", 0.0, 1.0, 0.0),
                draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
            ],
        ),
    );
    let mut win = draw::frame(name, x, y, w, h);
    win.fills = vec![draw::linear(180.0, &[(0.0, s.glass.0), (1.0, s.glass.1)])];
    win.clips_content = true;
    let win = draw::with_effects(
        draw::rounded(win, 3.0),
        vec![draw::inner_shadow("#000000d0", 0.0, 2.0, 6.0)],
    );
    let root = b.add(parent, win);
    if s.mesh {
        let rows: Vec<Vec<(f64, f64)>> = (0..)
            .map(|i| 1.7 + 3.0 * f64::from(i))
            .take_while(|y| *y < h)
            .map(|y| vec![(0.0, y), (w, y)])
            .collect();
        let mut mesh = stroke_path("mesh", (0.0, 0.0, w, h), &rows, 0.45, "#000000", false);
        mesh.stroke.cap = StrokeCap::None;
        b.add(root, draw::with_opacity(mesh, 0.35));
    }
    root
}

/// A live readout in a window: the bound value, lit, left-, centre- or
/// right-aligned in the box (`x, cy, w`) at `size`; it gives off its own
/// light.
#[allow(clippy::too_many_arguments)]
pub fn readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    binding: Binding,
    (x, cy, w): (f64, f64, f64),
    size: f64,
    ink: &str,
    align: TextAlign,
) -> NodeId {
    let h = (size * 1.5).ceil();
    let root = b.add(parent, draw::frame(name, x, cy - h / 2.0, w, h));
    let lit = Ink::new(size, ink).weight(600.0).track(0.04).plain();
    let mut t = text_node("", 0.0, h / 2.0, w, &lit, align);
    t.name = "value".into();
    let value = b.add(root, emit(t, ink, 0.9, size * 0.8));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    b.controls.insert(root, c);
    root
}

/// A note-value picker on a display: the division the time locks to, lit,
/// with the step arrows beside it. A `Selector` over the division param —
/// wheel or arrow keys step it — whose `Value` slot is the lit text, printed
/// as the division's name.
#[allow(clippy::too_many_arguments)]
pub fn note_picker(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (x, cy, w): (f64, f64, f64),
    size: f64,
    ink: &str,
    frame: &str,
) -> NodeId {
    let h = (size * 1.6).ceil();
    let root = b.add(
        parent,
        draw::stroked(
            draw::rounded(draw::frame(name, x, cy - h / 2.0, w, h), 3.0),
            frame,
            1.0,
        ),
    );
    let lit_ink = Ink::new(size, ink).weight(600.0).track(0.04).plain();
    let mut t = text_node("", 0.0, h / 2.0, w - 10.0, &lit_ink, TextAlign::Center);
    t.name = "value".into();
    let value = b.add(root, emit(t, ink, 0.9, size * 0.8));
    let (ax, s) = (w - 8.0, size * 0.22);
    let up = draw::polygon(&[
        (ax - s, h / 2.0 - 1.5),
        (ax + s, h / 2.0 - 1.5),
        (ax, h / 2.0 - 1.5 - s * 1.2),
    ]);
    let down = draw::polygon(&[
        (ax - s, h / 2.0 + 1.5),
        (ax + s, h / 2.0 + 1.5),
        (ax, h / 2.0 + 1.5 + s * 1.2),
    ]);
    b.add(
        root,
        draw::path(
            "step arrows",
            0.0,
            0.0,
            w,
            h,
            format!("{up} {down}"),
            draw::solid(frame),
        ),
    );
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, value.0);
    c.binding = Some(bind(addr));
    c.format = Some(ValueFormat::labels(DIVISIONS));
    b.controls.insert(root, c);
    root
}

/// Lit legend text on a display (not bound): the fixed annunciators.
pub fn lit(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    (x, cy): (f64, f64),
    ink: Ink<'_>,
    align: TextAlign,
) -> NodeId {
    let w = width_for(s, &ink);
    let x0 = match align {
        TextAlign::Center => x - w / 2.0,
        TextAlign::Right => x - w,
        _ => x,
    };
    let t = text_node(s, x0, cy, w, &ink, align);
    b.add(parent, emit(t, ink.color, 0.6, ink.size))
}

// ── catalog entries ──────────────────────────────────────────────────────

/// A continuous param with a real range; `default` in real units.
#[must_use]
pub fn param(addr: &str, name: &str, range: Range, default: f64) -> ParamInfo {
    ParamInfo::new(addr, name, range).with_default_real(default)
}

/// 0–100 %.
#[must_use]
pub fn percent(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(
        addr,
        name,
        Range {
            steps: Some(101),
            ..Range::linear(0.0, 100.0, Unit::Percent)
        },
        default,
    )
}

/// A dB param over `min..max`.
#[must_use]
pub fn db(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    let p = param(addr, name, Range::linear(min, max, Unit::Db), default);
    if min < 0.0 && max > 0.0 {
        p.bipolar()
    } else {
        p
    }
}

/// A trim around the circuit as designed: −50 … +50 % of its travel, noon
/// = the voicing as it comes.
#[must_use]
pub fn trim(addr: &str, name: &str) -> ParamInfo {
    param(
        addr,
        name,
        Range {
            steps: Some(101),
            ..Range::linear(-50.0, 50.0, Unit::Percent)
        },
        0.0,
    )
    .bipolar()
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

/// A level stream of `channels`.
#[must_use]
pub fn level_stream(id: &str, name: &str, channels: u8) -> StreamInfo {
    StreamInfo {
        id: EntryId::new(id),
        name: name.into(),
        kind: StreamKind::Level { channels },
        rate_hz: 30.0,
    }
}

/// A param binding.
#[must_use]
pub fn bind(addr: &str) -> Binding {
    Binding::param(addr)
}

/// A stream binding.
#[must_use]
pub fn stream(id: &str) -> Binding {
    Binding::stream(id)
}

// ── time ─────────────────────────────────────────────────────────────────

/// The note values a synced time locks to, longest first — the same 21 as
/// `musical-time` (`1/1 … 1/64T`, straight / dotted / triplet).
pub const DIVISIONS: &[&str] = &[
    "1/1", "1/1D", "1/1T", "1/2", "1/2D", "1/2T", "1/4", "1/4D", "1/4T", "1/8", "1/8D", "1/8T",
    "1/16", "1/16D", "1/16T", "1/32", "1/32D", "1/32T", "1/64", "1/64D", "1/64T",
];
/// 1/8D: at 120 BPM exactly the 375 ms free-running default, so switching a
/// 120 BPM session to Sync changes nothing.
pub const DEFAULT_DIV: usize = 10;
/// The time mode switch's positions.
pub const TIME_MODES: &[&str] = &["Free", "Sync"];

/// A delay time in ms, swept logarithmically over what the line holds.
#[must_use]
pub fn time_ms(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    param(
        addr,
        name,
        Range::log(min, max, Unit::Milliseconds),
        default,
    )
}

/// Scale marks for a time knob over `min..max` ms (log).
#[must_use]
pub fn time_marks(min: f64, max: f64) -> Vec<(f64, &'static str)> {
    const NAMES: &[(f64, &str)] = &[
        (1.0, "1"),
        (2.0, "2"),
        (5.0, "5"),
        (10.0, "10"),
        (20.0, "20"),
        (25.0, "25"),
        (40.0, "40"),
        (50.0, "50"),
        (100.0, "100"),
        (200.0, "200"),
        (500.0, "500"),
        (800.0, "800"),
        (1000.0, "1k"),
        (1200.0, "1.2k"),
        (2000.0, "2k"),
        (4000.0, "4k"),
    ];
    // The two ends, and the decades between that don't crowd them.
    let mut v: Vec<(f64, &'static str)> = NAMES
        .iter()
        .filter(|(ms, _)| (*ms - min).abs() < 1e-9 || (*ms - max).abs() < 1e-9)
        .map(|(ms, l)| (log_pos(*ms, min, max), *l))
        .collect();
    for (ms, l) in [(10.0, "10"), (100.0, "100"), (1000.0, "1k")] {
        let p = log_pos(ms, min, max);
        if p > 0.14 && p < 0.86 {
            v.push((p, l));
        }
    }
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

/// 0 / 5 / 10 over a 0–100 % knob.
pub const PCT_MARKS: &[(f64, &str)] = &[(0.0, "0"), (0.5, "5"), (1.0, "10")];
/// Feedback's scale: 0, 50, 100 and the run-away past it.
pub const FB_MARKS: &[(f64, &str)] = &[
    (0.0, "0"),
    (50.0 / 110.0, "50"),
    (100.0 / 110.0, "100"),
    (1.0, ""),
];

/// Feedback, 0–110 %: past 100 the repeats run away, as the units do.
#[must_use]
pub fn feedback(addr: &str, name: &str, default: f64) -> ParamInfo {
    param(
        addr,
        name,
        Range {
            steps: Some(111),
            ..Range::linear(0.0, 110.0, Unit::Percent)
        },
        default,
    )
}
