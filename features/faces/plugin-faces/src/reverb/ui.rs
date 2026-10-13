//! Panel-building helpers for the reverb and modulation faces: placing kit
//! components by their shaft, silkscreen, stroked artwork, lit displays and
//! glowing readouts, the kit's LED meters and lamps in the face's own
//! colours, a stompbox enclosure, knobs printed in the face's ink, and the
//! catalog entries these faces declare.

use frame::document::proto::{
    Color, Emission, Node, NodeId, NodeKind, Paint, ShaderFx, Size, Stroke, StrokeCap, StrokeJoin,
    TextAlign, TextCase, Transform,
};
use frame::surface::kit::{Ink, Place, TOUCH_MIN, draw};
use frame::surface::proto::{
    Binding, Control, ControlKind, Curve, CurvePoint, EntryId, ParamInfo, ParamKind, Range,
    SlotRole, Source, StreamInfo, StreamKind, Unit, ValueFormat,
};
use frame::surface::widgets::Builder;

use crate::face::Face;

// ── placing ──────────────────────────────────────────────────────────────

/// Move `id` so its top-left corner is at (`x`, `y`) in its parent.
pub fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

/// The centre of `root`'s first child named one of `names`, in `root`'s
/// box — else the centre of the box.
fn anchor(b: &Builder<'_>, root: NodeId, names: &[&str]) -> (f64, f64) {
    let Some(n) = b.scene.node(root) else {
        return (0.0, 0.0);
    };
    for c in &n.children {
        if let Some(k) = b.scene.node(*c)
            && names.contains(&k.name.as_str())
        {
            return (
                k.transform.tx + k.size.width / 2.0,
                k.transform.ty + k.size.height / 2.0,
            );
        }
    }
    (n.size.width / 2.0, n.size.height / 2.0)
}

/// Place kit component `id` under `parent` with its shaft (a knob's `knob`,
/// a rotary switch's `dial`) — or the centre of its box — at (`cx`, `cy`).
///
/// # Panics
/// On an unknown component id or variant — faces are literals.
pub fn put(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, cx: f64, cy: f64) -> NodeId {
    put_by(b, parent, id, p, &["knob", "dial"], cx, cy)
}

/// Place kit component `id` with the centre of its child named one of
/// `names` (a footswitch's `cap`, a lamp's `bezel`) at (`cx`, `cy`).
///
/// # Panics
/// On an unknown component id or variant.
#[allow(clippy::too_many_arguments)]
pub fn put_by(
    b: &mut Builder<'_>,
    parent: NodeId,
    id: &str,
    p: &Place,
    names: &[&str],
    cx: f64,
    cy: f64,
) -> NodeId {
    let root = frame::surface::kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"));
    let (ax, ay) = anchor(b, root, names);
    move_to(b, root, cx - ax, cy - ay);
    root
}

/// Place kit component `id` with its box's top-left at (`x`, `y`).
///
/// # Panics
/// On an unknown component id or variant.
pub fn put_at(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place, x: f64, y: f64) -> NodeId {
    let root = frame::surface::kit::place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"));
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
        n.size = Size {
            width: w,
            height: h,
        };
    }
}

/// Take a placed control's legend (and its `Label` slot) off: the face
/// prints the legend itself, where the row's other legends are.
pub fn strip_legend(b: &mut Builder<'_>, control: NodeId) {
    let ids: Vec<NodeId> = b
        .controls
        .get(&control)
        .map(|c| c.slots_of(SlotRole::Label).map(NodeId).collect())
        .unwrap_or_default();
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
    }
    for id in ids {
        if b.scene.node(id).is_some() {
            let _ = b.scene.apply(frame::document::proto::Op::Remove { id });
        }
    }
}

/// A range whose evenly spaced printed marks are exactly `values` (a dial
/// printed unevenly — pre-delay's 0…250 ms, a plate's decay in seconds):
/// the control position follows the print, piecewise between marks.
///
/// # Panics
/// With fewer than two values.
#[must_use]
pub fn printed(values: &[f64], unit: Unit) -> Range {
    assert!(values.len() >= 2, "a printed scale needs two marks");
    let last = (values.len() - 1) as f64;
    let (lo, hi) = (values[0], values[values.len() - 1]);
    let points = values
        .iter()
        .enumerate()
        .map(|(i, v)| CurvePoint {
            x: i as f64 / last,
            y: (v - lo) / (hi - lo),
        })
        .collect();
    Range {
        min: lo,
        max: hi,
        scale: frame::surface::proto::Scale::Curve {
            curve: Curve::Points { points },
        },
        unit,
        steps: None,
    }
}

/// Add a live value readout (`Value` slot) under a placed control's box,
/// growing the box to hold it.
pub fn add_value(b: &mut Builder<'_>, control: NodeId, size: f64, color: &str) -> NodeId {
    let (w, h) = b
        .scene
        .node(control)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let mut t = text(
        "value",
        "",
        0.0,
        h + size * 0.72,
        w,
        size,
        color,
        TextAlign::Center,
        500.0,
        false,
    );
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.02;
    }
    let id = b.add(control, t);
    if let Some(n) = b.scene.node_mut(control) {
        n.size.height = h + (size * 1.5).ceil();
    }
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.push(frame::surface::proto::Slot {
            role: SlotRole::Value,
            node: id.0,
        });
    }
    id
}

// ── silkscreen ───────────────────────────────────────────────────────────

/// A text node whose box is `w` wide from `x`, vertically centred on `cy`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn text(
    name: &str,
    s: &str,
    x: f64,
    cy: f64,
    w: f64,
    size: f64,
    color: &str,
    align: TextAlign,
    weight: f64,
    upper: bool,
) -> Node {
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
fn text_w(s: &str, size: f64) -> f64 {
    s.chars().count() as f64 * size * 0.72
}

/// Silkscreen: uppercase, tracked, centred on (`cx`, `cy`).
#[allow(clippy::too_many_arguments)]
pub fn silk(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(
        parent,
        text(
            "silk",
            s,
            cx - w / 2.0,
            cy,
            w,
            size,
            color,
            TextAlign::Center,
            weight,
            true,
        ),
    )
}

/// Silkscreen set from its left edge.
#[allow(clippy::too_many_arguments)]
pub fn silk_left(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(
        parent,
        text(
            "silk",
            s,
            x,
            cy,
            w,
            size,
            color,
            TextAlign::Left,
            weight,
            true,
        ),
    )
}

/// Silkscreen set to end at `x`.
#[allow(clippy::too_many_arguments)]
pub fn silk_right(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(
        parent,
        text(
            "silk",
            s,
            x - w,
            cy,
            w,
            size,
            color,
            TextAlign::Right,
            weight,
            true,
        ),
    )
}

/// Mixed-case text (a model line in script-less caps is too loud).
#[allow(clippy::too_many_arguments)]
pub fn plain(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    color: &str,
    weight: f64,
) -> NodeId {
    let w = text_w(s, size) + size * 2.0;
    b.add(
        parent,
        text(
            "print",
            s,
            cx - w / 2.0,
            cy,
            w,
            size,
            color,
            TextAlign::Center,
            weight,
            false,
        ),
    )
}

/// A hairline rule.
pub fn rule(
    b: &mut Builder<'_>,
    parent: NodeId,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: &str,
) -> NodeId {
    b.add(parent, draw::rect("rule", x, y, w, h, draw::solid(color)))
}

/// A rounded outline box (a section frame).
#[allow(clippy::too_many_arguments)]
pub fn section(
    b: &mut Builder<'_>,
    parent: NodeId,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: &str,
    weight: f64,
) -> NodeId {
    b.add(
        parent,
        draw::stroked(
            draw::rounded(draw::frame("section", x, y, w, h), 6.0),
            color,
            weight,
        ),
    )
}

/// Print `labels` under a placed bank's keys (evenly across its box), at
/// `cy`: the bank's own caps then carry numerals.
#[allow(clippy::too_many_arguments)]
pub fn bank_labels(
    b: &mut Builder<'_>,
    parent: NodeId,
    bank: NodeId,
    labels: &[&str],
    cy: f64,
    size: f64,
    color: &str,
) {
    let (bx, bw) = b
        .scene
        .node(bank)
        .map_or((0.0, 0.0), |n| (n.transform.tx, n.size.width));
    let n = labels.len() as f64;
    for (i, l) in labels.iter().enumerate() {
        silk(
            b,
            parent,
            l,
            bx + bw * (i as f64 + 0.5) / n,
            cy,
            size,
            color,
            800.0,
        );
    }
}

/// Repaint a kit panel in paint the kit doesn't carry (an indigo, a
/// blue-black): its fill becomes `stops`, top to bottom; its finish stays.
pub fn repaint(b: &mut Builder<'_>, panel: NodeId, stops: &[(f64, &str)]) {
    if let Some(n) = b.scene.node_mut(panel) {
        n.fills = vec![draw::linear(178.0, stops)];
    }
}

// ── colour ───────────────────────────────────────────────────────────────

fn hex_of(c: Color) -> String {
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if c.a >= 1.0 {
        format!("#{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b))
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b), q(c.a))
    }
}

/// `#rrggbbaa` for `color` at alpha `a`.
#[must_use]
pub fn alpha(color: &str, a: f64) -> String {
    hex_of(draw::with_alpha(draw::hex(color), a.min(0.999)))
}

/// `a` mixed toward `b` by `t`, as `#rrggbb`.
#[must_use]
pub fn blend(a: &str, b: &str, t: f64) -> String {
    hex_of(Color {
        a: 1.0,
        ..draw::mix(draw::hex(a), draw::hex(b), t)
    })
}

// ── light ────────────────────────────────────────────────────────────────

/// Make `n` give off light in `color`: its `emission` (the GPU renderer
/// blooms it; the fallback draws a soft glow round its outline).
#[must_use]
pub fn glowing(mut n: Node, color: &str, intensity: f64, radius: f64) -> Node {
    // A stroke-only vector's light needs a (clear) fill to keep its path:
    // the effects stage recolours a node's paints for its emission and,
    // finding no fill, drops the geometry the stroke is drawn along — and
    // lights the node's box instead.
    if matches!(n.kind, NodeKind::Vector) && n.fills.is_empty() && !n.stroke.paints.is_empty() {
        n.fills = vec![draw::solid("#00000000")];
    }
    n.emission = Some(Emission {
        color: Color {
            a: 1.0,
            ..draw::hex(color)
        },
        intensity,
        radius,
    });
    n
}

/// A lit lens: hot core, the colour, a dark rim — and its light.
fn lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let hot = [
        (0.0, "#ffffffee".to_string()),
        (0.2, blend(color, "#ffffff", 0.55)),
        (0.55, color.to_string()),
        (1.0, blend(color, "#000000", 0.5)),
    ];
    let stops: Vec<(f64, &str)> = hot.iter().map(|(p, c)| (*p, c.as_str())).collect();
    glowing(
        draw::circle("lamp", cx, cy, r, draw::radial_at(0.38, 0.32, 0.9, &stops)),
        color,
        0.9,
        r * 3.0,
    )
}

/// An unlit lens in its bezel.
fn dark_lens(cx: f64, cy: f64, r: f64, color: &str) -> Node {
    let dark = [
        (0.0, "#ffffff22".to_string()),
        (0.5, blend(color, "#000000", 0.62)),
        (1.0, "#000000d0".to_string()),
    ];
    let stops: Vec<(f64, &str)> = dark.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::with_effects(
        draw::circle("lens", cx, cy, r, draw::radial_at(0.38, 0.3, 0.95, &stops)),
        vec![draw::inner_shadow("#000000b0", 0.0, 0.0, r * 0.7)],
    )
}

/// A panel lamp in a black bezel, lit or not (a pilot light). Decoration,
/// not a control.
pub fn lamp(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, d: f64, color: &str, lit: bool) {
    let r = d / 2.0;
    b.add(
        parent,
        draw::with_effects(
            draw::circle(
                "bezel",
                cx,
                cy,
                r + (d * 0.16).max(1.2),
                draw::linear(180.0, &[(0.0, "#2a2b2e"), (1.0, "#060607")]),
            ),
            vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)],
        ),
    );
    b.add(parent, dark_lens(cx, cy, r, color));
    if lit {
        b.add(parent, lens(cx, cy, r, color));
    }
}

/// A recessed readout window with a live `Value` slot — the number behind
/// a printed scale.
#[allow(clippy::too_many_arguments)]
pub fn readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    binding: Binding,
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    ink: &str,
) -> NodeId {
    let mut win = draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h);
    win.fills = vec![draw::linear(180.0, &[(0.0, "#07080a"), (1.0, "#121418")])];
    let win = draw::with_effects(
        draw::stroked(draw::rounded(win, 3.0), "#00000099", 1.0),
        vec![
            draw::inner_shadow("#000000c0", 0.0, 1.5, 3.0),
            draw::drop_shadow("#ffffff1c", 0.0, 1.0, 0.0),
        ],
    );
    let root = b.add(parent, win);
    let size = (h * 0.52).round();
    let value = b.add(
        root,
        glowing(
            text(
                "value",
                "",
                0.0,
                h / 2.0,
                w,
                size,
                ink,
                TextAlign::Center,
                600.0,
                false,
            ),
            ink,
            0.5,
            size * 0.3,
        ),
    );
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    b.controls.insert(root, c);
    root
}

// ── stroked artwork ──────────────────────────────────────────────────────

/// Open polylines stroked `weight` wide in `paint` (a ring repeats its
/// first point last and stays open, so no fill ever fills it), points in the parent's coordinates: one vector whose
/// box is the points' extent, the lines round-capped and round-joined.
#[must_use]
pub fn strokes(name: &str, lines: &[Vec<(f64, f64)>], weight: f64, paint: Paint) -> Node {
    let pts = lines.iter().flatten();
    let (x0, y0) = pts
        .clone()
        .fold((f64::INFINITY, f64::INFINITY), |(a, b), (x, y)| {
            (a.min(*x), b.min(*y))
        });
    let (x1, y1) = pts.fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |(a, b), (x, y)| {
        (a.max(*x), b.max(*y))
    });
    let (x0, y0) = if x0.is_finite() { (x0, y0) } else { (0.0, 0.0) };
    let mut d = String::new();
    for line in lines {
        for (i, (x, y)) in line.iter().enumerate() {
            d.push_str(&format!(
                "{} {:.3} {:.3} ",
                if i == 0 { "M" } else { "L" },
                x - x0,
                y - y0
            ));
        }
    }
    let mut n = Node::new(NodeKind::Vector, name);
    n.transform = Transform::translate(x0, y0);
    n.size = Size {
        width: (x1 - x0).max(1.0),
        height: (y1 - y0).max(1.0),
    };
    n.fill_geometry = vec![d];
    n.stroke = Stroke {
        paints: vec![paint],
        weight,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        ..Stroke::default()
    };
    n
}

/// Straight bars (a column per sample, a tap's spike): `lines` stroked
/// `weight` wide in `color`, square-ended.
#[must_use]
pub fn bars(name: &str, lines: &[Vec<(f64, f64)>], weight: f64, color: &str) -> Node {
    let mut n = strokes(name, lines, weight, draw::solid(color));
    n.stroke.cap = StrokeCap::None;
    n
}

/// A stroked polyline node, points in its parent's coordinates.
#[must_use]
pub fn line_node(name: &str, pts: &[(f64, f64)], weight: f64, color: &str) -> Node {
    strokes(name, &[pts.to_vec()], weight, draw::solid(color))
}

/// A stroked polyline in `parent`.
pub fn line(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    pts: &[(f64, f64)],
    weight: f64,
    color: &str,
) -> NodeId {
    b.add(parent, line_node(name, pts, weight, color))
}

/// A filled path in parent coordinates spanning the parent's `w × h`.
#[must_use]
pub fn area(name: &str, d: String, w: f64, h: f64, paint: Paint) -> Node {
    draw::path(name, 0.0, 0.0, w, h, d, paint)
}

/// The outline of an ellipse as a closed polyline.
#[must_use]
pub fn ellipse_pts(cx: f64, cy: f64, rx: f64, ry: f64, n: usize) -> Vec<(f64, f64)> {
    let mut pts: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect();
    pts.push(pts[0]);
    pts
}

// ── displays ─────────────────────────────────────────────────────────────

/// A shader effect with one `strength`.
fn shader(id: &str, strength: f64) -> Option<ShaderFx> {
    Some(ShaderFx {
        shader: id.into(),
        params: std::collections::BTreeMap::from([("strength".to_string(), strength)]),
        inputs: Vec::new(),
    })
}

/// A recessed glass display window (`x, y, w, h`) lit in `tint`: a bezel, a
/// near-black face with the faintest bloom of its phosphor. Returns the face
/// frame (clips its content) to draw into — its origin is the window's
/// top-left; finish with [`glass`].
///
/// `fx` names the optional shader over it (`crt` for a phosphor tube,
/// `glass` for a lens) — the display reads the same without it.
#[allow(clippy::too_many_arguments)]
pub fn display(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    tint: &str,
    fx: &str,
) -> NodeId {
    let bez = 5.0;
    let bezel = draw::rounded(
        draw::rect(
            "bezel",
            x - bez,
            y - bez,
            w + 2.0 * bez,
            h + 2.0 * bez,
            draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1c1d20")]),
        ),
        6.0,
    );
    b.add(
        parent,
        draw::with_effects(
            bezel,
            vec![
                draw::drop_shadow("#ffffff1f", 0.0, 1.0, 0.0),
                draw::inner_shadow("#000000", 0.0, 2.0, 3.0),
            ],
        ),
    );
    let mut face = draw::frame(name, x, y, w, h);
    face.fills = vec![draw::radial_at(
        0.5,
        0.45,
        0.75,
        &[(0.0, &blend(tint, "#050607", 0.88)), (1.0, "#040505")],
    )];
    face.clips_content = true;
    face.corner_radii = frame::document::proto::CornerRadii::uniform(3.0);
    face.effects = vec![draw::inner_shadow("#000000e0", 0.0, 2.0, 8.0)];
    face.shader = match fx {
        "crt" => shader("crt", 0.35),
        "glass" => shader("glass", 0.3),
        _ => None,
    };
    b.add(parent, face)
}

/// The glass over a [`display`], added last so it lies over what's drawn.
pub fn glass(b: &mut Builder<'_>, face: NodeId) {
    let (w, h) = b
        .scene
        .node(face)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let sheen = draw::linear(
        160.0,
        &[
            (0.0, "#ffffff1a"),
            (0.34, "#ffffff08"),
            (0.35, "#ffffff00"),
            (1.0, "#ffffff00"),
        ],
    );
    b.add(face, draw::rect("glass", 0.0, 0.0, w, h, sheen));
}

/// Text set in a display: glowing phosphor, mixed case.
#[allow(clippy::too_many_arguments)]
pub fn lit_text(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    w: f64,
    size: f64,
    color: &str,
    align: TextAlign,
    weight: f64,
) -> NodeId {
    let mut t = text("lit", s, x, cy, w, size, color, align, weight, false);
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.06;
    }
    b.add(parent, glowing(t, color, 0.6, size * 0.3))
}

/// A glowing live value in a display: a `Readout` whose `Value` text is lit
/// in `color`, in a box `w` wide from `x`, centred on `cy`.
#[allow(clippy::too_many_arguments)]
pub fn lit_readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    binding: Binding,
    x: f64,
    cy: f64,
    w: f64,
    size: f64,
    color: &str,
    align: TextAlign,
) -> NodeId {
    let h = (size * 1.5).ceil();
    let root = b.add(parent, draw::frame(name, x, cy - h / 2.0, w, h));
    let mut t = text(
        "value",
        "",
        0.0,
        h / 2.0,
        w,
        size,
        color,
        align,
        600.0,
        false,
    );
    if let NodeKind::Text(d) = &mut t.kind {
        d.letter_spacing = size * 0.04;
    }
    let value = b.add(root, glowing(t, color, 0.7, size * 0.3));
    let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, value.0);
    c.binding = Some(binding);
    b.controls.insert(root, c);
    root
}

// ── LED meters and lamps (the kit's, in the face's colours) ──────────────

/// One channel of a level stream.
fn channel(stream: &str, ch: u8) -> Binding {
    Binding {
        source: Source::Stream {
            id: EntryId::new(stream),
            channel: Some(ch),
        },
        ..Binding::stream(stream)
    }
}

/// A `meter.led_column` of `n` segments `thick` wide and `h` tall over
/// `floor..ceiling` dB, top-left at (`x`, `y`), on one channel of a level
/// stream.
#[allow(clippy::too_many_arguments)]
pub fn led_column(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    stream: &str,
    ch: u8,
    (x, y, thick, h): (f64, f64, f64, f64),
    n: usize,
    floor: f64,
    ceiling: f64,
) -> NodeId {
    let opts = [
        format!("floor={floor}"),
        format!("ceiling={ceiling}"),
        format!("segments={n}"),
        format!("thick={thick}"),
    ];
    let p = Place {
        options: Some(opts.to_vec()),
        ..Place::new(name).size(h).bind(channel(stream, ch))
    };
    put_at(b, parent, "meter.led_column", &p, x, y)
}

/// The kit option lighting lamp `i` of `n` in `colors(i)`.
fn lamp_colors(n: usize, colors: &dyn Fn(usize) -> String) -> String {
    format!(
        "colors={}",
        (0..n).map(colors).collect::<Vec<_>>().join(",")
    )
}

/// A `meter.led_bar` of `n` lamps lit left to right over `floor..ceiling`
/// dB by a level stream, `len × thick` from (`x`, `y`), lamp `i` in
/// `colors(i)`; `scale` printed under it.
#[allow(clippy::too_many_arguments)]
pub fn led_bar(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    stream: &str,
    (x, y, len, thick): (f64, f64, f64, f64),
    n: usize,
    (floor, ceiling): (f64, f64),
    scale: &[&str],
    ink: &str,
    colors: &dyn Fn(usize) -> String,
) -> NodeId {
    let opts = vec![
        format!("floor={floor}"),
        format!("ceiling={ceiling}"),
        format!("segments={n}"),
        format!("thick={thick}"),
        lamp_colors(n, colors),
    ];
    let p = Place {
        options: Some(opts),
        ..Place::new(name)
            .size(len)
            .bind(Binding::stream(stream))
            .scale(scale)
            .ink(Ink::Color(ink.into()))
            .keep_case()
    };
    put_at(b, parent, "meter.led_bar", &p, x, y)
}

/// A `meter.lamp` (`d` across the bezel) that brightens and dims with a
/// scalar stream, centred at (`cx`, `cy`), lit in `color`, its legend
/// under it in `ink`.
#[allow(clippy::too_many_arguments)]
pub fn stream_lamp(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    stream: &str,
    legend: &str,
    cx: f64,
    cy: f64,
    d: f64,
    color: &str,
    ink: &str,
) -> NodeId {
    let p = Place::new(name)
        .size(d)
        .bind(Binding::stream(stream))
        .options(&[&format!("color={color}")]);
    let lamp = put_by(b, parent, "meter.lamp", &p, &["bezel"], cx, cy);
    // Printed, not a `Label`: the lamp reads out the control it names.
    silk(b, parent, legend, cx, cy + d / 2.0 + 10.0, 7.5, ink, 800.0);
    lamp
}

/// A `switch.footswitch` on toggle param `addr` (`d` across the actuator,
/// centred at (`cx`, `cy`)), its status LED lit in `led`.
#[allow(clippy::too_many_arguments)]
pub fn stomp(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    cx: f64,
    cy: f64,
    d: f64,
    led: &str,
) -> NodeId {
    let p = Place::new(name)
        .size(d)
        .bind(bind(addr))
        .options(&[&format!("color={led}")]);
    put_by(b, parent, "switch.footswitch", &p, &["cap"], cx, cy)
}

/// An LFO stream: the modulator's output, 0..1, at a lamp's frame rate.
#[must_use]
pub fn lfo_stream(id: &str, name: &str) -> StreamInfo {
    StreamInfo {
        id: EntryId::new(id),
        name: name.into(),
        kind: StreamKind::Scalar {
            range: Range::linear(0.0, 1.0, Unit::None),
        },
        rate_hz: 60.0,
    }
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
        Range::linear(0.0, 100.0, Unit::Percent),
        default,
    )
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

/// How a panel prints a param of `unit`: whole percents and milliseconds,
/// a tenth of a dB; seconds, hertz and the rest as the range prints them.
fn format_for(unit: &Unit) -> Option<ValueFormat> {
    match unit {
        Unit::Percent | Unit::Milliseconds => Some(ValueFormat::decimals(0)),
        Unit::Db => Some(ValueFormat::decimals(1)),
        _ => None,
    }
}

/// Give every readout on `f` (knob values, fader values, display windows)
/// that has no format of its own the one its param's unit reads best in.
#[must_use]
pub fn formatted(mut f: Face) -> Face {
    let ids: Vec<NodeId> = f.scene.walk(f.scene.root);
    for id in ids {
        let Some(c) = f.scene.node_mut(id).and_then(|n| n.control.as_mut()) else {
            continue;
        };
        if c.format.is_some() || c.slots_of(SlotRole::Value).next().is_none() {
            continue;
        }
        let Some(Source::Param { addr }) = c.binding.as_ref().map(|b| &b.source) else {
            continue;
        };
        let Some(p) = f.params.iter().find(|p| &p.addr == addr) else {
            continue;
        };
        if p.kind == ParamKind::Continuous {
            c.format = format_for(&p.range.unit);
        }
    }
    f
}

// ── stompbox enclosure ───────────────────────────────────────────────────

/// The cast edge round an [`enclosure`]'s lid.
pub const EDGE: f64 = 9.0;

/// A die-cast stompbox enclosure `w × h` filling `root`, painted `paint`
/// (top, mid, bottom): a dark cast edge, the painted lid with a satin
/// sheen, four screws. Returns the lid (its origin is inset by the edge).
pub fn enclosure(
    b: &mut Builder<'_>,
    root: NodeId,
    w: f64,
    h: f64,
    paint: [&str; 3],
    screws: &str,
) -> NodeId {
    fit_root(b, root, w, h);
    let mut body = draw::frame("Enclosure", 0.0, 0.0, w, h);
    body.fills = vec![draw::linear(
        180.0,
        &[
            (0.0, &blend(paint[1], "#000000", 0.35)),
            (1.0, &blend(paint[2], "#000000", 0.6)),
        ],
    )];
    body.corner_radii = frame::document::proto::CornerRadii::uniform(22.0);
    let body = b.add(root, body);
    let e = EDGE;
    let mut lid = draw::frame("Lid", e, e, w - 2.0 * e, h - 2.0 * e);
    lid.fills = vec![
        draw::linear(172.0, &[(0.0, paint[0]), (0.5, paint[1]), (1.0, paint[2])]),
        draw::linear(
            115.0,
            &[
                (0.0, "#ffffff00"),
                (0.42, "#ffffff00"),
                (0.5, "#ffffff1a"),
                (0.58, "#ffffff00"),
                (1.0, "#ffffff00"),
            ],
        ),
    ];
    lid.corner_radii = frame::document::proto::CornerRadii::uniform(15.0);
    lid.effects = vec![
        draw::inner_shadow("#ffffff40", 0.0, 1.5, 1.0),
        draw::inner_shadow("#00000055", 0.0, -3.0, 6.0),
        draw::drop_shadow("#00000080", 0.0, 1.0, 2.0),
    ];
    let lid = b.add(body, lid);
    let (lw, lh) = (w - 2.0 * e, h - 2.0 * e);
    for (sx, sy) in [
        (13.0, 13.0),
        (lw - 13.0, 13.0),
        (13.0, lh - 13.0),
        (lw - 13.0, lh - 13.0),
    ] {
        let head = draw::circle(
            "screw",
            sx,
            sy,
            4.2,
            draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#f4f4f0"), (1.0, screws)]),
        );
        b.add(
            lid,
            draw::with_effects(head, vec![draw::inner_shadow("#00000080", 0.0, 0.0, 1.5)]),
        );
        b.add(
            lid,
            draw::rotated(
                draw::rect(
                    "slot",
                    sx - 2.6,
                    sy - 0.5,
                    5.2,
                    1.0,
                    draw::solid("#0000008c"),
                ),
                40.0,
            ),
        );
    }
    lid
}

// ── knobs in the face's ink ──────────────────────────────────────────────

/// How a knob (or rotary switch) is printed.
#[derive(Clone, Copy)]
pub struct Dial<'a> {
    /// Kit id (`knob.metal`, `switch.rotary`) and variant.
    pub style: &'a str,
    pub variant: &'a str,
    pub d: f64,
    /// Scale labels printed round it (empty = an unlabelled scale).
    pub scale: &'a [&'a str],
    /// A stepped control's positions, printed round it as its detents.
    pub options: &'a [&'a str],
    pub ink: &'a str,
    pub dim: &'a str,
    /// Show the live value under the dial.
    pub value: bool,
    /// Legend size (0 = the face prints its own).
    pub legend: f64,
    /// Legend centre line, absolute (0 = just under the dial).
    pub legend_y: f64,
    /// Travel in degrees (`None` = the style's own, or a stepped control's
    /// detent sweep).
    pub sweep: Option<f64>,
}

impl<'a> Dial<'a> {
    #[must_use]
    pub const fn new(style: &'a str, variant: &'a str, d: f64, ink: &'a str, dim: &'a str) -> Self {
        Self {
            style,
            variant,
            d,
            scale: &[],
            options: &[],
            ink,
            dim,
            value: true,
            legend: 9.0,
            legend_y: 0.0,
            sweep: None,
        }
    }
    #[must_use]
    pub const fn legend_at(mut self, y: f64) -> Self {
        self.legend_y = y;
        self
    }
    #[must_use]
    pub const fn sweep(mut self, deg: f64) -> Self {
        self.sweep = Some(deg);
        self
    }
    #[must_use]
    pub const fn scale(mut self, s: &'a [&'a str]) -> Self {
        self.scale = s;
        self
    }
    #[must_use]
    pub const fn options(mut self, o: &'a [&'a str]) -> Self {
        self.options = o;
        self.value = false;
        self
    }
    #[must_use]
    pub const fn legend(mut self, size: f64) -> Self {
        self.legend = size;
        self
    }
}

/// Place a kit knob (or rotary switch) with its shaft at (`cx`, `cy`),
/// printed in the face's ink, its box tight round the moving part; the
/// live value just under it and the legend below that, in the face's inks.
pub fn dial(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    legend: &str,
    cx: f64,
    cy: f64,
    s: &Dial<'_>,
) -> NodeId {
    let mut p = Place::new(legend)
        .size(s.d)
        .variant(s.variant)
        .bind(bind(addr))
        .ink(Ink::Color(s.ink.into()))
        .keep_case()
        .touch(TOUCH_MIN);
    p = if s.options.is_empty() {
        p.scale(s.scale)
    } else {
        p.options(s.options)
    };
    if let Some(sw) = s.sweep {
        p = p.sweep(sw);
    }
    let k = put(b, parent, s.style, &p, cx, cy);
    strip_legend(b, k);
    if s.value {
        add_value(b, k, (s.d * 0.14).clamp(7.5, 9.0), s.dim);
    }
    if s.legend > 0.0 {
        let below = if s.value {
            s.d / 2.0 + 24.0
        } else {
            s.d / 2.0 + 14.0
        };
        let y = if s.legend_y > 0.0 {
            s.legend_y
        } else {
            cy + below + s.legend * 0.4
        };
        silk(b, parent, legend, cx, y, s.legend, s.ink, 800.0);
    }
    k
}

/// Square off a face's panel so it sits flush in its host's box, edge to
/// edge against the next module: every child of `root` covering the whole
/// `w × h` loses its rounded corners and its outline. A host that frames
/// the face already draws the edge; a second, rounded one inside it reads
/// as a card in a card.
pub fn flush(b: &mut Builder<'_>, root: NodeId, w: f64, h: f64) {
    let kids = b
        .scene
        .node(root)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    for id in kids {
        if let Some(n) = b.scene.node_mut(id) {
            let whole = n.transform.tx.abs() < 1.0
                && n.transform.ty.abs() < 1.0
                && (n.size.width - w).abs() < 1.0
                && (n.size.height - h).abs() < 1.0;
            if whole {
                n.corner_radii = frame::document::proto::CornerRadii::default();
                n.stroke.paints.clear();
                n.stroke.weight = 0.0;
            }
        }
    }
}
