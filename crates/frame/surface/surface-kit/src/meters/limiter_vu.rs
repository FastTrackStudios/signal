//! `meter.vu_limiter`: the FET limiter's meter, drawn from photographs of
//! the 1176's movements (the early Weston, the later Modutec).
//!
//! A wide card in a deep black frame, the needle's pivot hidden under a
//! black cover strip along the bottom of the window. The card prints two
//! scales on one arc: the VU scale above it (`-20 … +3`, the plus side in
//! red over a heavy red band) and the percentage scale under it (`0 … 100`,
//! 100 at 0 VU), with `VU` in each top corner — black on the left, red on
//! the right. The arc is taller and steeper than a console VU's: the pivot
//! sits just under the card.
//!
//! Variants: `cream` (the blackface's card, lit from above by two small
//! bulbs), `amber` (the late silver unit's backlit amber card), `ivory`
//! (the early unit's Weston card, a row of dots under the arc). The needle
//! follows the VU card's law (`scale::Card::Vu`) over `-20..+3`, so it lands
//! on the printed marks; the `glass` option puts the glass shader over the
//! window. Size = the frame's width.

use document_proto::{NodeId, TextAlign, TextData};
use surface_proto::{ControlKind, Curve, SlotRole, Source};
use surface_widgets::Builder;

use super::scale::Card;
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// Half the needle's travel, degrees.
const SWEEP: f64 = 30.0;
/// Frame width : height (measured 320 × 180 px).
const ASPECT: f64 = 1.74;

struct Look {
    card: [&'static str; 2],
    ink: &'static str,
    red: &'static str,
    /// `(x, y, reach, colour)` lamp glows on the card, fractions of it.
    lamps: &'static [(f64, f64, f64, &'static str)],
    /// How much light the card gives off (0 = none).
    emit: f64,
    dots: bool,
}

fn look(v: Option<&str>) -> Look {
    match v {
        Some("amber") => Look {
            card: ["#f7d68a", "#e4a94a"],
            ink: "#2a1d0c",
            red: "#c62f1c",
            lamps: &[(0.5, 0.55, 0.75, "#fff2c066")],
            emit: 0.35,
            dots: false,
        },
        Some("ivory") => Look {
            card: ["#f3ecd8", "#e2d7bb"],
            ink: "#1d1a16",
            red: "#c8321f",
            lamps: &[(0.5, 0.0, 0.9, "#fffbef40")],
            emit: 0.0,
            dots: true,
        },
        _ => Look {
            card: ["#f2ecda", "#e1d7bc"],
            ink: "#1c1a17",
            red: "#d3321f",
            lamps: &[(0.3, -0.05, 0.55, "#fffaeb59"), (0.7, -0.05, 0.55, "#fffaeb59")],
            emit: 0.0,
            dots: false,
        },
    }
}

fn print(name: &str, s: &str, (cx, cy): (f64, f64), size: f64, color: &str, weight: f64) -> document_proto::Node {
    let mut n = draw::label_at(name, s, cx, cy, size, color);
    if let document_proto::NodeKind::Text(TextData { font_weight, align, .. }) = &mut n.kind {
        *font_weight = Some(weight);
        *align = TextAlign::Center;
    }
    n
}

fn seg(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-9);
    let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
    format!("{} ", draw::polygon(&[(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]))
}

fn circle_d(cx: f64, cy: f64, r: f64) -> String {
    format!("M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} Z ", cx + r, cx - r, cx + r)
}

#[allow(clippy::too_many_lines)]
fn vu_limiter(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(186.0).max(40.0);
    let h = w / ASPECT;
    // `-`-separated tokens: a card, and `open` for the bare movement.
    let v = p.variant.as_deref().unwrap_or("");
    let open = v.split('-').any(|t| t == "open");
    let lk = look(v.split('-').find(|t| *t != "open"));
    let t = if open { 0.0 } else { w * 0.034 };
    let (ww, wh) = (w - 2.0 * t, h - 2.0 * t);
    let (cw, ch) = (ww, wh * 0.84);

    // ── the frame and the window ──
    let mut root = draw::frame(&p.name, 0.0, 0.0, w, h);
    if !open {
        root.fills = vec![draw::linear(180.0, &[(0.0, "#2b2b2e"), (0.5, "#161618"), (1.0, "#0b0b0c")])];
        root = draw::with_effects(
            draw::stroked(draw::rounded(root, w * 0.012), "#000000", 1.0),
            vec![draw::drop_shadow("#0000008c", 0.0, h * 0.03, h * 0.08), draw::inner_shadow("#ffffff1f", 0.0, 1.0, 0.0)],
        );
    }
    let root = b.add(parent, root);
    if !open {
        // The window's deep chamfer: the top in shadow, the bottom catching light.
        let well = draw::rect("window", t - 2.0, t - 2.0, ww + 4.0, wh + 4.0, draw::linear(180.0, &[(0.0, "#000000"), (1.0, "#3a3a3d")]));
        b.add(root, draw::rounded(well, 2.0));
    }

    let mut card = draw::frame("card", t, t, cw, ch);
    card.fills = vec![draw::linear(180.0, &[(0.0, lk.card[0]), (1.0, lk.card[1])])];
    card.clips_content = true;
    card.effects = vec![draw::inner_shadow("#00000073", 0.0, ch * 0.06, ch * 0.12), draw::inner_shadow("#00000040", ch * 0.04, 0.0, ch * 0.08)];
    if lk.emit > 0.0 {
        card = draw::emissive(card, draw::hex(lk.card[0]), lk.emit, ch * 0.3);
    }
    if p.flag("glass") {
        card.shader = Some(document_proto::ShaderFx { shader: "glass".into(), params: std::iter::once(("strength".to_string(), 0.4)).collect(), inputs: Vec::new() });
    }
    let card = b.add(root, card);
    for (i, (x, y, reach, c)) in lk.lamps.iter().enumerate() {
        let glow = draw::radial_at(*x, *y, *reach, &[(0.0, c), (1.0, "#ffffff00")]);
        b.add(card, draw::rect(&format!("lamp {i}"), 0.0, 0.0, cw, ch, glow));
    }

    // ── the print ──
    // The kit's movement geometry: the pivot under the card's centre.
    let (px, py) = (cw * super::scale::PIVOT_X / super::scale::VU_W, cw * super::scale::PIVOT_Y / super::scale::VU_W);
    let rr = py - ch * 0.34;
    let card_law = Card::Vu;
    let ang = |v: f64| (card_law.fraction(v.clamp(-20.0, 3.0)) * 2.0 - 1.0) * SWEEP;
    let at = |r: f64, v: f64| draw::polar(px, py, r, ang(v));
    let fs = ch * 0.105;
    // The heavy red band under the plus side, then the arc.
    let band = draw::sector(px, py, rr - ch * 0.075, rr + ch * 0.004, ang(0.0), ang(3.0));
    b.add(card, draw::path("red band", 0.0, 0.0, cw, ch, band, draw::solid(lk.red)));
    let arc = draw::sector(px, py, rr - ch * 0.006, rr + ch * 0.006, ang(-20.0), ang(3.0));
    b.add(card, draw::path("scale arc", 0.0, 0.0, cw, ch, arc, draw::solid(lk.ink)));
    // VU ticks outward from the arc, numbered.
    let vu: &[(f64, &str)] = &[
        (-20.0, "-20"),
        (-10.0, "-10"),
        (-7.0, "-7"),
        (-5.0, "-5"),
        (-3.0, "-3"),
        (-2.0, ""),
        (-1.0, "-1"),
        (0.0, "0"),
        (1.0, "+1"),
        (2.0, "+2"),
        (3.0, "+3"),
    ];
    let (mut black, mut red) = (String::new(), String::new());
    for &(v, l) in vu {
        let len = if l.is_empty() { 0.05 } else { 0.085 };
        let dst = if v > 0.0 { &mut red } else { &mut black };
        dst.push_str(&seg(at(rr, v), at(rr + ch * len, v), (ch * 0.014).max(0.6)));
        if !l.is_empty() {
            let (x, y) = at(rr + ch * 0.16, v);
            b.add(card, print("numeral", l, (x, y), fs, if v > 0.0 { lk.red } else { lk.ink }, 500.0));
        }
    }
    for v in [-15.0, -8.0, -6.0, -4.0, -0.5, 0.5, 1.5, 2.5] {
        let dst = if v > 0.0 { &mut red } else { &mut black };
        dst.push_str(&seg(at(rr, v), at(rr + ch * 0.04, v), (ch * 0.009).max(0.5)));
    }
    // The percentage scale under the arc: 100 % at 0 VU.
    let pct: &[(f64, &str)] = &[(0.0, "0"), (20.0, "20"), (40.0, "40"), (60.0, "60"), (80.0, "80"), (100.0, "100")];
    for &(pc, l) in pct {
        let v = if pc <= 0.0 { -20.0 } else { 20.0 * (pc / 100.0).log10() };
        black.push_str(&seg(at(rr, v), at(rr - ch * 0.05, v), (ch * 0.011).max(0.5)));
        let (x, y) = at(rr - ch * 0.14, v);
        b.add(card, print("percent", l, (x, y), fs * 0.92, lk.ink, 500.0));
    }
    if lk.dots {
        let mut d = String::new();
        for &(v, l) in vu {
            if !l.is_empty() {
                let (x, y) = at(rr - ch * 0.075, v);
                d.push_str(&circle_d(x, y, ch * 0.018));
            }
        }
        b.add(card, draw::path("dots", 0.0, 0.0, cw, ch, d, draw::solid(lk.ink)));
    }
    b.add(card, draw::path("ticks", 0.0, 0.0, cw, ch, black, draw::solid(lk.ink)));
    b.add(card, draw::path("red ticks", 0.0, 0.0, cw, ch, red, draw::solid(lk.red)));
    b.add(card, print("legend", "VU", (cw * 0.075, ch * 0.13), fs, lk.ink, 500.0));
    b.add(card, print("legend", "VU", (cw * 0.925, ch * 0.13), fs, lk.red, 500.0));

    // ── the needle, on a pivot under the cover strip ──
    let len = rr + ch * 0.1;
    let nw = (ch * 0.016).max(0.9);
    let piv = b.add(card, draw::frame("needle pivot", px - len, py - len, 2.0 * len, 2.0 * len));
    let needle = draw::rect("needle", len - nw / 2.0, 0.0, nw, len, draw::solid("#141312"));
    let needle = b.add(piv, draw::with_effects(needle, vec![draw::drop_shadow("#0000004d", ch * 0.02, ch * 0.03, ch * 0.02)]));
    // The frame's lip shading the top of the card.
    b.add(card, draw::rect("lip shadow", 0.0, 0.0, cw, ch * 0.08, draw::linear(180.0, &[(0.0, "#00000059"), (1.0, "#00000000")])));
    b.add(card, draw::rect("glass", 0.0, 0.0, cw, ch, draw::linear(160.0, &[(0.0, "#ffffff2e"), (0.35, "#ffffff08"), (0.6, "#ffffff00"), (1.0, "#ffffff0d")])));

    // The cover strip over the movement, across the bottom of the window.
    let cover = draw::rect("cover", t, t + ch, cw, wh - ch, draw::linear(180.0, &[(0.0, "#050505"), (0.3, "#18181a"), (1.0, "#0e0e0f")]));
    b.add(root, draw::with_effects(cover, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]));

    let binding = p.binding.clone().map(|mut bd| {
        if matches!(bd.source, Source::Stream { .. }) && bd.curve == Curve::Linear {
            bd.curve = card_law.curve();
        }
        bd
    });
    let (floor_db, ceiling_db) = card_law.range();
    let mut control = p.control(ControlKind::Gauge { sweep_degrees: 2.0 * SWEEP, floor_db, ceiling_db }).slot(SlotRole::Needle, needle.0);
    control.binding = binding;
    control.reference_dbfs = Some(surface_proto::VU_REFERENCE_DBFS);
    b.controls.insert(root, control);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "meter.vu_limiter",
        family: Family::Meter,
        title: "VU (FET limiter)",
        description: "The FET limiter's meter: a wide card in a deep black frame with the pivot hidden under a black cover strip, printed with the VU scale (-20…+3, a heavy red band over the plus side) above the arc and a 0…100 % scale under it, VU in both top corners. Variants: `cream` (black unit), `amber` (backlit, late silver unit), `ivory` (early unit, dots under the arc). Needle meter for a level or gain-reduction stream. Size = frame width.",
        variants: &["cream", "amber", "ivory", "open", "cream-open", "amber-open", "ivory-open"],
        default_size: 186.0,
        binds: "level stream",
        build: vu_limiter,
    }]
}
