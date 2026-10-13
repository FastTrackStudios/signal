//! What the amp-suite recreations share: the pedal, amp-head, cab and
//! tabletop scaffolds, placing kit parts by their centre, the silkscreen
//! and the catalog entries a unit binds.
//!
//! Forms follow `docs/form-factors.md`: pedals are one height (286 design
//! px) and a class width (`panel.pedal`); treadles 241 × 600; amp heads
//! 1200 × 510-ish (the plugin's faceplate aspect); cabs 900 × 900;
//! tabletop EQ units at their render's aspect. The view system isn't built
//! yet, so every unit is one page of its own.

use document_proto::{Node, NodeId, NodeKind, TextAlign, TextCase, TextData};
use surface_kit::{Ink as KitInk, Place, TOUCH_MIN, draw};
use surface_proto::{ParamInfo, Range, SlotRole, Unit};
use surface_widgets::Builder;

use crate::delay::parts::{self, Ink};

pub mod lcd;
pub mod lettering;
pub use lettering::{Letter, letter, letter_aligned, letter_fit};

pub use crate::delay::parts::{bind, choice, hexa, mixed, percent, toggle};

/// The pedal height every class shares.
pub const PEDAL_H: f64 = 286.0;

/// The top of a pedal's footswitch band (the bottom 30 %).
pub const BAND: f64 = PEDAL_H * 0.7;

/// How a pedal's enclosure looks.
#[derive(Clone, Copy)]
pub struct Enclosure<'a> {
    /// `panel.pedal` class: `micro`, `nano`, `standard`, `compact`, `wide`,
    /// `medium`, `large`, `bigbox`, `treadle`.
    pub class: &'a str,
    pub color: &'a str,
    /// `powder`, `brushed`, `satin`, `gloss`.
    pub finish: &'a str,
    /// The base under the lid (`None` = the paint, darker).
    pub base: Option<&'a str>,
    /// `side`, `top`, `both`, `none`.
    pub jacks: &'a str,
    /// Corner radius (`None` = the kit's).
    pub radius: Option<f64>,
}

impl<'a> Enclosure<'a> {
    #[must_use]
    pub const fn new(class: &'a str, color: &'a str, finish: &'a str) -> Self {
        Self {
            class,
            color,
            finish,
            base: None,
            jacks: "side",
            radius: None,
        }
    }
    #[must_use]
    pub const fn base(mut self, c: &'a str) -> Self {
        self.base = Some(c);
        self
    }
    #[must_use]
    pub const fn jacks(mut self, j: &'a str) -> Self {
        self.jacks = j;
        self
    }
    #[must_use]
    pub const fn radius(mut self, r: f64) -> Self {
        self.radius = Some(r);
        self
    }
    /// The class's footprint.
    #[must_use]
    pub fn size(&self) -> (f64, f64) {
        surface_kit::panels::pedal::class_size(self.class)
    }
}

/// Size `root` to the class and place the enclosure; returns the panel
/// frame (the whole footprint — place parts in its coordinates).
pub fn pedal_body(b: &mut Builder<'_>, root: NodeId, e: &Enclosure<'_>) -> NodeId {
    let (w, h) = e.size();
    parts::fit_root(b, root, w, h);
    let mut opts = vec![
        format!("color={}", e.color),
        format!("finish={}", e.finish),
        format!("jacks={}", e.jacks),
    ];
    if let Some(base) = e.base {
        opts.push(format!("base={base}"));
    }
    if let Some(r) = e.radius {
        opts.push(format!("radius={r}"));
    }
    let p = Place {
        options: Some(opts),
        ..Place::new("Enclosure").variant(e.class)
    };
    parts::put_at(b, root, "panel.pedal_3d", &p, 0.0, 0.0)
}

/// A knob style as a unit uses it.
#[derive(Clone, Copy)]
pub struct KnobStyle<'a> {
    pub id: &'a str,
    pub variant: &'a str,
    pub d: f64,
    /// Extra `Place::options` (`dots=21`).
    pub opts: &'a [&'a str],
    /// Panel ink for anything the kit prints round it.
    pub ink: &'a str,
    /// The hit box's least edge (default the lint's touch minimum).
    pub touch: f64,
}

impl<'a> KnobStyle<'a> {
    #[must_use]
    pub const fn new(id: &'a str, variant: &'a str, d: f64, ink: &'a str) -> Self {
        Self {
            id,
            variant,
            d,
            opts: &[],
            ink,
            touch: TOUCH_MIN,
        }
    }
    #[must_use]
    pub const fn opts(mut self, o: &'a [&'a str]) -> Self {
        self.opts = o;
        self
    }
    #[must_use]
    pub const fn touch(mut self, t: f64) -> Self {
        self.touch = t;
        self
    }
    #[must_use]
    pub const fn size(mut self, d: f64) -> Self {
        self.d = d;
        self
    }
}

/// A knob with its shaft at (`cx`, `cy`), a tight box, the kit's legend
/// off and the unit's own legend printed centred at `legend_y` (as the
/// control's `Label`). An empty `legend` prints nothing (the control is
/// still named).
#[allow(clippy::too_many_arguments)]
pub fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &KnobStyle<'_>,
    addr: &str,
    legend: &str,
    (cx, cy): (f64, f64),
    legend_y: f64,
    ink: Ink<'_>,
) -> NodeId {
    let mut p = Place::new(if legend.is_empty() { addr } else { legend })
        .size(s.d)
        .bind(bind(addr))
        .touch(s.touch)
        .ink(KitInk::Color(s.ink.into()));
    if !s.variant.is_empty() {
        p = p.variant(s.variant);
    }
    if !s.opts.is_empty() {
        p.options = Some(s.opts.iter().map(ToString::to_string).collect());
    }
    let k = parts::put(b, parent, s.id, &p, cx, cy);
    parts::strip_legend(b, k);
    if !legend.is_empty() {
        parts::label_at(b, k, legend, (cx, legend_y), ink);
    }
    k
}

/// [`knob`] with its legend cut as lettering at the ink's real weight.
#[allow(clippy::too_many_arguments)]
pub fn knob_lt(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &KnobStyle<'_>,
    addr: &str,
    legend: &str,
    at: (f64, f64),
    legend_y: f64,
    ink: Ink<'_>,
) -> NodeId {
    let k = knob(b, parent, s, addr, "", at, legend_y, ink);
    if !legend.is_empty() {
        label(b, k, legend, (at.0, legend_y), ink);
    }
    k
}

/// A control's printed legend, cut as lettering at the ink's real weight,
/// centred on panel (`cx`, `cy`) (the control's parent's coordinates), as
/// its `Label` slot.
pub fn label(
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
    let text = if ink.upper {
        s.to_uppercase()
    } else {
        s.to_string()
    };
    let id = letter(
        b,
        control,
        &text,
        (cx - tx, cy - ty),
        Letter::new(ink.size, ink.weight, ink.color).track(ink.track),
    );
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
        c.slots.push(surface_proto::Slot {
            role: SlotRole::Label,
            node: id.0,
        });
    }
    id
}

/// [`knob`] with its legend set in display lettering `l` (capitals).
#[allow(clippy::too_many_arguments)]
pub fn knob_l(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &KnobStyle<'_>,
    addr: &str,
    legend: &str,
    at: (f64, f64),
    legend_y: f64,
    l: Letter<'_>,
) -> NodeId {
    let k = knob(
        b,
        parent,
        s,
        addr,
        "",
        at,
        legend_y,
        Ink::new(l.size, l.color),
    );
    if !legend.is_empty() {
        label_l(b, k, &legend.to_uppercase(), (at.0, legend_y), l);
    }
    k
}

/// A control's printed legend in display lettering `l`, centred on panel
/// (`cx`, `cy`), as its `Label` slot.
pub fn label_l(
    b: &mut Builder<'_>,
    control: NodeId,
    s: &str,
    (cx, cy): (f64, f64),
    l: Letter<'_>,
) -> NodeId {
    let (tx, ty) = b
        .scene
        .node(control)
        .map_or((0.0, 0.0), |n| (n.transform.tx, n.transform.ty));
    let id = letter(b, control, s, (cx - tx, cy - ty), l);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
        c.slots.push(surface_proto::Slot {
            role: SlotRole::Label,
            node: id.0,
        });
    }
    id
}

/// Lettering from a silkscreen `Ink` (its size, weight, tracking, case),
/// centred on (`cx`, `cy`).
pub fn print(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    (cx, cy): (f64, f64),
    ink: Ink<'_>,
) -> NodeId {
    let text = if ink.upper {
        s.to_uppercase()
    } else {
        s.to_string()
    };
    letter(
        b,
        parent,
        &text,
        (cx, cy),
        Letter::new(ink.size, ink.weight, ink.color).track(ink.track),
    )
}

/// The stomp switch, its actuator centred on (`cx`, `cy`), its LED `led`
/// (dx, dy) from it. `style` is the LED's `bezel` or `dot`.
#[allow(clippy::too_many_arguments)]
pub fn stomp(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
    d: f64,
    led_color: &str,
    (dx, dy): (f64, f64),
    led_size: f64,
    style: &str,
    momentary: bool,
) -> NodeId {
    let mut opts = vec![
        format!("color={led_color}"),
        format!("led_dx={dx}"),
        format!("led_dy={dy}"),
        format!("led_size={led_size}"),
        format!("led_style={style}"),
    ];
    if momentary {
        opts.push("momentary".into());
    }
    let p = Place {
        options: Some(opts),
        ..Place::new(name).size(d).bind(bind(addr)).touch(TOUCH_MIN)
    };
    let root = surface_kit::place(b, parent, "switch.stomp", &p).unwrap_or_else(|e| panic!("{e}"));
    // Centre the actuator (the `cap`) on the point.
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
    let (ax, ay) = cap.unwrap_or((0.0, 0.0));
    parts::move_to(b, root, cx - ax, cy - ay);
    root
}

/// A mini toggle (`switch.toggle_nut`, chrome bat) centred at (`cx`,
/// `cy`), size = its nut; the unit prints its positions.
pub fn mini_toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
    size: f64,
    variant: &str,
) -> NodeId {
    let p = Place::new(name)
        .size(size)
        .variant(variant)
        .bind(bind(addr))
        .legend("")
        .touch(TOUCH_MIN);
    let t = parts::put(b, parent, "switch.toggle_nut", &p, cx, cy);
    parts::strip_legend(b, t);
    t
}

/// A text node set in `ink`, `w` wide from `x`, centred on `cy`, with a
/// custom font weight and case; `italic` slants it (a synthetic oblique
/// via the text's own skew is not available — Inter Italic is used).
#[must_use]
pub fn text(s: &str, x: f64, cy: f64, w: f64, ink: &Ink<'_>, align: TextAlign) -> Node {
    parts::text_node(s, x, cy, w, ink, align)
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
    parts::silk(b, parent, s, cx, cy, ink)
}

/// Lettering that lights (an LED readout, a lit logo).
pub fn lit_text(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    (cx, cy): (f64, f64),
    ink: Ink<'_>,
    glow: f64,
) -> NodeId {
    let w = s.chars().count() as f64 * ink.size * (0.72 + ink.track) + ink.size * 2.0;
    let t = parts::text_node(s, cx - w / 2.0, cy, w, &ink, TextAlign::Center);
    b.add(parent, parts::emit(t, ink.color, glow, ink.size))
}

/// A text node's `TextData`, for restyling after placing.
pub fn restyle(b: &mut Builder<'_>, id: NodeId, f: impl FnOnce(&mut TextData)) {
    if let Some(n) = b.scene.node_mut(id)
        && let NodeKind::Text(t) = &mut n.kind
    {
        f(t);
    }
}

/// Set a text node in capitals or as written.
pub fn case(b: &mut Builder<'_>, id: NodeId, upper: bool) {
    restyle(b, id, |t| {
        t.text_case = if upper {
            TextCase::Upper
        } else {
            TextCase::Original
        }
    });
}

/// A filled vector path in `parent`'s coordinates over its `(w, h)`.
pub fn shape(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (w, h): (f64, f64),
    d: String,
    color: &str,
) -> NodeId {
    b.add(
        parent,
        draw::path(name, 0.0, 0.0, w, h, d, draw::solid(color)),
    )
}

/// A stroked open polyline.
pub fn line(
    b: &mut Builder<'_>,
    parent: NodeId,
    pts: &[(f64, f64)],
    weight: f64,
    color: &str,
) -> NodeId {
    parts::line(b, parent, "line", pts, weight, color)
}

/// A smooth stroked curve through `pts` (Catmull-Rom as cubic Béziers),
/// in `parent`'s `(w, h)` box.
pub fn curve(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (w, h): (f64, f64),
    pts: &[(f64, f64)],
    weight: f64,
    color: &str,
) -> NodeId {
    let mut d = String::new();
    if let Some((x, y)) = pts.first() {
        d.push_str(&format!("M {x:.2} {y:.2} "));
    }
    for i in 0..pts.len().saturating_sub(1) {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(pts.len() - 1)];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        d.push_str(&format!(
            "C {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} ",
            c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
        ));
    }
    let mut n = draw::path(name, 0.0, 0.0, w, h, d, draw::solid(color));
    n.fills = Vec::new();
    n.stroke = document_proto::Stroke {
        paints: vec![draw::solid(color)],
        weight,
        cap: document_proto::StrokeCap::Round,
        join: document_proto::StrokeJoin::Round,
        align: document_proto::StrokeAlign::Center,
        ..document_proto::Stroke::default()
    };
    b.add(parent, n)
}

/// An LED with no control of its own, lit (decoration: a meter's lamp, a
/// status dot on a display).
pub fn led(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy): (f64, f64),
    d: f64,
    color: &str,
    lit: bool,
) -> NodeId {
    if lit {
        b.add(parent, parts::lens(cx, cy, d / 2.0, color))
    } else {
        b.add(parent, parts::dark_lens(cx, cy, d / 2.0, color))
    }
}

/// Give a placed control's `Needle`/`Label` less say: the node the
/// control's `Label` slot names.
#[must_use]
pub fn label_of(b: &Builder<'_>, control: NodeId) -> Option<NodeId> {
    b.controls
        .get(&control)
        .and_then(|c| c.slots_of(SlotRole::Label).next())
        .map(NodeId)
}

// ── catalog entries ──────────────────────────────────────────────────────

/// An amp's 0–10 knob: 0 … 10 in tenths, `default` on that scale.
#[must_use]
pub fn dial10(addr: &str, name: &str, default: f64) -> ParamInfo {
    parts::param(
        addr,
        name,
        Range {
            steps: Some(101),
            ..Range::linear(0.0, 10.0, Unit::None)
        },
        default,
    )
}

/// A dB param.
#[must_use]
pub fn db(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    parts::db(addr, name, min, max, default)
}

/// A time in ms, log.
#[must_use]
pub fn ms(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    parts::time_ms(addr, name, min, max, default)
}

/// A frequency in Hz, log.
#[must_use]
pub fn hz(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    parts::param(addr, name, Range::log(min, max, Unit::Hz), default)
}

/// A rate in Hz, log.
#[must_use]
pub fn rate(addr: &str, name: &str, min: f64, max: f64, default: f64) -> ParamInfo {
    hz(addr, name, min, max, default)
}

/// Musical divisions.
pub use crate::delay::parts::{DEFAULT_DIV, DIVISIONS};

/// A gain-reduction stream.
#[must_use]
pub fn gr_stream(id: &str, name: &str) -> surface_proto::StreamInfo {
    surface_proto::StreamInfo {
        id: surface_proto::EntryId::new(id),
        name: name.into(),
        kind: surface_proto::StreamKind::GainReduction,
        rate_hz: 30.0,
    }
}

/// A level stream.
#[must_use]
pub fn level_stream(id: &str, name: &str, channels: u8) -> surface_proto::StreamInfo {
    parts::level_stream(id, name, channels)
}

/// A row of round LEDs lit as gain reduction deepens, one per threshold
/// (dB of reduction, shallowest first), laid out left to right over
/// `x0..x1` at `cy`; `rtl` lights them from the right (the shallowest
/// lamp at the right end). Unbound decoration round them is the face's.
#[allow(clippy::too_many_arguments)]
pub fn gr_dots(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    stream: &str,
    (x0, x1, cy): (f64, f64, f64),
    d: f64,
    thresholds: &[f64],
    color: &str,
    rtl: bool,
) -> NodeId {
    use surface_proto::{Control, ControlKind, Curve, CurvePoint};
    let n = thresholds.len().max(1);
    let pitch = (x1 - x0) / n as f64;
    let h = d + 4.0;
    let root = b.add(parent, draw::frame(name, x0, cy - h / 2.0, x1 - x0, h));
    let deepest = thresholds.iter().copied().fold(1.0, f64::max);
    let (floor, ceiling) = (0.0, -deepest);
    let mut pts = vec![CurvePoint { x: 0.0, y: 0.0 }];
    for (i, t) in thresholds.iter().enumerate() {
        pts.push(CurvePoint {
            x: (t / deepest).clamp(0.0, 1.0),
            y: (i as f64 + 1.0) / n as f64,
        });
    }
    pts.sort_by(|a, b| a.x.total_cmp(&b.x));
    pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9);
    let mut binding = surface_proto::Binding::stream(stream);
    binding.curve = Curve::Points { points: pts };
    let mut c = Control::new(ControlKind::Gauge {
        sweep_degrees: 0.0,
        floor_db: floor,
        ceiling_db: ceiling,
    });
    c.binding = Some(binding);
    for i in 0..n {
        let slot = if rtl { n - 1 - i } else { i };
        let cx = pitch * (slot as f64 + 0.5);
        b.add(root, parts::dark_lens(cx, h / 2.0, d / 2.0, color));
        let lit = b.add(root, parts::lens(cx, h / 2.0, d / 2.0, color));
        c = c.slot(SlotRole::Led, lit.0);
    }
    b.controls.insert(root, c);
    root
}
