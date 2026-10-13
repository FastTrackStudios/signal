//! Kit knobs: the valve-era mouldings of the tube compressors — shapes the
//! round-tier [`KnobSpec`] can't draw.
//!
//! Each style is the shared knob builder ([`super::build`]: the printed
//! scale, the legend, the control, the tight box) given a bare spec — no
//! tiers, only its index line — and then dressed: the moulding's turning
//! parts go into the body *under* the index, and the room's light over it
//! as fixed overlays. So the moulding turns as one piece with its paint
//! kept rotation-invariant (flat or radial about the shaft), while the
//! gloss and the shading stay where the room puts them (see `KIT.md`).
//!
//! - `knob.tab_pointer` — a glossy black round knob with a square-ended
//!   tab moulded on its tail and a white line on top: the grey opto
//!   leveler's GAIN / PEAK REDUCTION / meter knob.
//! - `knob.heptagon` — a glossy black seven-sided grip on a round skirt,
//!   a flat top and a white line: the blue opto compressor's big knobs.
//! - `knob.bar_pointer` — a round black hub under a long tapered bar with
//!   a white line down its pointing end: the blue unit's selectors.
//! - `knob.fluted_stack` — chunky phenolic: a finely scalloped skirt, a
//!   stepped body and a dome, read by a white chevron on the skirt and a
//!   dot on the body: the big twin-valve limiter's gain and threshold.
//! - `knob.paddle` — a domed cap with a broad paddle tail and a white
//!   chevron at its nose: the twin-valve limiter's metering and time
//!   constant selectors.
//! - `knob.fin` — a matte black disc with a raised fin across it and a
//!   white line at one end of the fin: the modern vari-mu's knobs.

use crate::document::proto::{Color, Effect, EffectKind, Node, NodeId, NodeKind, Op, Paint};
use crate::surface::widgets::Builder;

use super::spec::{BODY_R, Index, KnobSpec, RingKind};
use super::{
    BLACK, Geo, WHITE, circle_path, css, fmt_pts, hex_of, refs, room_light, rrect, scallop, vector,
};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// The white of a painted index.
const LINE: &str = "#f1efe9";

/// A bare spec: the shared builder draws only the index line and the scale.
const fn bare(
    index: Index,
    ring: RingKind,
    ring_offset: f64,
    scale: &'static [&'static str],
    variants: &'static [&'static str],
    tints: &'static [Option<&'static str>],
) -> KnobSpec {
    KnobSpec {
        tiers: &[],
        index,
        collar_index: None,
        flutes: None,
        specular: None,
        ring_offset,
        hub: None,
        sweep: 300.0,
        ring,
        scale,
        variants,
        tints,
    }
}

/// No index of the builder's own (the style paints its own).
const NO_LINE: Index = Index::Bar {
    from: 0.0,
    to: 0.0,
    width: 0.0,
    color: "#00000000",
};

const ZERO_TEN: &[&str] = &["0", "2", "4", "6", "8", "10"];
const BLACKS: &[&str] = &["black"];
const BLACK_TINT: &[Option<&str>] = &[None];

static TAB_POINTER: KnobSpec = bare(
    Index::Bar {
        from: 0.10,
        to: 0.94,
        width: 2.4,
        color: LINE,
    },
    RingKind::Numerals,
    7.0,
    ZERO_TEN,
    &["black", "brown"],
    &[None, Some("#2e2119")],
);
static HEPTAGON: KnobSpec = bare(
    Index::Bar {
        from: 0.08,
        to: 0.90,
        width: 2.6,
        color: LINE,
    },
    RingKind::Dots,
    2.0,
    &["0", "", "", "", "", "10"],
    BLACKS,
    BLACK_TINT,
);
static BAR_POINTER: KnobSpec = bare(
    Index::Bar {
        from: 0.06,
        to: 0.86,
        width: 2.2,
        color: LINE,
    },
    RingKind::Ticks,
    4.0,
    &["1", "2", "3"],
    BLACKS,
    BLACK_TINT,
);
static FLUTED_STACK: KnobSpec = bare(
    NO_LINE,
    RingKind::Numerals,
    2.0,
    &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    BLACKS,
    BLACK_TINT,
);
static PADDLE: KnobSpec = bare(
    NO_LINE,
    RingKind::Ticks,
    6.0,
    &["1", "2", "3", "4", "5", "6"],
    BLACKS,
    BLACK_TINT,
);
static FIN: KnobSpec = bare(
    Index::Bar {
        from: 0.52,
        to: 0.94,
        width: 2.2,
        color: LINE,
    },
    RingKind::Ticks,
    0.0,
    &["", "", "", "", "", "", "", "", "", "", ""],
    BLACKS,
    BLACK_TINT,
);

type Dress = fn(&mut Parts<'_, '_>);

macro_rules! valve_knobs {
    ($( $fn:ident => $spec:ident, $dress:ident, $id:literal, $title:literal, $size:literal, $desc:literal; )*) => {
        $(
            fn $fn(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
                dressed(b, parent, p, &$spec, $title, $size, $dress)
            }
        )*

        /// These styles' components.
        pub(crate) fn defs() -> Vec<ComponentDef> {
            vec![$(
                ComponentDef {
                    id: $id,
                    family: Family::Knob,
                    title: $title,
                    description: $desc,
                    variants: $spec.variants,
                    default_size: $size,
                    binds: "param",
                    build: $fn,
                },
            )*]
        }
    };
}

valve_knobs! {
    tab_pointer => TAB_POINTER, dress_tab, "knob.tab_pointer", "Tab pointer", 56.0,
        "Glossy black round knob with a square-ended tab moulded on its tail and a white line across the top — the grey optical leveling amplifier's knob. For a big gain-style control read against numerals printed on a light panel.";
    heptagon => HEPTAGON, dress_heptagon, "knob.heptagon", "Heptagon", 64.0,
        "Glossy black seven-sided grip standing on a round skirt, with a flat top and a white line from the centre to a corner — the blue Scandinavian optical compressor's knob. Prints a dot per step with numerals outside.";
    bar_pointer => BAR_POINTER, dress_bar, "knob.bar_pointer", "Bar pointer", 48.0,
        "A round black hub under a long tapered black bar, a white line down the bar's pointing end — the selector on the blue optical compressor (meter, timing, sidechain). Reads as a switch: use it for stepped choices.";
    fluted_stack => FLUTED_STACK, dress_fluted_stack, "knob.fluted_stack", "Fluted stack", 72.0,
        "Chunky 1950s phenolic: a wide finely scalloped skirt, a stepped round body and a dome, read by a white chevron painted on the skirt and a white dot on the body — the big twin-channel valve limiter's input gain and threshold.";
    paddle => PADDLE, dress_paddle, "knob.paddle", "Paddle", 52.0,
        "A domed black cap with a broad paddle moulded on its tail and a white chevron at its nose — the valve limiter's metering and time-constant selectors. Use it for stepped choices on dark, vintage panels.";
    fin => FIN, dress_fin, "knob.fin", "Fin", 56.0,
        "Matte black disc with a raised fin moulded straight across it and a white line at the fin's pointing end — the modern vari-mu's knob, inside a ring of white ticks. For any continuous control on a dark sub-panel.";
}

/// Where a dressing puts its parts.
struct Parts<'a, 'b> {
    b: &'a mut Builder<'b>,
    /// The fixed knob frame (the room's light goes here, over the body).
    knob: NodeId,
    /// The turning body; moulding goes in under the index.
    body: NodeId,
    g: Geo,
    /// The variant's tint, if any (the moulding's black otherwise).
    tint: Option<Color>,
    /// How many moulding parts are already in under the index.
    under: u32,
}

impl Parts<'_, '_> {
    fn side(&self) -> f64 {
        self.g.size()
    }
    /// Units → px, about the shaft.
    fn pt(&self, u: f64, v: f64) -> (f64, f64) {
        self.g.p(u, v)
    }
    fn poly(&self, pts: &[(f64, f64)]) -> String {
        fmt_pts(&pts.iter().map(|(u, v)| self.pt(*u, *v)).collect::<Vec<_>>())
    }
    fn circle(&self, u: f64, v: f64, r: f64) -> String {
        let (x, y) = self.pt(u, v);
        circle_path(x, y, r * self.g.k)
    }
    /// A turning part, under the index line.
    fn turn(&mut self, n: Node) {
        self.b
            .scene
            .apply(Op::Insert {
                node: n,
                parent: self.body,
                index: self.under,
            })
            .expect("body exists");
        self.under += 1;
    }
    /// A fixed part over the body (light, gloss).
    fn fix(&mut self, n: Node) {
        self.b.add(self.knob, n);
    }
    /// A turning path.
    fn part(&mut self, name: &str, d: String, fills: Vec<Paint>) {
        let s = self.side();
        self.turn(vector(name, 0.0, 0.0, s, s, d, fills));
    }
    /// A turning part's soft contact shadow round its real outline — no
    /// offset, so it stays under the part whichever way it points.
    fn shadow(&mut self, name: &str, d: String, alpha: f64, blur: f64) {
        let shade = Color { a: alpha, ..BLACK };
        let s = self.side();
        let mut n = vector(name, 0.0, 0.0, s, s, d, vec![Paint::solid(shade)]);
        n.effects = vec![Effect {
            kind: EffectKind::DropShadow,
            visible: true,
            color: shade,
            offset_x: 0.0,
            offset_y: 0.0,
            radius: 2.0 * blur * self.g.k,
            spread: 0.0,
        }];
        self.turn(n);
    }
    /// A fixed path.
    fn over(&mut self, name: &str, d: String, fills: Vec<Paint>) {
        let s = self.side();
        self.fix(vector(name, 0.0, 0.0, s, s, d, fills));
    }
    /// The moulding's colour: black phenolic, or the variant's tint.
    fn black(&self, base: &str) -> Color {
        self.tint
            .map_or_else(|| css(base), |t| draw::mix(t, BLACK, 0.35))
    }
    /// A rotation-invariant fill for a round moulding of radius `r` units:
    /// `centre` at the shaft to `edge` at the rim (the sheen of a gloss
    /// moulding is the fixed light over it, not this).
    fn radial(&self, r: f64, centre: Color, edge: Color) -> Paint {
        let f = r * self.g.k / self.side();
        let stops = [
            (0.0, hex_of(centre)),
            (f * 1.6, hex_of(draw::mix(centre, edge, 0.4))),
            (f * 2.0, hex_of(edge)),
            (1.0, hex_of(edge)),
        ];
        draw::radial_at(0.5, 0.5, 0.5, &refs(&stops))
    }
    /// The room's gloss on a round surface of radius `r` units: the lit
    /// wash, a soft highlight up and to the left, a hairline on the rim.
    fn gloss(&mut self, name: &str, r: f64, (lit, dark): (f64, f64), spot: f64) {
        let d = self.circle(0.0, 0.0, r);
        self.over(&format!("{name} light"), d, vec![room_light(lit, dark)]);
        if spot > 0.0 {
            let (cx, cy) = self.pt(-r * 0.34, -r * 0.42);
            let (w, h) = (r * 0.62 * self.g.k, r * 0.30 * self.g.k);
            let mut blob = Node::new(NodeKind::Ellipse, format!("{name} highlight light"));
            blob.transform =
                crate::document::proto::Transform::translate(cx - w / 2.0, cy - h / 2.0);
            blob.size = crate::document::proto::Size {
                width: w,
                height: h,
            };
            let c = |a: f64| hex_of(Color { a, ..WHITE });
            let stops = [(0.0, c(spot)), (0.45, c(spot * 0.35)), (0.7, c(0.0))];
            blob.fills = vec![draw::radial_at(0.5, 0.5, 0.7, &refs(&stops))];
            self.fix(draw::rotated(blob, -34.0));
        }
        let arc = draw::sector(
            self.g.c,
            self.g.c,
            (r - 0.9) * self.g.k,
            (r - 0.2) * self.g.k,
            -62.0,
            28.0,
        );
        self.over(
            &format!("{name} rim light"),
            arc,
            vec![Paint::solid(Color {
                a: 0.22 * lit / 0.2,
                ..WHITE
            })],
        );
    }
}

/// The shared builder with a bare spec, then `dress` adds the moulding.
fn dressed(
    b: &mut Builder<'_>,
    parent: NodeId,
    p: &Place,
    spec: &KnobSpec,
    title: &str,
    default_size: f64,
    dress: Dress,
) -> NodeId {
    let root = super::build(b, parent, p, spec, title, default_size);
    let d = p.size.unwrap_or(default_size).max(12.0);
    let k = d / (2.0 * BODY_R);
    let child = |b: &Builder<'_>, of: NodeId, name: &str| {
        b.scene.node(of).and_then(|n| {
            n.children
                .iter()
                .copied()
                .find(|c| b.scene.node(*c).is_some_and(|c| c.name == name))
        })
    };
    let knob = child(b, root, "knob").expect("a knob frame");
    let body = child(b, knob, "body").expect("a body frame");
    // The builder's round contact shadow: each dressing casts its own.
    if let Some(s) = child(b, knob, "body shadow")
        && let Some(n) = b.scene.node_mut(s)
    {
        n.visible = false;
    }
    let side = b.scene.node(knob).map_or(0.0, |n| n.size.width);
    let which = p
        .variant
        .as_deref()
        .and_then(|v| spec.variants.iter().position(|n| *n == v))
        .unwrap_or(0);
    let tint = spec.tints.get(which).copied().flatten().map(css);
    let mut parts = Parts {
        b,
        knob,
        body,
        g: Geo { k, c: side / 2.0 },
        tint,
        under: 0,
    };
    dress(&mut parts);
    root
}

/// A rounded, square-ended tab on the tail (pointing down, +v).
fn tail_tab(from: f64, to: f64, half_in: f64, half_out: f64) -> [(f64, f64); 4] {
    [
        (-half_in, from),
        (half_in, from),
        (half_out, to),
        (-half_out, to),
    ]
}

// ── the dressings ────────────────────────────────────────────────────────

fn dress_tab(k: &mut Parts<'_, '_>) {
    let black = k.black("#0b0b0d");
    // The tab: square-ended, a touch wider at its foot, moulded on the tail.
    let tab = tail_tab(16.0, 36.5, 12.5, 14.0);
    let tab_d = k.poly(&tab);
    k.shadow("tab shadow", tab_d.clone(), 0.55, 2.2);
    let disc_d = k.circle(0.0, 0.0, BODY_R);
    k.shadow("dome shadow", disc_d.clone(), 0.55, 2.6);
    k.part(
        "tab",
        tab_d,
        vec![Paint::solid(draw::mix(black, WHITE, 0.03))],
    );
    // The dome: glossy phenolic, darkest at the rim.
    let dome = k.radial(BODY_R, draw::mix(black, WHITE, 0.10), black);
    k.part("dome", disc_d, vec![dome]);
    // The fixed room: the tab's lit edge can't follow it, so it is left
    // matte; the dome takes the gloss.
    k.gloss("dome", BODY_R, (0.20, 0.34), 0.34);
}

/// A regular `n`-gon of circumradius `r` with its corners rounded to `rc`
/// (units), one corner straight up.
fn rounded_polygon(n: u32, r: f64, rc: f64) -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let step = 360.0 / f64::from(n);
    for i in 0..n {
        let a = f64::from(i) * step;
        let (cx, cy) = draw::polar(0.0, 0.0, r - rc, a);
        for j in 0..=6 {
            let t = a - step / 2.0 + step * f64::from(j) / 6.0;
            pts.push(draw::polar(cx, cy, rc, t));
        }
    }
    pts
}

fn dress_heptagon(k: &mut Parts<'_, '_>) {
    let black = k.black("#09090b");
    let skirt_d = k.circle(0.0, 0.0, BODY_R);
    k.shadow("skirt shadow", skirt_d.clone(), 0.5, 2.4);
    k.part(
        "skirt",
        skirt_d,
        vec![k.radial(BODY_R, draw::mix(black, WHITE, 0.08), black)],
    );
    // The seven-sided grip, a corner on the index.
    let grip = rounded_polygon(7, 27.5, 5.5);
    let grip_d = k.poly(&grip);
    k.shadow("grip shadow", grip_d.clone(), 0.6, 1.4);
    let mut grip_n = vector(
        "grip",
        0.0,
        0.0,
        k.side(),
        k.side(),
        grip_d,
        vec![k.radial(
            27.5,
            draw::mix(black, WHITE, 0.07),
            draw::mix(black, WHITE, 0.02),
        )],
    );
    grip_n.stroke = crate::document::proto::Stroke {
        paints: vec![Paint::solid(Color { a: 0.10, ..WHITE })],
        weight: (0.5 * k.g.k).max(0.5),
        align: crate::document::proto::StrokeAlign::Inside,
        ..crate::document::proto::Stroke::default()
    };
    k.turn(grip_n);
    // The flat, polished top.
    let top_d = k.circle(0.0, 0.0, 18.0);
    let mut top = vector(
        "top",
        0.0,
        0.0,
        k.side(),
        k.side(),
        top_d,
        vec![k.radial(
            18.0,
            draw::mix(black, WHITE, 0.06),
            draw::mix(black, WHITE, 0.02),
        )],
    );
    top.stroke = crate::document::proto::Stroke {
        paints: vec![Paint::solid(Color { a: 0.3, ..BLACK })],
        weight: (0.4 * k.g.k).max(0.5),
        align: crate::document::proto::StrokeAlign::Outside,
        ..crate::document::proto::Stroke::default()
    };
    k.turn(top);
    k.gloss("grip", 27.0, (0.18, 0.36), 0.0);
    // Polished: the window's reflection runs down the lit facets on the
    // right, a hard crescent, and a softer one on the left.
    let crescent = draw::sector(k.g.c, k.g.c, 20.5 * k.g.k, 25.5 * k.g.k, 22.0, 78.0);
    k.over(
        "grip window light",
        crescent,
        vec![Paint::solid(Color { a: 0.30, ..WHITE })],
    );
    let soft = draw::sector(k.g.c, k.g.c, 21.0 * k.g.k, 25.0 * k.g.k, -80.0, -40.0);
    k.over(
        "grip soft light",
        soft,
        vec![Paint::solid(Color { a: 0.10, ..WHITE })],
    );
}

fn dress_bar(k: &mut Parts<'_, '_>) {
    let black = k.black("#0a0a0c");
    let hub_d = k.circle(0.0, 0.0, 20.0);
    k.shadow("boss shadow", hub_d.clone(), 0.55, 2.4);
    // The bar: tapered, its tail longer than its nose, ends rounded.
    let (nose, tail) = (-27.0, 35.0);
    let bar = |o: f64| {
        vec![
            (-5.8 - o, nose - o),
            (5.8 + o, nose - o),
            (9.0 + o, tail + o),
            (-9.0 - o, tail + o),
        ]
    };
    let bar_d = k.poly(&bar(0.0));
    k.shadow("bar shadow", bar_d.clone(), 0.6, 2.2);
    k.part(
        "boss",
        hub_d,
        vec![k.radial(20.0, draw::mix(black, WHITE, 0.12), black)],
    );
    k.part(
        "bar edge",
        k.poly(&bar(0.6)),
        vec![Paint::solid(Color { a: 0.7, ..BLACK })],
    );
    // Lit along its crest, down both flanks: a ridge, symmetric so it turns.
    let x = |u: f64| (k.g.c + u * k.g.k) / k.side();
    let c = |t: f64| hex_of(draw::mix(black, WHITE, t));
    let crest = [
        (0.0, c(0.0)),
        (x(-7.0), c(0.0)),
        (x(-2.0), c(0.14)),
        (x(2.0), c(0.14)),
        (x(7.0), c(0.0)),
        (1.0, c(0.0)),
    ];
    k.part("bar", bar_d, vec![draw::linear(90.0, &refs(&crest))]);
    k.gloss("boss", 20.0, (0.16, 0.30), 0.22);
}

fn dress_fluted_stack(k: &mut Parts<'_, '_>) {
    let black = k.black("#0c0c0e");
    // The skirt: fine scallops all round.
    let (cx, cy) = k.pt(0.0, 0.0);
    let kk = k.g.k;
    let skirt = scallop(cx, cy, BODY_R * kk, 36, 1.3 * kk);
    let skirt_d = fmt_pts(&skirt);
    k.shadow("skirt shadow", skirt_d.clone(), 0.55, 2.6);
    let mut skirt_n = vector(
        "skirt",
        0.0,
        0.0,
        k.side(),
        k.side(),
        skirt_d,
        vec![k.radial(
            BODY_R,
            draw::mix(black, WHITE, 0.05),
            draw::mix(black, WHITE, 0.10),
        )],
    );
    skirt_n.stroke = crate::document::proto::Stroke {
        paints: vec![Paint::solid(Color { a: 0.55, ..BLACK })],
        weight: (0.6 * kk).max(0.5),
        align: crate::document::proto::StrokeAlign::Inside,
        ..crate::document::proto::Stroke::default()
    };
    k.turn(skirt_n);
    // The step up to the body, as a dark wall.
    k.part(
        "step",
        k.circle(0.0, 0.0, 22.6),
        vec![Paint::solid(draw::mix(black, BLACK, 0.6))],
    );
    k.part(
        "body wall",
        k.circle(0.0, 0.0, 21.2),
        vec![k.radial(
            21.2,
            draw::mix(black, WHITE, 0.10),
            draw::mix(black, WHITE, 0.03),
        )],
    );
    // The dome on top.
    let dome = k.circle(0.0, 0.0, 13.5);
    k.shadow("dome shadow", dome.clone(), 0.45, 1.6);
    k.part(
        "dome",
        dome,
        vec![k.radial(
            13.5,
            draw::mix(black, WHITE, 0.16),
            draw::mix(black, WHITE, 0.04),
        )],
    );
    // The index: a white chevron on the skirt, a white dot on the body.
    let (w, t) = (3.6, 1.35);
    let chevron = k.poly(&[
        (0.0, -29.2),
        (w + t, -23.0),
        (w, -22.0),
        (0.0, -26.7),
        (-w, -22.0),
        (-w - t, -23.0),
    ]);
    k.part("chevron", chevron, vec![Paint::solid(css(LINE))]);
    k.part(
        "dot",
        k.circle(0.0, -17.4, 1.9),
        vec![Paint::solid(css(LINE))],
    );
    k.gloss("skirt", BODY_R, (0.12, 0.30), 0.0);
    k.gloss("dome", 21.0, (0.16, 0.30), 0.30);
}

fn dress_paddle(k: &mut Parts<'_, '_>) {
    let black = k.black("#0b0b0d");
    // The paddle: broad, round-ended, on the tail.
    let g = k.g;
    let paddle = |o: f64| {
        let (hw, end) = (12.5 + o, 40.0 + o);
        let (x0, y0) = g.p(-hw, 6.0);
        let (x1, y1) = g.p(hw, 6.0);
        let (x2, y2) = g.p(hw * 0.86, end - hw * 0.86);
        let (x3, y3) = g.p(-hw * 0.86, end - hw * 0.86);
        let r = hw * 0.86 * g.k;
        format!(
            "M {x0:.3} {y0:.3} L {x1:.3} {y1:.3} L {x2:.3} {y2:.3} A {r:.3} {r:.3} 0 0 1 {x3:.3} {y3:.3} Z"
        )
    };
    k.shadow("paddle shadow", paddle(0.0), 0.6, 2.4);
    let collar = k.circle(0.0, 0.0, 20.5);
    k.shadow("collar shadow", collar.clone(), 0.5, 2.2);
    k.part(
        "paddle edge",
        paddle(0.6),
        vec![Paint::solid(Color { a: 0.7, ..BLACK })],
    );
    let x = |u: f64| (k.g.c + u * k.g.k) / k.side();
    let c = |t: f64| hex_of(draw::mix(black, WHITE, t));
    let ridge = [
        (0.0, c(0.02)),
        (x(-10.0), c(0.02)),
        (x(-3.0), c(0.11)),
        (x(3.0), c(0.11)),
        (x(10.0), c(0.02)),
        (1.0, c(0.02)),
    ];
    k.part(
        "paddle",
        paddle(0.0),
        vec![draw::linear(90.0, &refs(&ridge))],
    );
    k.part(
        "collar",
        collar,
        vec![k.radial(20.5, draw::mix(black, WHITE, 0.06), black)],
    );
    let cap = k.circle(0.0, 0.0, 15.0);
    k.shadow("cap shadow", cap.clone(), 0.4, 1.2);
    k.part(
        "cap",
        cap,
        vec![k.radial(
            15.0,
            draw::mix(black, WHITE, 0.15),
            draw::mix(black, WHITE, 0.03),
        )],
    );
    // The chevron at the nose, on the collar.
    let (w, t) = (4.2, 1.5);
    let chevron = k.poly(&[
        (0.0, -26.5),
        (w + t, -18.8),
        (w, -17.6),
        (0.0, -23.6),
        (-w, -17.6),
        (-w - t, -18.8),
    ]);
    k.part("chevron", chevron, vec![Paint::solid(css(LINE))]);
    k.gloss("collar", 20.5, (0.16, 0.30), 0.0);
    k.gloss("cap", 15.0, (0.18, 0.28), 0.34);
}

fn dress_fin(k: &mut Parts<'_, '_>) {
    let black = k.black("#141416");
    let disc = k.circle(0.0, 0.0, BODY_R);
    k.shadow("disc shadow", disc.clone(), 0.5, 2.4);
    k.part(
        "disc",
        disc,
        vec![k.radial(
            BODY_R,
            draw::mix(black, WHITE, 0.05),
            draw::mix(black, BLACK, 0.3),
        )],
    );
    // The fin, straight across, standing proud of the disc.
    let g = k.g;
    let fin = |o: f64| {
        rrect(
            g.c - (7.0 + o) * g.k,
            g.c - (29.4 + o) * g.k,
            (14.0 + 2.0 * o) * g.k,
            (58.8 + 2.0 * o) * g.k,
            3.0 * g.k,
        )
    };
    k.shadow("fin shadow", fin(0.0), 0.6, 1.8);
    let x = |u: f64| (k.g.c + u * k.g.k) / k.side();
    let c = |t: f64| hex_of(draw::mix(black, WHITE, t));
    let ridge = [
        (0.0, c(0.0)),
        (x(-7.0), c(0.0)),
        (x(-5.2), c(0.13)),
        (x(-3.8), c(0.07)),
        (x(3.8), c(0.07)),
        (x(5.2), c(0.13)),
        (x(7.0), c(0.0)),
        (1.0, c(0.0)),
    ];
    k.part("fin", fin(0.0), vec![draw::linear(90.0, &refs(&ridge))]);
    k.gloss("disc", BODY_R, (0.10, 0.34), 0.0);
}
