//! Kit family: knobs. Ported from `fts-audio-ui::hardware::{knob, knob_kit,
//! knob_parts, knob_svg}` and `rack::Ring` — see `KIT.md`.
//!
//! Every style is one [`spec::KnobSpec`] (tiers, knurl, index, light, printed
//! scale, colour variants) and one builder, [`build`], walks it. A placed
//! knob is:
//!
//! ```text
//! <root control=Knob{sweep}>          box × (box + legend)
//!   knob   (square, centred on the shaft)
//!     scale: arc / ticks / dots / numerals printed on the panel   (fixed)
//!     shadows and the round tiers, lit by the room               (fixed)
//!     body  ← Needle slot: a square frame over the knob, turning about
//!             the shaft with everything that turns — teeth, knurl, index,
//!             wing / beak, the skirt's numerals — drawn pointing up
//!     light: highlights, the light on teeth and mouldings, the hub (fixed)
//!   legend ← Label slot (Place::legend, else the bound param's name)
//! ```
//!
//! The renderer composes a frame's rotation into its children, so the body
//! is ordinary nodes (text included). What must *not* turn is what the room
//! does to the knob: the shading of a round tier is its lighting, so round
//! tiers stay put under the body, and a turning part's own paint is kept
//! rotation-invariant (centred on the shaft) with the directional light laid
//! over it from the fixed overlay above.

pub(crate) mod chrome_skirt;
pub(crate) mod concentric;
pub(crate) mod knurled_cap;
pub(crate) mod studio;
pub(crate) mod valve;
pub(crate) mod outboard;
pub(crate) mod pedal;
pub(crate) mod amp;
pub(crate) mod pedal_tall;
pub(crate) mod amp_pointer;
pub mod spec;

use document_proto::{
    Color, Effect, Node, NodeId, NodeKind, Paint, Size, Stroke, StrokeAlign, TextAlign, TextCase, TextData, Transform,
};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::draw;
use crate::registry::{ComponentDef, Family, Place};
use spec::{BODY_R, Edge, Finish, Flutes, Grad, Index, KnobSpec, RingKind, SpecFill, Specular, Tier, TierPaint};

/// Scale-ring ink: silkscreen on a dark panel.
const INK: &str = "#d6d9dd";

macro_rules! knobs {
    ($( $fn:ident => $spec:path, $id:literal, $title:literal, $size:literal, $desc:literal; )*) => {
        $(
            fn $fn(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
                build(b, parent, p, &$spec, $title, $size)
            }
        )*

        /// This family's components.
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

        /// Every style's spec, by id (for tests and tooling).
        #[must_use]
        pub fn spec_of(id: &str) -> Option<&'static KnobSpec> {
            match id {
                $( $id => Some(&$spec), )*
                _ => None,
            }
        }
    };
}

knobs! {
    bakelite => spec::BAKELITE, "knob.bakelite", "Bakelite", 64.0,
        "Black bakelite with a painted white line: a dark base, a matte cone and a flat top face — the LA-2A / 1176 knob. A plain, classic pointer knob for any gain-style control; prints a 0–10 tick scale.";
    metal => spec::METAL, "knob.metal", "Metal", 60.0,
        "Brushed aluminium on a dark seat, read by a dark tapered blade. A generic metal knob for utility controls on metal or dark panels.";
    skirted => spec::SKIRTED, "knob.skirted", "Skirted", 68.0,
        "Davies-style phenolic: a wide flat skirt, a narrower ribbed body and a flat top, with the index line running out over the skirt to the panel's numbers. Vintage outboard, not a Pultec.";
    daka => spec::DAKA, "knob.daka", "Daka-Ware", 72.0,
        "The Pultec EQP-1A's Daka-Ware knob: black phenolic with a coarsely scalloped skirt, a ridged raised body and a dome, and a bold white engraved index; numerals 0–10 hug the skirt. For vintage passive EQ boost/cut/frequency.";
    marconi => spec::MARCONI, "knob.marconi", "Marconi", 64.0,
        "The Neve 1073's Marconi wing knob: a coloured wing laid across a dark disc and overhanging it, with a white line down the wing, inside a dotted scale. Colour-coded: red gain, blue filters, grey shelf.";
    collet => spec::COLLET, "knob.collet", "Collet", 52.0,
        "SSL 4000 channel collet cap: a flat-topped coloured cap with a fluted rim and one white bar, inside a printed dot ring. Band colours (blue LMF, green HMF, magenta HF, red, black, cream) find a band without reading.";
    silver_top => spec::SILVER_TOP, "knob.silver_top", "Silver-top", 72.0,
        "The UREI 1176 knob: a wide matte black collar with a brushed, knurled aluminium cap set in it, the index a white line on the collar — reads as a dark ring around a bright disc. For FET-compressor input/output/attack/release.";
    metal_fluted => spec::METAL_FLUTED, "knob.metal_fluted", "Metal fluted", 60.0,
        "dbx-generation knob: brushed aluminium with a fluted rim and a dark centre cap, read by a white line across the metal. 1970s–80s outboard.";
    pointer => spec::POINTER, "knob.pointer", "Pointer", 64.0,
        "Teletronix LA-2A pointer knob: a plain black round knob with a moulded nose that points at a scale printed on the panel. No skirt, no flutes — you read the nose.";
    neve => spec::NEVE, "knob.neve", "Neve", 64.0,
        "Neve 1073 module knob: a smooth turned light-grey ring around a dark GEARED cap, a white index at the cap's teeth and a slim dark one on the ring, inside a dotted ring. Cap colours code the band.";
    soft_pointer => spec::SOFT_POINTER, "knob.soft_pointer", "Soft pointer", 52.0,
        "Rogan PT modular knob (Make Noise, Mutable): soft-touch matte rubber with no gloss and a moulded pointer that widens toward the rim. The one modern knob in the kit — for synth and modular-style panels.";
    chicken_head => spec::CHICKEN_HEAD, "knob.chicken_head", "Chicken-head", 64.0,
        "Chicken-head: the pointer is the knob — a cream moulding with a round body and a beak, on a small dark hub (tweed Fender, Ampeg, EMS VCS3, meter selectors). Reads as a switch: use it for stepped selectors.";
    dial => spec::DIAL, "knob.dial", "Dial", 72.0,
        "Empirical Labs Distressor dial: a wide brushed dial whose numerals are printed on its own skirt and turn past a fixed index on the panel, around a dark centre cap. The panel around it is bare.";
}

// ── colour ───────────────────────────────────────────────────────────────

/// A source colour literal: `#hex` or `rgba(r, g, b, a)`.
fn css(s: &str) -> Color {
    if let Some(body) = s.strip_prefix("rgba(").and_then(|r| r.strip_suffix(')')) {
        let v: Vec<f64> = body.split(',').map(|x| x.trim().parse().unwrap_or(0.0)).collect();
        return Color { r: v[0] / 255.0, g: v[1] / 255.0, b: v[2] / 255.0, a: v.get(3).copied().unwrap_or(1.0) };
    }
    draw::hex(s)
}

fn hex_of(c: Color) -> String {
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", q(c.r), q(c.g), q(c.b), q(c.a))
}

const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

fn luminance(c: Color) -> f64 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

fn owned(s: &[(f64, &str)]) -> Vec<(f64, String)> {
    s.iter().map(|(p, c)| (*p, hex_of(css(c)))).collect()
}

fn refs(s: &[(f64, String)]) -> Vec<(f64, &str)> {
    s.iter().map(|(p, c)| (*p, c.as_str())).collect()
}

fn linear(angle: f64, s: &[(f64, Color)]) -> Paint {
    let o: Vec<(f64, String)> = s.iter().map(|(p, c)| (*p, hex_of(*c))).collect();
    draw::linear(angle, &refs(&o))
}

/// CSS `radial-gradient(circle at x y, …)` (farthest-corner) on a circle.
fn radial_css(x: f64, y: f64, s: &[(f64, Color)]) -> Paint {
    let far = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].iter().map(|(cx, cy)| (cx - x).hypot(cy - y)).fold(0.0, f64::max);
    let o: Vec<(f64, String)> = s.iter().map(|(p, c)| (*p, hex_of(*c))).collect();
    draw::radial_at(x, y, far, &refs(&o))
}

fn colors(s: &[(f64, &str)]) -> Vec<(f64, Color)> {
    s.iter().map(|(p, c)| (*p, css(c))).collect()
}

/// Turned aluminium in layers: the metal, the anisotropic lobes a lathe-faced
/// cap throws, the room's glint (bottom to top).
fn brushed(metal: [(f64, Color); 3]) -> Vec<Paint> {
    let conic = owned(&[
        (0.0, "rgba(255,255,255,0.14)"),
        (88.0 / 360.0, "rgba(0,0,0,0.12)"),
        (0.5, "rgba(255,255,255,0.13)"),
        (268.0 / 360.0, "rgba(0,0,0,0.13)"),
        (1.0, "rgba(255,255,255,0.14)"),
    ]);
    vec![
        linear(162.0, &metal),
        draw::conic(208.0, &refs(&conic)),
        radial_css(0.34, 0.26, &colors(&[(0.0, "rgba(255,255,255,0.26)"), (0.58, "rgba(255,255,255,0.0)")])),
    ]
}

/// A material in the variant's colour: the finish keeps its gradient shape.
fn tinted(finish: Finish, c: Color) -> Vec<Paint> {
    let lift = |t: f64| draw::mix(c, WHITE, t);
    let sink = |t: f64| draw::mix(c, BLACK, t);
    match finish {
        Finish::Moulded => vec![radial_css(0.36, 0.24, &[(0.0, lift(0.28)), (0.42, c), (1.0, sink(0.38))])],
        Finish::FlatTop => vec![linear(162.0, &[(0.0, lift(0.24)), (0.44, c), (1.0, sink(0.28))])],
        Finish::Matte => vec![linear(160.0, &[(0.0, lift(0.12)), (0.38, c), (1.0, sink(0.18))])],
        Finish::SoftTouch => vec![radial_css(0.5, 0.5, &[(0.0, c), (0.62 * 0.707, c), (0.88 * 0.707, lift(0.10)), (0.707, sink(0.22))])],
        Finish::Brushed => brushed([(0.0, lift(0.14)), (0.46, c), (1.0, sink(0.16))]),
    }
}

fn grad_paints(g: Grad) -> Vec<Paint> {
    match g {
        Grad::Linear(a, s) => vec![linear(a, &colors(s))],
        Grad::Radial(x, y, s) => {
            // A centred CSS circle is sized to the corner; our circles are
            // the box's incircle, so stops keep their CSS meaning.
            vec![radial_css(x, y, &colors(s))]
        }
        Grad::Brushed => brushed([(0.0, css("#cdcdc8")), (0.46, css("#b4b4af")), (1.0, css("#96968f"))]),
    }
}

// ── geometry ─────────────────────────────────────────────────────────────

/// Knob design units → the knob frame's px: origin at the shaft.
#[derive(Clone, Copy)]
struct Geo {
    /// px per unit.
    k: f64,
    /// The shaft, px from the frame's corner (the frame is `2c` square).
    c: f64,
}

impl Geo {
    fn p(self, u: f64, v: f64) -> (f64, f64) {
        (self.c + u * self.k, self.c + v * self.k)
    }
    fn polar(self, r: f64, deg: f64) -> (f64, f64) {
        draw::polar(self.c, self.c, r * self.k, deg)
    }
    fn size(self) -> f64 {
        2.0 * self.c
    }
}

fn fmt_pts(pts: &[(f64, f64)]) -> String {
    draw::polygon(pts)
}

/// A rounded rectangle, as path data.
fn rrect(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let r = r.min(w / 2.0).min(h / 2.0);
    format!(
        "M {:.3} {y:.3} L {:.3} {y:.3} A {r:.3} {r:.3} 0 0 1 {:.3} {:.3} L {:.3} {:.3} A {r:.3} {r:.3} 0 0 1 {:.3} {:.3} L {:.3} {:.3} A {r:.3} {r:.3} 0 0 1 {x:.3} {:.3} L {x:.3} {:.3} A {r:.3} {r:.3} 0 0 1 {:.3} {y:.3} Z ",
        x + r,
        x + w - r,
        x + w,
        y + r,
        x + w,
        y + h - r,
        x + w - r,
        y + h,
        x + r,
        y + h,
        y + h - r,
        y + r,
        x + r,
    )
}

/// A full circle as a closed subpath.
fn circle_path(cx: f64, cy: f64, r: f64) -> String {
    format!(
        "M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} Z ",
        cx + r,
        cx - r,
        cx + r
    )
}

/// A full circle wound the other way: a hole in a nonzero path.
fn circle_path_ccw(cx: f64, cy: f64, r: f64) -> String {
    format!(
        "M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 0 {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 0 {:.3} {cy:.3} Z ",
        cx + r,
        cx - r,
        cx + r
    )
}

/// The scalloped outline (the source's quadratic lobes, sampled): radius
/// `r`, `lobes` of `depth` (same units), centred on (`cx`, `cy`).
fn scallop(cx: f64, cy: f64, r: f64, lobes: u32, depth: f64) -> Vec<(f64, f64)> {
    const PER_LOBE: u32 = 8;
    let inner = r - depth;
    let ctrl = r + depth * 0.55;
    (0..lobes * PER_LOBE)
        .map(|i| {
            let t = f64::from(i) / f64::from(PER_LOBE);
            let lt = t.fract();
            let rr = (1.0 - lt).powi(2) * inner + 2.0 * (1.0 - lt) * lt * ctrl + lt.powi(2) * inner;
            draw::polar(cx, cy, rr, t / f64::from(lobes) * 360.0)
        })
        .collect()
}

/// A band `w` wide along a closed outline, as one nonzero path: the outline
/// pushed out, then pulled in and wound the other way.
fn outline_band(cx: f64, cy: f64, pts: &[(f64, f64)], w: f64) -> String {
    let push = |s: f64| -> Vec<(f64, f64)> {
        pts.iter()
            .map(|(x, y)| {
                let (dx, dy) = (x - cx, y - cy);
                let len = dx.hypot(dy).max(1e-6);
                (x + dx / len * s, y + dy / len * s)
            })
            .collect()
    };
    let mut inner = push(-w / 2.0);
    inner.reverse();
    format!("{} {}", fmt_pts(&push(w / 2.0)), fmt_pts(&inner))
}

/// How many ridges a knurl of `asked` can show at `band_px` across — one per
/// ~3.2 px, never fewer than eight.
fn flute_count(asked: u32, band_px: f64) -> u32 {
    const PITCH_PX: f64 = 3.2;
    let fits = (std::f64::consts::PI * band_px.max(1.0) / PITCH_PX) as u32;
    asked.min(fits).max(8)
}

// ── nodes ────────────────────────────────────────────────────────────────

fn vector(name: &str, x: f64, y: f64, w: f64, h: f64, d: String, fills: Vec<Paint>) -> Node {
    let mut n = Node::new(NodeKind::Vector, name);
    n.transform = Transform::translate(x, y);
    n.size = Size { width: w, height: h };
    n.fill_geometry = vec![d];
    n.fills = fills;
    n
}

/// A second colour on a vector: filled stroke geometry drawn over its fill.
fn with_stroke_geometry(mut n: Node, d: String, color: Color) -> Node {
    n.stroke_geometry = vec![d];
    n.stroke = Stroke { paints: vec![Paint::solid(color)], weight: 1.0, ..Stroke::default() };
    n
}

fn ellipse(name: &str, cx: f64, cy: f64, r: f64, fills: Vec<Paint>) -> Node {
    let mut n = draw::circle(name, cx, cy, r, Paint::solid(BLACK));
    n.fills = fills;
    n
}

fn outline(mut n: Node, color: Color, weight: f64, align: StrokeAlign) -> Node {
    n.stroke = Stroke { paints: vec![Paint::solid(color)], weight, align, ..Stroke::default() };
    n
}

fn label(name: &str, s: &str, (cx, cy): (f64, f64), size: f64, color: &str) -> Node {
    let mut n = draw::label_at(name, s, cx, cy, size, color);
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(700.0);
    }
    n
}

fn legend(text: &str, y: f64, w: f64, size: f64, ink: &str, case: TextCase) -> Node {
    let mut n = Node::new(
        NodeKind::Text(TextData {
            characters: text.to_string(),
            font_size: size,
            font_weight: Some(600.0),
            letter_spacing: size * 0.06,
            text_case: case,
            align: TextAlign::Center,
            ..TextData::default()
        }),
        "legend",
    );
    n.transform = Transform::translate(0.0, y);
    n.size = Size { width: w, height: (size * 1.35).ceil() };
    n.fills = vec![draw::solid(ink)];
    n
}

// ── the builder ──────────────────────────────────────────────────────────

/// Positions (0..1 along the sweep) of `n` evenly spaced labels.
fn positions(n: usize) -> Vec<f64> {
    match n {
        0 => Vec::new(),
        1 => vec![0.5],
        _ => (0..n).map(|i| i as f64 / (n - 1) as f64).collect(),
    }
}

/// Place a knob of `spec` under `parent`.
pub(crate) fn build(b: &mut Builder<'_>, parent: NodeId, p: &Place, spec: &KnobSpec, title: &str, default_size: f64) -> NodeId {
    let d = p.size.unwrap_or(default_size).max(12.0);
    let k = d / (2.0 * BODY_R);
    let which = p.variant.as_deref().and_then(|v| spec.variants.iter().position(|n| *n == v)).unwrap_or(0);
    let tint: Option<Color> = spec.tints.get(which).copied().flatten().map(css);
    // A stepped knob (options given) prints its positions and turns through
    // exactly its detents; an authored scale or sweep wins.
    let stepped = p.options.as_ref().filter(|o| !o.is_empty());
    let labels: Vec<String> = p
        .scale
        .clone()
        .or_else(|| stepped.cloned())
        .unwrap_or_else(|| spec.scale.iter().map(ToString::to_string).collect());
    let sweep = p.sweep.unwrap_or_else(|| stepped.map_or(spec.sweep, |o| crate::registry::detent_sweep(o.len())));
    let spec = &KnobSpec { sweep, ..*spec };
    let ink = p.ink_color(INK);
    let (ring_r, label_r) = {
        let (r, l) = spec.ring.geometry();
        (r + spec.ring_offset, l + spec.ring_offset)
    };
    // The printed labels: set clear of their ticks, at a readable size
    // whatever the knob's (px, relative to the shaft).
    // Position words (a stepped knob's options) are set in the place's
    // case, like its legend; an authored scale as written.
    let words = stepped.is_some() && p.scale.is_none();
    let case = if words { p.case() } else { TextCase::Original };
    let print = Print { clear: clearance(spec.ring, ring_r, label_r) * k, size: (7.0 * k).max(draw::SCALE_TYPE_MIN), case };
    let shown = |s: &str| if case == TextCase::Upper { s.to_uppercase() } else { s.to_string() };
    let angle = |t: f64| (t - 0.5) * spec.sweep;
    let extents: Vec<(f64, f64, f64, f64)> = if spec.ring == RingKind::OnKnob {
        Vec::new()
    } else {
        labels.iter().zip(positions(labels.len())).filter(|(s, _)| !s.is_empty()).map(|(s, t)| draw::radial_extent(&shown(s), print.clear, angle(t), print.size)).collect()
    };
    let reach = extents.iter().map(|e| e.0.abs().max(e.1.abs()).max(e.2.abs()).max(e.3.abs())).fold(0.0, f64::max);
    let bottom = extents.iter().map(|e| e.3).fold(f64::MIN, f64::max);
    // Half the frame, in units: the labels plus a margin, or the widest
    // overhang when the scale is on the knob.
    let half = if spec.ring == RingKind::OnKnob { 44.0 } else { (label_r + 6.0).max(44.0).max(reach / k + 3.0) };
    let g = Geo { k, c: half * k };
    let side = g.size();

    let legend_size = (9.5 * k).max(8.0);
    // The legend sits under whatever is printed lowest: the knob, the ends
    // of its ring, its lowest label.
    let ring_bottom = if labels.is_empty() || spec.ring == RingKind::OnKnob { 0.0 } else { -(ring_r + 5.0) * (spec.sweep / 2.0).to_radians().cos() };
    let body_r = BODY_R * spec.tiers.first().map_or(1.0, |t| t.r).max(1.0);
    let legend_y = if spec.ring == RingKind::OnKnob {
        g.c + (0.866 * label_r + 5.0) * k
    } else {
        g.c + ((body_r + 6.0) * k).max((ring_bottom + 4.0) * k).max(bottom + 4.0)
    };
    let height = side.max(legend_y + (legend_size * 1.35).ceil());
    let name = if p.name.is_empty() { title } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, height));
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));

    // ── fixed: the panel, the shadows, the round (lit) tiers ──
    printed_scale(b, knob, g, spec, &labels, ring_r, print, &ink, stepped.is_some() && p.scale.is_none());
    let outer = spec.tiers.first().map_or(1.0, |t| t.r);
    let (sx, sy) = g.p(0.0, 1.5);
    b.add(knob, ellipse("body shadow", sx, sy, BODY_R * outer * k, vec![Paint::solid(css("rgba(0,0,0,0.35)"))]));
    for (i, tier) in spec.tiers.iter().enumerate() {
        add_tier(b, knob, g, tier, tint, i);
    }
    if let Index::Beak { .. } = spec.index {
        // The lobe is centred on the shaft: its cast shadow never moves.
        let (x, y) = g.p(0.9, 1.8);
        b.add(knob, ellipse("moulding shadow", x, y, BODY_R * BEAK_LOBE * k, vec![Paint::solid(css("rgba(0,0,0,0.45)"))]));
    }

    // ── turning: the body ──
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    for (i, tier) in spec.tiers.iter().enumerate() {
        if let Edge::Toothed { teeth, depth } = tier.edge {
            add_teeth(b, body, g, tier, (teeth, depth), tint, i);
        }
    }
    if let Some(fl) = spec.flutes {
        add_flutes(b, body, g, d, &fl);
    }
    add_index(b, body, g, spec, tint, &labels);

    // ── fixed: the room's light over it, and the shaft ──
    for (i, tier) in spec.tiers.iter().enumerate() {
        if let Edge::Toothed { depth, .. } = tier.edge {
            let r = BODY_R * tier.r * k;
            let depth = BODY_R * depth * k;
            let ring = circle_path(g.c, g.c, r - depth * 0.3) + &circle_path_ccw(g.c, g.c, r - depth);
            b.add(knob, vector(&format!("tier {i} light"), 0.0, 0.0, side, side, ring, vec![room_light(0.16, 0.26)]));
        }
    }
    if let Index::Beak { .. } = spec.index {
        let lobe = circle_path(g.c, g.c, BODY_R * BEAK_LOBE * k * 0.98);
        b.add(knob, vector("moulding light", 0.0, 0.0, side, side, lobe, vec![room_light(0.24, 0.20)]));
    }
    if let Some(lit) = spec.specular {
        add_light(b, knob, g, d, &lit);
    }
    if let Some(hub) = spec.hub {
        b.add(knob, ellipse("hub", g.c, g.c, 2.6 * k, vec![Paint::solid(css(hub))]));
    }
    if spec.ring == RingKind::OnKnob {
        // The panel's fixed index the turning numerals are read against.
        let pts = [g.p(0.0, -(BODY_R + 6.0)), g.p(-3.2, -(BODY_R + 12.0)), g.p(3.2, -(BODY_R + 12.0))];
        let index_ink = p.ink.as_ref().map_or_else(|| "#e8eaec".to_string(), |_| ink.clone());
        b.add(knob, vector("panel index", 0.0, 0.0, side, side, fmt_pts(&pts), vec![draw::solid(&index_ink)]));
    }

    let legend_node = b.add(root, legend(p.legend.as_deref().unwrap_or(""), legend_y, side, legend_size, &ink, p.case()));

    let c = p.control(ControlKind::Knob { sweep_degrees: spec.sweep }).slot(SlotRole::Needle, body.0).slot(SlotRole::Label, legend_node.0);
    b.controls.insert(root, c);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, root, (g.c, g.c), d, touch);
    }
    root
}

/// The room's light on a turning part, as a fixed wash laid over it: lit
/// from the upper left (`lit` white) into shadow at the lower right (`dark`
/// black).
fn room_light(lit: f64, dark: f64) -> Paint {
    linear(
        160.0,
        &[
            (0.0, Color { a: lit, ..WHITE }),
            (0.42, Color { a: 0.0, ..WHITE }),
            (0.58, Color { a: 0.0, ..BLACK }),
            (1.0, Color { a: dark, ..BLACK }),
        ],
    )
}

/// A straight stroke from `a` to `b`, `w` wide and square-capped, as a
/// closed subpath (always wound the same way, so overlaps union).
fn seg(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-6);
    let (ux, uy) = (dx / len * w / 2.0, dy / len * w / 2.0);
    let (nx, ny) = (-uy, ux);
    let p = [
        (a.0 - ux + nx, a.1 - uy + ny),
        (b.0 + ux + nx, b.1 + uy + ny),
        (b.0 + ux - nx, b.1 + uy - ny),
        (a.0 - ux - nx, a.1 - uy - ny),
    ];
    format!("{} ", fmt_pts(&p))
}

/// How a knob's scale labels are set: `clear` is the radius (px from the
/// shaft) their boxes stay outside — the end of the ticks or dots plus a
/// gap — and `size` their type size, px.
#[derive(Clone, Copy)]
struct Print {
    clear: f64,
    size: f64,
    case: TextCase,
}

/// The radius (units) a ring's labels clear: past its major ticks or its
/// dots, or — numerals alone — about where the source centred them.
fn clearance(ring: RingKind, ring_r: f64, label_r: f64) -> f64 {
    match ring {
        RingKind::Ticks | RingKind::OnKnob => ring_r + 5.0 + 4.0,
        RingKind::Dots => ring_r + 1.35 + 4.5,
        RingKind::Numerals => label_r - 3.5,
    }
}

/// The scale printed on the panel around the knob (the source's `Ring`).
/// An empty scale prints nothing — no ring either.
#[allow(clippy::too_many_arguments)]
fn printed_scale(b: &mut Builder<'_>, knob: NodeId, g: Geo, spec: &KnobSpec, labels: &[String], ring_r: f64, print: Print, ink: &str, stepped: bool) {
    if labels.is_empty() {
        return;
    }
    let side = g.size();
    let at = positions(labels.len());
    let angle = |n: f64| (n - 0.5) * spec.sweep;
    match spec.ring {
        RingKind::Ticks => {
            let arc = draw::sector(g.c, g.c, (ring_r - 0.3) * g.k, (ring_r + 0.3) * g.k, angle(0.0), angle(1.0));
            b.add(knob, draw::with_opacity(vector("scale arc", 0.0, 0.0, side, side, arc, vec![draw::solid(ink)]), 0.35));
            let tick = |n: f64, len: f64, w: f64| seg(g.polar(ring_r - 1.0, angle(n)), g.polar(ring_r + len, angle(n)), w * g.k);
            let majors: String = at.iter().map(|n| tick(*n, 5.0, 1.4)).collect();
            let minors: String = at.windows(2).map(|w| tick((w[0] + w[1]) / 2.0, 3.0, 0.8)).collect();
            if !majors.is_empty() {
                b.add(knob, draw::with_opacity(vector("scale majors", 0.0, 0.0, side, side, majors, vec![draw::solid(ink)]), 0.9));
            }
            if !minors.is_empty() {
                b.add(knob, draw::with_opacity(vector("scale minors", 0.0, 0.0, side, side, minors, vec![draw::solid(ink)]), 0.55));
            }
        }
        RingKind::Dots => {
            // 21 is the 1073's: an arc from across the room, countable up
            // close. A stepped knob has a dot per detent.
            let n = if stepped { labels.len() } else { labels.len().max(21) };
            let dots: String = positions(n)
                .iter()
                .map(|t| {
                    let (x, y) = g.polar(ring_r, angle(*t));
                    circle_path(x, y, 1.35 * g.k)
                })
                .collect();
            b.add(knob, draw::with_opacity(vector("scale dots", 0.0, 0.0, side, side, dots, vec![draw::solid(ink)]), 0.92));
        }
        RingKind::Numerals | RingKind::OnKnob => {}
    }
    if spec.ring != RingKind::OnKnob {
        for (s, n) in labels.iter().zip(&at) {
            if !s.is_empty() {
                let est = if print.case == TextCase::Upper { s.to_uppercase() } else { s.clone() };
                let mut t = draw::radial_label("numeral", &est, (g.c, g.c), print.clear, angle(*n), print.size, ink);
                if let NodeKind::Text(t) = &mut t.kind {
                    t.characters.clone_from(s);
                    t.font_weight = Some(700.0);
                    t.text_case = print.case;
                }
                b.add(knob, t);
            }
        }
    }
}

/// The paints a tier's surface takes (empty for a groove).
fn tier_fills(tier: &Tier, tint: Option<Color>) -> Vec<Paint> {
    match tier.paint {
        TierPaint::Surface { grad, finish, tint: tintable } => match (tintable, tint) {
            (true, Some(c)) => tinted(finish, c),
            _ => grad_paints(grad),
        },
        TierPaint::Flat(c) => vec![Paint::solid(css(c))],
        TierPaint::Groove { .. } => Vec::new(),
    }
}

/// A tier's body colour, for the parts of it that turn (teeth).
fn tier_color(tier: &Tier, tint: Option<Color>) -> Color {
    match (tier.paint, tint) {
        (TierPaint::Surface { tint: true, .. }, Some(t)) => t,
        (TierPaint::Surface { grad: Grad::Linear(_, s) | Grad::Radial(_, _, s), .. }, _) => css(s[s.len() / 2].1),
        (TierPaint::Surface { grad: Grad::Brushed, .. }, _) => css("#b4b4af"),
        (TierPaint::Flat(c) | TierPaint::Groove { color: c, .. }, _) => css(c),
    }
}

/// A tier's fixed part: a round tier whole (its shading is the room's
/// light), a toothed tier's shadow and the lit disc inside its teeth.
fn add_tier(b: &mut Builder<'_>, knob: NodeId, g: Geo, tier: &Tier, tint: Option<Color>, i: usize) {
    let k = g.k;
    let r = BODY_R * tier.r * k;
    let name = format!("tier {i}");
    let fills = tier_fills(tier, tint);
    match tier.edge {
        Edge::Round => {
            if tier.shadow > 0.0 {
                let (x, y) = g.p(0.0, BODY_R * tier.shadow);
                b.add(knob, ellipse(&format!("{name} shadow"), x, y, r, vec![Paint::solid(css("rgba(0,0,0,0.45)"))]));
            }
            let mut n = ellipse(&name, g.c, g.c, r, fills);
            match tier.paint {
                TierPaint::Groove { color, width } => n = outline(n, css(color), (width * k).max(0.5), StrokeAlign::Center),
                TierPaint::Surface { finish, .. } => {
                    // The relief a lit div carries: contact shadows under it,
                    // a dark under-edge and a light lip — at the finish's own
                    // strength, in fractions of the tier.
                    let px = 2.0 * r;
                    let (under, lip) = finish.relief();
                    n.effects = vec![
                        draw::drop_shadow("#00000073", 0.0, px * 0.020, px * 0.045),
                        draw::drop_shadow("#00000042", 0.0, px * 0.055, px * 0.130),
                        shadow_alpha(draw::inner_shadow("#000000", 0.0, -px * 0.10, px * 0.16), under),
                        shadow_alpha(draw::inner_shadow("#ffffff", 0.0, px * 0.05, px * 0.10), lip),
                    ];
                    let (c, w) = tier.stroke.map_or((css("rgba(0,0,0,0.45)"), 0.6_f64.max(0.5 * k)), |(c, w)| (css(c), (w * k).max(0.5)));
                    n = outline(n, c, w, StrokeAlign::Inside);
                }
                TierPaint::Flat(_) => {
                    if let Some((c, w)) = tier.stroke {
                        n = outline(n, css(c), (w * k).max(0.5), StrokeAlign::Center);
                    }
                }
            }
            b.add(knob, n);
        }
        Edge::Toothed { depth, .. } => {
            let depth = BODY_R * depth * k;
            if tier.shadow > 0.0 {
                // Round, so the teeth turning over it don't drag it along.
                let (x, y) = g.p(0.0, BODY_R * tier.shadow);
                b.add(knob, ellipse(&format!("{name} shadow"), x, y, r - depth * 0.45, vec![Paint::solid(css("rgba(0,0,0,0.45)"))]));
            }
            // The disc inside the teeth, lit like a round tier; the teeth
            // turn over its edge.
            b.add(knob, ellipse(&name, g.c, g.c, r - depth + 0.5, fills));
        }
    }
}

/// A toothed tier's teeth: a band from the tooth roots out, shaded about
/// the shaft only (so it can turn), with the tier's outline.
fn add_teeth(b: &mut Builder<'_>, body: NodeId, g: Geo, tier: &Tier, (teeth, depth): (u32, f64), tint: Option<Color>, i: usize) {
    let r = BODY_R * tier.r * g.k;
    let depth = BODY_R * depth * g.k;
    let (x0, side) = (g.c - r - 2.0, 2.0 * r + 4.0);
    let local = side / 2.0;
    let pts = scallop(local, local, r, teeth, depth);
    let band = format!("{} {}", fmt_pts(&pts), circle_path_ccw(local, local, r - depth));
    let c = tier_color(tier, tint);
    let root = (r - depth) / local;
    let stops = [(0.0, hex_of(c)), (root, hex_of(c)), (1.0, hex_of(draw::mix(c, BLACK, 0.35)))];
    let fill = draw::radial_at(0.5, 0.5, 0.5, &refs(&stops));
    let mut n = vector(&format!("tier {i} teeth"), x0, x0, side, side, band, vec![fill]);
    if let Some((sc, w)) = tier.stroke {
        n = with_stroke_geometry(n, outline_band(local, local, &pts, (w * g.k).max(0.5)), css(sc));
    }
    b.add(body, n);
}

fn shadow_alpha(mut e: Effect, a: f64) -> Effect {
    e.color.a = a;
    e
}

/// The room's light: a soft blob on the lit tier and a hairline on its rim.
fn add_light(b: &mut Builder<'_>, knob: NodeId, g: Geo, d: f64, lit: &Specular) {
    let (w, h) = (d * lit.w, d * lit.h);
    let (cx, cy) = (g.c + d * lit.dx, g.c + d * lit.dy);
    let fill = match lit.fill {
        SpecFill::Ellipse(s) => {
            // CSS `ellipse at 50% 50%` reaches the box corners.
            let o = owned(s);
            draw::radial_at(0.5, 0.5, std::f64::consts::FRAC_1_SQRT_2, &refs(&o))
        }
        SpecFill::Linear(a, s) => draw::linear(a, &refs(&owned(s))),
    };
    let mut blob = Node::new(NodeKind::Ellipse, "light");
    blob.transform = Transform::translate(cx - w / 2.0, cy - h / 2.0);
    blob.size = Size { width: w, height: h };
    blob.fills = vec![fill];
    b.add(knob, draw::rotated(blob, lit.rotate));
    if let Some(rim) = lit.rim {
        // On the lit tier's own edge, upper left to upper right.
        let rr = BODY_R * lit.r * 0.965;
        let arc = draw::sector(g.c, g.c, (rr - 0.5) * g.k, (rr + 0.5) * g.k, -51.2, 25.5);
        b.add(knob, vector("rim light", 0.0, 0.0, g.size(), g.size(), arc, vec![Paint::solid(css(rim))]));
    }
}

fn add_flutes(b: &mut Builder<'_>, body: NodeId, g: Geo, d: f64, fl: &Flutes) {
    let n = flute_count(fl.count, d * fl.to);
    let (inner, outer) = (BODY_R * fl.from, BODY_R * fl.to);
    let ridge = |a: f64| seg(g.polar(inner, a), g.polar(outer, a), fl.width * g.k);
    let step = 360.0 / f64::from(n);
    let lit: String = (0..n).map(|i| ridge(f64::from(i) * step)).collect();
    let mut node = vector("knurl", 0.0, 0.0, g.size(), g.size(), lit, vec![Paint::solid(css(fl.stroke))]);
    if let Some(dark) = fl.shadow {
        let shade: String = (0..n).map(|i| ridge(f64::from(i) * step + step / 2.0)).collect();
        node = with_stroke_geometry(node, shade, css(dark));
    }
    b.add(body, node);
}

/// How far a Marconi wing overhangs, and how far its tail runs back.
const WING_REACH: f64 = 1.26;
const WING_TAIL: f64 = 0.66;
/// The chicken-head's beak reach and body lobe, multiples of the radius.
const BEAK_REACH: f64 = 1.25;
const BEAK_LOBE: f64 = 0.62;
const BEAK_SHOULDER: f64 = 0.62;

/// An index colour that still reads on a variant: a white line on a light
/// cap goes dark, a dark line on a dark moulding goes light.
fn legible(color: &str, on: Option<Color>) -> Color {
    let c = css(color);
    match on {
        Some(t) if luminance(t) > 0.45 && luminance(c) > 0.6 => css("#1c1c1e"),
        Some(t) if luminance(t) < 0.2 && luminance(c) < 0.2 => css("#f2f2f0"),
        _ => c,
    }
}

/// A bar index as `(dark outline, bar)` paths.
fn bar(g: Geo, from: f64, to: f64, width: f64) -> (String, String) {
    let (x, y) = g.p(-width / 2.0, -(BODY_R * to));
    let (w, h) = (width * g.k, BODY_R * (to - from) * g.k);
    let r = (width / 2.4).min(1.6) * g.k;
    let o = 0.275 * g.k;
    (rrect(x - o, y - o, w + 2.0 * o, h + 2.0 * o, r + o), rrect(x, y, w, h, r))
}

/// A part's soft contact shadow: its outline `d` in black, blurred — no
/// offset, so it stays under the part whichever way the part points.
///
/// `shape(o)` is the part's outline pushed out by `o` units. The shadow is
/// one vector — the outline itself, hidden under the part — with a
/// zero-offset drop shadow the renderer draws round its real outline,
/// reaching about `blur` units out.
fn contact(b: &mut Builder<'_>, body: NodeId, g: Geo, name: &str, shape: &dyn Fn(f64) -> String, alpha: f64, blur: f64) {
    let shade = Color { a: alpha, ..BLACK };
    let mut n = vector(name, 0.0, 0.0, g.size(), g.size(), shape(0.0), vec![Paint::solid(shade)]);
    n.effects = vec![Effect {
        kind: document_proto::EffectKind::DropShadow,
        visible: true,
        color: shade,
        offset_x: 0.0,
        offset_y: 0.0,
        radius: 2.0 * blur * g.k,
        spread: 0.0,
    }];
    b.add(body, n);
}

/// The index: every part that reads the value, drawn pointing up into the
/// turning `body`.
#[allow(clippy::too_many_lines)]
fn add_index(b: &mut Builder<'_>, body: NodeId, g: Geo, spec: &KnobSpec, tint: Option<Color>, labels: &[String]) {
    let side = g.size();
    let pts = |p: &[(f64, f64)]| fmt_pts(&p.iter().map(|(u, v)| g.p(*u, *v)).collect::<Vec<_>>());
    let node = |name: &str, d: String, fills: Vec<Paint>| vector(name, 0.0, 0.0, side, side, d, fills);
    // What the cap's tier is tinted — the colour an index has to read on.
    let face = tint.filter(|_| !matches!(spec.index, Index::Wing { .. }));
    match spec.index {
        Index::Bar { from, to, width, color } => {
            let (edge, line) = bar(g, from, to, width);
            let (fill, fill_color) = match spec.collar_index {
                // The 1073: the ring's slim dark index under the cap's white one.
                Some(Index::Bar { from: cf, to: ct, width: cw, color: cc }) => (bar(g, cf, ct, cw).1 + &edge, css(cc)),
                _ => (edge, css("rgba(0,0,0,0.38)")),
            };
            b.add(body, with_stroke_geometry(node("index", fill, vec![Paint::solid(fill_color)]), line, legible(color, face)));
        }
        Index::Blade { to, half_width, color } => {
            let (l, hw) = (BODY_R * to, BODY_R * half_width);
            let blade = |o: f64| pts(&[(-hw - o, o), (hw + o, o), (hw * 0.35 + o, -l - o), (-hw * 0.35 - o, -l - o)]);
            contact(b, body, g, "index shadow", &blade, 0.35, 1.2);
            b.add(body, node("index", blade(0.0), vec![Paint::solid(legible(color, face))]));
        }
        Index::Taper { from, to, hub_half, tip_half, color } => {
            let (f, t) = (BODY_R * from, BODY_R * to);
            let shape = |dx: f64, dy: f64| {
                pts(&[(-hub_half + dx, -f + dy), (hub_half + dx, -f + dy), (tip_half + dx, -t + dy), (-tip_half + dx, -t + dy)])
            };
            let shadow = node("index", shape(0.7, 1.0), vec![Paint::solid(css("rgba(0,0,0,0.34)"))]);
            b.add(body, with_stroke_geometry(shadow, shape(0.0, 0.0), legible(color, face)));
        }
        Index::Nose { color } => {
            let w = BODY_R * 0.30;
            let (base, tip) = (BODY_R * 0.84, BODY_R + 9.0);
            let nose = |o: f64| pts(&[(-w - o, -base + o), (w + o, -base + o), (w * 0.34 + o, -tip - o), (-w * 0.34 - o, -tip - o)]);
            let tone = face.map_or_else(|| css("#232327"), |t| draw::mix(t, BLACK, 0.2));
            contact(b, body, g, "nose shadow", &nose, 0.5, 2.0);
            b.add(body, node("nose", nose(0.0), vec![Paint::solid(tone)]));
            let (x, y) = g.p(-1.2, -(BODY_R + 7.0));
            let line = rrect(x, y, 2.4 * g.k, BODY_R * 0.55 * g.k, g.k);
            b.add(body, node("index", line, vec![Paint::solid(legible(color, face))]));
        }
        Index::Wing { color, body: disc } => {
            let w = BODY_R * disc * 0.46;
            let (tail, reach) = (BODY_R * WING_TAIL, BODY_R * WING_REACH);
            let wing = |o: f64| pts(&[(-w - o, tail + o), (w + o, tail + o), (w * 0.64 + o, -reach - o), (-w * 0.64 - o, -reach - o)]);
            // The moulding's dark parting line, and the shadow it throws on
            // the disc under it.
            contact(b, body, g, "wing shadow", &|o| wing(0.7 + o), 0.55, 2.6);
            b.add(body, node("wing edge", wing(0.7), vec![Paint::solid(css("rgba(0,0,0,0.72)"))]));
            let paint = tint.unwrap_or_else(|| css("#3a3a40"));
            // The lit flank down the left side, so the wing reads as a raised
            // bar rather than a painted stripe; the right flank falls away.
            let x = |u: f64| (g.c + u * g.k) / side;
            let fill = linear(
                90.0,
                &[
                    (0.0, draw::mix(paint, WHITE, 0.22)),
                    (x(-0.5 * w), draw::mix(paint, WHITE, 0.22)),
                    (x(-0.5 * w) + 0.004, paint),
                    (x(0.55 * w), paint),
                    (x(0.62 * w), draw::mix(paint, BLACK, 0.3)),
                    (1.0, draw::mix(paint, BLACK, 0.3)),
                ],
            );
            b.add(body, node("wing", wing(0.0), vec![fill]));
            let (lx, ly) = g.p(-1.3, -(reach - 3.0));
            let line = rrect(lx, ly, 2.6 * g.k, (reach - 3.0) * g.k, g.k);
            b.add(body, node("index", line, vec![Paint::solid(legible(color, Some(paint)))]));
        }
        Index::Beak { color, .. } => {
            // `o` px of outline all round.
            let beak = |o: f64| {
                let lr = BODY_R * BEAK_LOBE * g.k + o;
                let (tx, ty) = g.p(0.0, -(BODY_R * BEAK_REACH) - 1.6 * o / g.k);
                let (sx, sy) = (lr * BEAK_SHOULDER.sin(), -lr * BEAK_SHOULDER.cos());
                format!(
                    "M {tx:.3} {ty:.3} L {:.3} {:.3} A {lr:.3} {lr:.3} 0 1 0 {:.3} {:.3} Z",
                    g.c - sx,
                    g.c + sy,
                    g.c + sx,
                    g.c + sy
                )
            };
            // The moulding's edge and the shadow of its beak turn with it.
            contact(b, body, g, "moulding shadow", &|o| beak((0.8 + o) * g.k), 0.5, 2.4);
            b.add(body, node("moulding edge", beak(0.8 * g.k), vec![Paint::solid(css("rgba(0,0,0,0.6)"))]));
            let moulding = tint.unwrap_or_else(|| css("#e8e2d4"));
            // Shaded about the shaft only; the fixed light over the lobe
            // does the rest.
            let lobe = BODY_R * BEAK_LOBE * g.k / side;
            let stops = [
                (0.0, hex_of(draw::mix(moulding, WHITE, 0.10))),
                (lobe * 1.8, hex_of(moulding)),
                (1.0, hex_of(draw::mix(moulding, BLACK, 0.30))),
            ];
            b.add(body, node("moulding", beak(0.0), vec![draw::radial_at(0.5, 0.5, 0.5, &refs(&stops))]));
            let (x, y) = g.p(-1.4, -(BODY_R * (BEAK_REACH - 0.14)));
            let line = rrect(x, y, 2.8 * g.k, BODY_R * (BEAK_REACH - 0.30) * g.k, 1.4 * g.k);
            b.add(body, node("index", line, vec![Paint::solid(legible(color, Some(moulding)))]));
        }
        Index::None => {
            // The Distressor: numerals printed on the skirt, standing on the
            // radius, increasing anticlockwise so the one under the panel's
            // fixed index is the value as the dial turns clockwise. Each is
            // real text in a frame turned to its angle.
            let r = BODY_R * 0.80;
            let size = 7.4 * g.k;
            for (s, n) in labels.iter().zip(positions(labels.len())) {
                if s.is_empty() {
                    continue;
                }
                let a = -(n - 0.5) * spec.sweep;
                let mut t = label("numeral", s, g.polar(r, a), size, "#15171a");
                let (w, h) = (t.size.width, t.size.height);
                let holder = b.add(body, draw::rotated(draw::frame("numeral", t.transform.tx, t.transform.ty, w, h), a));
                t.transform = Transform::IDENTITY;
                b.add(holder, t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_tint_entry() {
        for def in defs() {
            let spec = spec_of(def.id).expect("spec");
            assert!(!spec.variants.is_empty(), "{}", def.id);
            assert_eq!(spec.variants.len(), spec.tints.len(), "{}", def.id);
        }
    }

    #[test]
    fn css_colours_parse() {
        let c = css("rgba(255,255,255,0.5)");
        assert!((c.r - 1.0).abs() < 1e-9 && (c.a - 0.5).abs() < 1e-9);
        assert_eq!(hex_of(css("#102030")), "#102030ff");
    }

    #[test]
    fn a_knurl_is_cut_at_a_pitch_not_a_count() {
        assert_eq!(flute_count(54, 20.0), 19);
        assert_eq!(flute_count(54, 400.0), 54);
        assert_eq!(flute_count(54, 2.0), 8);
    }
}
