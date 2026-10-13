//! Kit family: meters. Ported from `fts-audio-ui::hardware::{vu, vu_faces,
//! vu_kit, vu_svg}` — see `KIT.md`.
//!
//! One component per VU face (`meter.vu_amber`, `meter.vu_ivory`,
//! `meter.vu_amber_blue`, `meter.vu_blue`, `meter.vu_backlit`). A face is a
//! printed card behind glass, lit from behind, with a needle on a pivot
//! below the card; mounted by default in the face's own frame. Variants pick
//! the frame (`chrome`, `brass`, `open` for the bare movement, …), the card
//! (`db` / `vu`, optionally with a frame: `db-chrome`), and for the backlit
//! movement the lamp colour (`red`, …).
//!
//! The needle is a `Needle` slot inside a square pivot frame centred on the
//! pivot, drawn pointing straight up (centre of travel). The control is a
//! `Gauge` over the card's printed range with its real sweep (±34°); bound to
//! a level stream, the binding's curve is set to the card's law
//! ([`scale::Card::curve`]) so the needle lands on the printed marks — see
//! [`scale`].

pub mod faces;
mod leds;
mod limiter_vu;
mod movements;
mod reduction;
pub mod scale;

use crate::document::proto::{
    Color, GradientStop, Node, NodeId, Paint, PaintKind, TextAlign, TextData, Transform,
};
use crate::surface::proto::{ControlKind, Curve, SlotRole, Source};
use crate::surface::widgets::Builder;

use self::faces::{Bezel, CardPaint, Ellipse, Face, Lin};
use self::scale::{Card, NEEDLE_LEN, PIVOT_X, PIVOT_Y, SWEEP_DEG, VU_H, VU_W};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// The card's width in the source's design px; frames are measured against it.
const CARD_W: f64 = 190.0;

/// This family's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let def = |id, title, description, variants, build| ComponentDef {
        id,
        family: Family::Meter,
        title,
        description,
        variants,
        default_size: 220.0,
        binds: "level stream",
        build,
    };
    let mut all = vec![
        def(
            "meter.vu_amber",
            "VU (amber)",
            "Warm ivory VU card under a yellow lamp, black scale with a red stretch above 0, in a black rack frame — the LA-2A's meter and most tube gear's. Needle meter for a level (or gain-reduction) stream.",
            VU_VARIANTS,
            build_amber,
        ),
        def(
            "meter.vu_ivory",
            "VU (ivory)",
            "Neutral ivory VU card under a white lamp in a black rack frame — the 1176's Modutec, an SSL's. The default period VU for a level stream.",
            VU_VARIANTS,
            build_ivory,
        ),
        def(
            "meter.vu_amber_blue",
            "dB meter (dbx)",
            "Amber card printed in blue with a plain, evenly spaced -40…+20 dB scale and no red — the dbx 160's decibel readout rather than a VU.",
            DB_VARIANTS,
            build_amber_blue,
        ),
        def(
            "meter.vu_blue",
            "VU (blue)",
            "Blue-lit VU card printed in white with a coral red stretch, in a black rack frame. For a unit that genuinely wore a blue face.",
            VU_VARIANTS,
            build_blue,
        ),
        def(
            "meter.vu_backlit",
            "VU (backlit)",
            "Modern console movement: near-black card behind heavy glass, lamp pooled behind the needle, white print, glowing needle, deep recessed frame. Lamp colour is a variant (blue, red, green, amber).",
            BACKLIT_VARIANTS,
            build_backlit,
        ),
    ];
    all.extend(leds::defs());
    all.extend(limiter_vu::defs());
    all.extend(reduction::defs());
    all.extend(movements::defs());
    all
}

/// Frame variants (card = the face's own), then the other card with each.
const VU_VARIANTS: &[&str] = &[
    "black_rack",
    "slim",
    "chrome",
    "brass",
    "satin_plate",
    "recessed",
    "lit_amber",
    "lit_blue",
    "lit_chrome",
    "open",
    "db",
    "db-black_rack",
    "db-slim",
    "db-chrome",
    "db-brass",
    "db-satin_plate",
    "db-recessed",
    "db-lit_amber",
    "db-lit_blue",
    "db-lit_chrome",
    "db-open",
];
const DB_VARIANTS: &[&str] = &[
    "black_rack",
    "slim",
    "chrome",
    "brass",
    "satin_plate",
    "recessed",
    "lit_amber",
    "lit_blue",
    "lit_chrome",
    "open",
    "vu",
    "vu-black_rack",
    "vu-slim",
    "vu-chrome",
    "vu-brass",
    "vu-satin_plate",
    "vu-recessed",
    "vu-lit_amber",
    "vu-lit_blue",
    "vu-lit_chrome",
    "vu-open",
];
const BACKLIT_VARIANTS: &[&str] = &[
    "blue",
    "red",
    "green",
    "amber",
    "blue-open",
    "red-open",
    "green-open",
    "amber-open",
    "black_rack",
    "slim",
    "chrome",
    "brass",
    "satin_plate",
    "recessed",
    "lit_amber",
    "lit_blue",
    "lit_chrome",
    "open",
    "db",
];

fn build_amber(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    build(&faces::AMBER, b, parent, p)
}
fn build_ivory(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    build(&faces::IVORY, b, parent, p)
}
fn build_amber_blue(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    build(&faces::AMBER_BLUE, b, parent, p)
}
fn build_blue(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    build(&faces::BLUE, b, parent, p)
}
fn build_backlit(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    build(&faces::BACKLIT, b, parent, p)
}

/// What a variant string asks for: `-`-separated tokens naming a card, a
/// frame (or `open`), and a lamp colour.
struct Choice {
    card: Card,
    bezel: Option<&'static Bezel>,
    tint: Option<Color>,
}

fn choose(face: &Face, variant: Option<&str>) -> Choice {
    let mut c = Choice {
        card: face.card,
        bezel: Some(face.bezel),
        tint: None,
    };
    for tok in variant.unwrap_or("").split('-').filter(|t| !t.is_empty()) {
        match tok {
            "vu" => c.card = Card::Vu,
            "db" => c.card = Card::Decibels,
            "open" => c.bezel = None,
            _ => {
                if let Some(z) = faces::BEZELS.iter().find(|z| z.name == tok) {
                    c.bezel = Some(z);
                } else if let Some((_, hex)) = faces::TINTS.iter().find(|(n, _)| *n == tok)
                    && face.tintable
                {
                    c.tint = Some(draw::hex(hex));
                }
            }
        }
    }
    c
}

// ── colour & paint ───────────────────────────────────────────────────────

/// A CSS colour as the source wrote it: `#hex` or `rgba(r,g,b,a)`.
fn css(s: &str) -> Color {
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|r| r.strip_suffix(')')) {
        let v: Vec<f64> = inner
            .split(',')
            .map(|x| x.trim().parse().unwrap_or(0.0))
            .collect();
        return Color {
            r: v[0] / 255.0,
            g: v[1] / 255.0,
            b: v[2] / 255.0,
            a: v.get(3).copied().unwrap_or(1.0),
        };
    }
    draw::hex(s)
}

fn to_hex(c: Color) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b), b(c.a))
}

fn solid(c: Color) -> Paint {
    Paint::solid(c)
}

fn lin(l: &Lin) -> Paint {
    let hexes: Vec<(f64, String)> = l.stops.iter().map(|(p, c)| (*p, to_hex(css(c)))).collect();
    let refs: Vec<(f64, &str)> = hexes.iter().map(|(p, c)| (*p, c.as_str())).collect();
    draw::linear(l.angle, &refs)
}

/// `radial-gradient(ellipse at x y, …)` with CSS's default farthest-corner
/// size: the ellipse through the farthest corner with the farthest-side
/// aspect, i.e. radii `√2 ×` the distances to the farthest sides. `x`, `y`
/// in the box's unit square.
fn ellipse(x: f64, y: f64, stops: &[(f64, Color)]) -> Paint {
    let rx = std::f64::consts::SQRT_2 * x.max(1.0 - x);
    let ry = std::f64::consts::SQRT_2 * y.max(1.0 - y);
    let t = Transform {
        a: 2.0 * rx,
        b: 0.0,
        tx: x - rx,
        c: 0.0,
        d: 2.0 * ry,
        ty: y - ry,
    };
    let stops = stops
        .iter()
        .map(|(p, c)| GradientStop {
            position: *p,
            color: *c,
        })
        .collect();
    Paint {
        kind: PaintKind::RadialGradient {
            stops,
            transform: Some(t),
        },
        ..Paint::solid(Color::rgba(0.0, 0.0, 0.0, 0.0))
    }
}

fn ellipse_css(e: &Ellipse) -> Paint {
    let s: Vec<(f64, Color)> = e.stops.iter().map(|(p, c)| (*p, css(c))).collect();
    ellipse(e.x, e.y, &s)
}

/// A lamp's glow: `color` at the bulb fading to nothing at `reach`.
fn glow(x: f64, y: f64, reach: f64, color: Color) -> Paint {
    ellipse(
        x / 100.0,
        y / 100.0,
        &[(0.0, color), (reach / 100.0, Color { a: 0.0, ..color })],
    )
}

fn effect(
    kind: crate::document::proto::EffectKind,
    c: Color,
    dx: f64,
    dy: f64,
    radius: f64,
) -> crate::document::proto::Effect {
    crate::document::proto::Effect {
        kind,
        visible: true,
        color: c,
        offset_x: dx,
        offset_y: dy,
        radius,
        spread: 0.0,
    }
}

fn drop(c: Color, dx: f64, dy: f64, r: f64) -> crate::document::proto::Effect {
    effect(crate::document::proto::EffectKind::DropShadow, c, dx, dy, r)
}

fn inner(c: Color, dx: f64, dy: f64, r: f64) -> crate::document::proto::Effect {
    effect(
        crate::document::proto::EffectKind::InnerShadow,
        c,
        dx,
        dy,
        r,
    )
}

fn print(
    name: &str,
    s: &str,
    (cx, cy): (f64, f64),
    size: f64,
    color: Color,
    (weight, spacing): (f64, f64),
) -> Node {
    let mut n = draw::label_at(name, s, cx, cy, size, "#000");
    n.fills = vec![solid(color)];
    if let crate::document::proto::NodeKind::Text(TextData {
        font_weight,
        letter_spacing,
        align,
        ..
    }) = &mut n.kind
    {
        *font_weight = Some(weight);
        *letter_spacing = spacing;
        *align = TextAlign::Center;
    }
    n
}

/// A straight stroke from `a` to `b` of width `w` as a closed quad.
fn quad(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-9);
    let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
    draw::polygon(&[
        (a.0 + nx, a.1 + ny),
        (b.0 + nx, b.1 + ny),
        (b.0 - nx, b.1 - ny),
        (a.0 - nx, a.1 - ny),
    ])
}

// ── the meter ────────────────────────────────────────────────────────────

fn build(face: &Face, b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let choice = choose(face, p.variant.as_deref());
    let card = choice.card;
    let bezel = choice.bezel;

    // Everything in design px (a 190 px card), then × `s` to the placed size.
    let card_h = CARD_W * VU_H / VU_W;
    let (outer_w, outer_h, inset) = match bezel {
        Some(z) => {
            let vent = if z.vent.is_some() { 8.0 } else { 0.0 };
            (
                CARD_W + 2.0 * z.depth + 10.0,
                5.0 + card_h + 2.0 * z.depth + vent + 4.0,
                5.0 + z.depth,
            )
        }
        None => (CARD_W, card_h, 0.0),
    };
    let size = p.size.unwrap_or(220.0).max(20.0);
    let s = size / outer_w;
    // Card units (the 100-wide viewBox) → px.
    let k = CARD_W * s / VU_W;

    // ── the frame ──
    let mut root = draw::frame(&p.name, 0.0, 0.0, outer_w * s, outer_h * s);
    if let Some(z) = bezel {
        root.fills = vec![lin(&z.frame)];
        root = draw::rounded(root, z.radius * s);
        let mut fx = vec![drop(css("rgba(0,0,0,0.55)"), 0.0, 3.0 * s, 7.0 * s)];
        if let Some(g) = z.glow {
            fx.insert(0, drop(css(g.color), 0.0, 0.0, g.spread * s));
            if let Some(core) = g.core {
                fx.insert(1, drop(css(core), 0.0, 0.0, g.spread * 0.35 * s));
            }
        }
        fx.push(inner(css("rgba(255,255,255,0.07)"), 0.0, 1.0 * s, 0.0));
        root.effects = fx;
    }
    let root = b.add(parent, root);

    if let Some(z) = bezel {
        // The chamfered opening: four mitred faces, lit from above (top in
        // shadow, bottom catching the light) so it reads as sunken.
        let (ow, oh, d) = (
            (CARD_W + 2.0 * z.depth) * s,
            (card_h + 2.0 * z.depth) * s,
            z.depth * s,
        );
        let faces = [
            (
                "chamfer top",
                z.top,
                vec![(0.0, 0.0), (ow, 0.0), (ow - d, d), (d, d)],
            ),
            (
                "chamfer left",
                z.left,
                vec![(0.0, 0.0), (d, d), (d, oh - d), (0.0, oh)],
            ),
            (
                "chamfer right",
                z.right,
                vec![(ow, 0.0), (ow, oh), (ow - d, oh - d), (ow - d, d)],
            ),
            (
                "chamfer bottom",
                z.bottom,
                vec![(0.0, oh), (d, oh - d), (ow - d, oh - d), (ow, oh)],
            ),
        ];
        for (name, color, pts) in faces {
            b.add(
                root,
                draw::path(
                    name,
                    5.0 * s,
                    5.0 * s,
                    ow,
                    oh,
                    draw::polygon(&pts),
                    draw::solid(color),
                ),
            );
        }
        // The louvre under the glass.
        if let Some(v) = z.vent {
            let (vw, vh) = (CARD_W * 0.30 * s, 4.0 * s);
            let mut vent = draw::frame(
                "vent",
                (outer_w * s - vw) / 2.0,
                (5.0 + card_h + 2.0 * z.depth + 4.0) * s,
                vw,
                vh,
            );
            vent.fills = vec![solid(css(v.dark))];
            vent.clips_content = true;
            let vent = b.add(root, draw::rounded(vent, 1.0 * s));
            let pitch = v.pitch * s;
            let mut d = String::new();
            let mut x = pitch / 2.0;
            while x < vw {
                d.push_str(&draw::polygon(&[
                    (x, 0.0),
                    ((x + pitch / 2.0).min(vw), 0.0),
                    ((x + pitch / 2.0).min(vw), vh),
                    (x, vh),
                ]));
                d.push(' ');
                x += pitch;
            }
            b.add(
                vent,
                draw::path("louvre", 0.0, 0.0, vw, vh, d, solid(css(v.light))),
            );
        }
    }

    // ── the card ──
    let (cw, ch) = (CARD_W * s, card_h * s);
    let tint = choice.tint;
    let card_paint = match (tint, face.card_paint) {
        (Some(t), _) => {
            let dark = draw::hex("#05070a");
            ellipse(0.5, 1.04, &[(0.0, draw::mix(dark, t, 0.30)), (0.62, dark)])
        }
        (None, CardPaint::Linear(l)) => lin(&l),
        (None, CardPaint::Radial(e)) => ellipse_css(&e),
    };
    let mut card_node = draw::frame("card", inset * s, inset * s, cw, ch);
    card_node.fills = vec![card_paint];
    card_node.clips_content = true;
    card_node.stroke = crate::document::proto::Stroke {
        paints: vec![solid(css("rgba(0,0,0,0.55)"))],
        weight: (1.5 * s).max(1.0),
        align: crate::document::proto::StrokeAlign::Inside,
        ..crate::document::proto::Stroke::default()
    };
    card_node.effects = vec![inner(css("rgba(0,0,0,0.45)"), 0.0, 0.0, 10.0 * s)];
    // A lit frame: the lamp's light spills out of the opening onto it.
    if let Some(g) = bezel.and_then(|z| z.glow) {
        card_node = draw::emissive(card_node, css(g.color), 0.5, g.spread * s);
    }
    // The `glass` option: a glass post-effect over the window (GPU
    // renderers only — the card reads the same without it).
    if p.flag("glass") {
        card_node.shader = Some(crate::document::proto::ShaderFx {
            shader: "glass".into(),
            params: std::iter::once(("strength".to_string(), 0.4)).collect(),
            inputs: Vec::new(),
        });
    }
    let card_id = b.add(root, draw::rounded(card_node, 3.0 * s));

    // The lamp behind the card: the wash, then the bulb's hotspot inside it.
    // Where the real lamp shows through, it gives off light too.
    if let Some(l) = face.lamp {
        let color = tint.map_or_else(|| css(l.color), |t| draw::with_alpha(t, 0.5));
        let mut wash = draw::rect("lamp", 0.0, 0.0, cw, ch, glow(l.x, l.y, l.reach, color));
        if l.light.0 > 0.0 {
            wash = draw::emissive(wash, color, l.light.0, 18.0 * s);
            // `flicker`: the lamp's light wavers like a filament's.
            if p.flag("flicker") {
                wash.motion.push(crate::surface::kit::motion::flicker(0.06));
            }
        }
        b.add(card_id, wash);
        if let Some((c, x, y, reach)) = l.core {
            let mut spot = draw::rect("lamp hotspot", 0.0, 0.0, cw, ch, glow(x, y, reach, css(c)));
            if l.light.1 > 0.0 {
                spot = draw::emissive(spot, css(c), l.light.1, 10.0 * s);
            }
            b.add(card_id, spot);
        }
    }

    let ink = css(face.ink);
    let hot = css(face.hot);
    let px = |(x, y): (f64, f64)| (x * k, y * k);

    // The printed arc and the red stretch above 0 VU.
    let ticks = card.ticks();
    let (first, last) = (ticks[0].0, ticks[ticks.len() - 1].0);
    let r_arc = (NEEDLE_LEN - 6.0) * k;
    let arc = |from: f64, to: f64, w: f64| {
        draw::sector(
            PIVOT_X * k,
            PIVOT_Y * k,
            r_arc - w * k / 2.0,
            r_arc + w * k / 2.0,
            card.angle(from),
            card.angle(to),
        )
    };
    b.add(
        card_id,
        draw::with_opacity(
            draw::path(
                "scale arc",
                0.0,
                0.0,
                cw,
                ch,
                arc(first, last, 0.8),
                solid(ink),
            ),
            0.85,
        ),
    );
    if let Some(from) = card.hot_from() {
        b.add(
            card_id,
            draw::path(
                "red stretch",
                0.0,
                0.0,
                cw,
                ch,
                arc(from, last, 1.6),
                solid(hot),
            ),
        );
    }

    // Ticks (majors longer and numbered) and numerals.
    let is_hot = |v: f64| card.hot_from().is_some_and(|from| v > from);
    let (mut ink_ticks, mut hot_ticks) = (String::new(), String::new());
    let mut labels = p.scale.iter().flatten();
    for &(v, label, major) in ticks {
        let a = px(scale::point(card, v, 6.0));
        let e = px(scale::point(card, v, if major { 13.0 } else { 10.0 }));
        let q = quad(a, e, if major { 1.1 } else { 0.6 } * k);
        let dst = if is_hot(v) {
            &mut hot_ticks
        } else {
            &mut ink_ticks
        };
        dst.push_str(&q);
        dst.push(' ');
        if major {
            // An authored scale replaces the printed numbers, in order.
            let text = labels.next().map_or(label, String::as_str);
            let (lx, ly) = px(scale::point(card, v, 22.0));
            b.add(
                card_id,
                print(
                    "numeral",
                    text,
                    (lx, ly + 0.1 * k),
                    6.0 * k,
                    if is_hot(v) { hot } else { ink },
                    (600.0, 0.0),
                ),
            );
        }
    }
    b.add(
        card_id,
        draw::path("ticks", 0.0, 0.0, cw, ch, ink_ticks, solid(ink)),
    );
    if !hot_ticks.is_empty() {
        b.add(
            card_id,
            draw::path("red ticks", 0.0, 0.0, cw, ch, hot_ticks, solid(hot)),
        );
    }

    // The legend: the instrument ("VU", "dB"), not what is being metered.
    let legend = p
        .legend
        .clone()
        .unwrap_or_else(|| card.legend().to_string());
    if !legend.trim().is_empty() {
        b.add(
            card_id,
            print(
                "legend",
                &legend,
                (VU_W * 0.5 * k, (VU_H * 0.93 - 3.4) * k),
                5.0 * k,
                ink,
                (400.0, 0.6 * k),
            ),
        );
    }

    // The needle, in a square frame centred on the pivot so the `Needle`
    // slot rotates about it; drawn at the centre of travel (straight up). A
    // lit needle is a group — the frame's rotation carries its children — of
    // layered halo lines, widest and faintest first, under the needle itself.
    let w = face.needle.width * k;
    let len = NEEDLE_LEN * k;
    let halo = face.needle.halo.map(|(c, spread)| {
        (
            tint.map_or_else(|| css(c), |t| draw::with_alpha(t, 0.34)),
            spread,
        )
    });
    let r = len + w * halo.map_or(1.0, |(_, spread)| spread);
    let pivot = b.add(
        card_id,
        draw::frame(
            "needle pivot",
            PIVOT_X * k - r,
            PIVOT_Y * k - r,
            2.0 * r,
            2.0 * r,
        ),
    );
    let line = |name: &str, lw: f64, color: Color| {
        draw::rounded(
            draw::rect(
                name,
                r - lw / 2.0,
                r - len - lw / 2.0,
                lw,
                len + lw,
                solid(color),
            ),
            lw / 2.0,
        )
    };
    let needle_color = css(face.needle.color);
    let needle = match halo {
        None => b.add(pivot, line("needle", w, needle_color)),
        Some((color, spread)) => {
            let group = b.add(pivot, draw::frame("needle", 0.0, 0.0, 2.0 * r, 2.0 * r));
            // Each layer's alpha is a share of the source's halo, so the
            // stack peaks at its colour along the needle and fades outward.
            for (i, (f, a)) in [(1.0, 0.30), (0.62, 0.45), (0.34, 0.7)]
                .into_iter()
                .enumerate()
            {
                let lw = w * (1.0 + (spread - 1.0) * f);
                b.add(
                    group,
                    line(
                        &format!("halo {i}"),
                        lw,
                        Color {
                            a: color.a * a,
                            ..color
                        },
                    ),
                );
            }
            let glow = tint.map_or(color, |t| draw::mix(t, draw::hex("#ffffff"), 0.4));
            b.add(
                group,
                draw::emissive(line("needle core", w, needle_color), glow, 0.7, 4.0 * w),
            );
            group
        }
    };
    // The hub, on the card's bottom edge: only its top half shows.
    let hub = draw::circle(
        "hub",
        PIVOT_X * k,
        VU_H * k,
        face.needle.hub_r * k,
        solid(css(face.needle.color)),
    );
    b.add(card_id, draw::with_opacity(hub, face.needle.hub_opacity));

    // The frame lip's shadow on the card (from the top and the left), then
    // the glass over all of it.
    if let Some(sh) = bezel.and_then(|z| z.shade) {
        let c = css(sh.color);
        let mut n = draw::rect("lip shadow", 0.0, 0.0, cw, ch, solid(Color { a: 0.0, ..c }));
        n.effects = vec![
            inner(c, 0.0, sh.top * s, sh.top * 1.7 * s),
            inner(c, sh.side * s, 0.0, sh.side * 1.8 * s),
        ];
        b.add(card_id, draw::rounded(n, 3.0 * s));
    }
    if let Some(g) = &face.glass {
        b.add(
            card_id,
            draw::rounded(draw::rect("glass", 0.0, 0.0, cw, ch, lin(g)), 3.0 * s),
        );
    }

    // The room's reflection on the frame itself, over everything.
    if let Some(sheen) = bezel.and_then(|z| z.sheen) {
        let r = bezel.map_or(0.0, |z| z.radius) * s;
        b.add(
            root,
            draw::rounded(
                draw::rect(
                    "frame sheen",
                    0.0,
                    0.0,
                    outer_w * s,
                    outer_h * s,
                    lin(&sheen),
                ),
                r,
            ),
        );
    }

    // A level stream reads through the card's law, so the needle lands on
    // the printed marks; any other binding (or an authored curve) is kept.
    let binding = p.binding.clone().map(|mut bd| {
        if matches!(bd.source, Source::Stream { .. }) && bd.curve == Curve::Linear {
            bd.curve = card.curve();
        }
        bd
    });
    let (floor_db, ceiling_db) = card.range();
    // A switched meter (`Place::select`) reads through the same law.
    let mut control = p
        .control(ControlKind::Gauge {
            sweep_degrees: 2.0 * SWEEP_DEG,
            floor_db,
            ceiling_db,
        })
        .slot(SlotRole::Needle, needle.0);
    control.binding = binding;
    // A VU is calibrated: 0 VU = −18 dBFS on a level stream.
    control.reference_dbfs = Some(crate::surface::proto::VU_REFERENCE_DBFS);
    b.controls.insert(root, control);
    root
}
