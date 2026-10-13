//! `knob.gold_knurl` and `knob.amp_cap`.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     printed ticks (amp_cap), shadow, skirt/rim, cap   (fixed, the room's light)
//!     body  ← Needle: the index dot
//!     glint                                             (fixed)
//!   legend ← Label
//! ```

use document_proto::{NodeId, NodeKind, TextAlign};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use super::{Finish, finish};
use crate::controls::parts::{PANEL_INK, alpha, cased, text_w};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// An ellipse as a path.
pub(crate) fn ellipse_d(cx: f64, cy: f64, rx: f64, ry: f64) -> String {
    format!(
        "M {:.3} {cy:.3} A {rx:.3} {ry:.3} 0 1 1 {:.3} {cy:.3} A {rx:.3} {ry:.3} 0 1 1 {:.3} {cy:.3} Z",
        cx + rx,
        cx - rx,
        cx + rx
    )
}

/// Radial ridges round the band `r0..r1` about (`c`, `c`): a knurled rim,
/// the same at any turn.
fn ridges(c: f64, r0: f64, r1: f64, n: u32, w: f64) -> String {
    let mut d = String::new();
    for i in 0..n {
        let a = 360.0 * f64::from(i) / f64::from(n);
        let (x0, y0) = draw::polar(c, c, r0, a);
        let (x1, y1) = draw::polar(c, c, r1, a);
        let (nx, ny) = (
            a.to_radians().cos() * w / 2.0,
            a.to_radians().sin() * w / 2.0,
        );
        d.push_str(&draw::polygon(&[
            (x0 - nx, y0 - ny),
            (x0 + nx, y0 + ny),
            (x1 + nx, y1 + ny),
            (x1 - nx, y1 - ny),
        ]));
        d.push(' ');
    }
    d
}

fn legend(b: &mut Builder<'_>, root: NodeId, p: &Place, cx: f64, cy: f64, size: f64) -> NodeId {
    let s = p.legend.as_deref().unwrap_or("");
    let w = text_w(s, size) * 1.2 + size * 2.0;
    let mut n = draw::text(
        "legend",
        s,
        cx - w / 2.0,
        cy - size * 0.68,
        w,
        size,
        &p.ink_color(PANEL_INK),
        TextAlign::Center,
    );
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(600.0);
    }
    b.add(root, cased(n, p.case()))
}

/// The index dot, on a square frame over the knob that turns with it.
fn index(
    b: &mut Builder<'_>,
    knob: NodeId,
    side: f64,
    c: f64,
    at: f64,
    dot: f64,
    color: &str,
) -> NodeId {
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let (x, y) = draw::polar(c, c, at, 0.0);
    let d = draw::circle("index", x, y, dot, draw::solid(color));
    b.add(
        body,
        draw::with_effects(
            d,
            vec![
                draw::inner_shadow("#00000080", 0.0, dot * 0.3, dot * 0.5),
                draw::drop_shadow("#ffffff40", 0.0, dot * 0.25, 0.0),
            ],
        ),
    );
    body
}

fn finish_knob(
    b: &mut Builder<'_>,
    root: NodeId,
    p: &Place,
    body: NodeId,
    label: NodeId,
    c: f64,
    d: f64,
) -> NodeId {
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: p.sweep.unwrap_or(300.0),
        })
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, root, (c, c), d, touch);
    }
    root
}

/// A machined knob seen a little from the front: cap on top, the knurled
/// skirt showing below it.
fn gold_knurl(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(34.0).max(14.0);
    let r = d / 2.0;
    let k = d / 34.0;
    let f: Finish = finish(p.variant.as_deref());
    let skirt = d * 0.5;
    let ry = r * 0.34;
    let half = (skirt + ry + 3.0 * k).max(r + 3.0 * k);
    let side = 2.0 * half;
    let c = half;
    let lsize = (8.0 * k).max(7.0);
    let name = if p.name.is_empty() {
        "Knurled knob"
    } else {
        p.name.as_str()
    };
    let root = b.add(
        parent,
        draw::frame(name, 0.0, 0.0, side, side + lsize * 1.6),
    );
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));

    // Its shadow on the enclosure, thrown down and forward.
    let shadow = draw::path(
        "shadow",
        0.0,
        0.0,
        side,
        side,
        ellipse_d(c, c + skirt + ry * 0.3, r * 1.05, ry * 1.25),
        draw::solid("#0000008c"),
    );
    b.add(
        knob,
        draw::with_effects(
            shadow,
            vec![draw::drop_shadow("#00000066", 0.0, 2.0 * k, 4.0 * k)],
        ),
    );
    // The skirt: a cylinder lit from the upper left, its bottom edge an ellipse.
    let sk = format!(
        "M {:.3} {c:.3} L {:.3} {:.3} A {r:.3} {ry:.3} 0 0 0 {:.3} {:.3} L {:.3} {c:.3} Z",
        c - r,
        c - r,
        c + skirt,
        c + r,
        c + skirt,
        c + r
    );
    let body_paint = draw::linear(
        90.0,
        &[
            (0.0, f.lo),
            (0.18, f.mid),
            (0.36, f.hi),
            (0.55, f.mid),
            (0.85, f.lo),
            (1.0, f.lo),
        ],
    );
    b.add(
        knob,
        draw::path("skirt", 0.0, 0.0, side, side, sk.clone(), body_paint),
    );
    // The knurl: fine vertical ridges, foreshortened round the cylinder.
    let n = (d / (1.5 * k)).clamp(14.0, 40.0) as u32;
    let (mut dark, mut lit) = (String::new(), String::new());
    for i in 0..n {
        let th = (-90.0 + 180.0 * (f64::from(i) + 0.5) / f64::from(n)).to_radians();
        let x = c + r * 0.985 * th.sin();
        let w = (0.9 * k * th.cos()).max(0.15);
        let u = ((x - c) / r).clamp(-1.0, 1.0);
        let bottom = c + skirt + ry * (1.0 - u * u).sqrt() - 0.6 * k;
        let top = c + r * 0.25;
        dark.push_str(&draw::polygon(&[
            (x - w / 2.0, top),
            (x + w / 2.0, top),
            (x + w / 2.0, bottom),
            (x - w / 2.0, bottom),
        ]));
        dark.push(' ');
        let lx = x + w * 0.9;
        lit.push_str(&draw::polygon(&[
            (lx - w / 3.0, top),
            (lx + w / 3.0, top),
            (lx + w / 3.0, bottom),
            (lx - w / 3.0, bottom),
        ]));
        lit.push(' ');
    }
    b.add(
        knob,
        draw::path(
            "knurl",
            0.0,
            0.0,
            side,
            side,
            dark,
            draw::solid(&alpha(f.lo, 0.75)),
        ),
    );
    b.add(
        knob,
        draw::path(
            "knurl light",
            0.0,
            0.0,
            side,
            side,
            lit,
            draw::solid("#ffffff38"),
        ),
    );
    // The skirt's bottom lip catching the room's light.
    let lip = format!(
        "M {:.3} {:.3} A {r:.3} {ry:.3} 0 0 0 {:.3} {:.3}",
        c - r * 0.96,
        c + skirt + ry * 0.2,
        c + r * 0.96,
        c + skirt + ry * 0.2
    );
    let mut lip = draw::path(
        "skirt lip",
        0.0,
        0.0,
        side,
        side,
        lip,
        draw::solid("#00000000"),
    );
    lip = draw::stroked(lip, &alpha(f.hi, 0.5), 0.8 * k);
    b.add(knob, lip);

    // The cap: a shallow dome, lit from the upper left.
    let cap = draw::circle(
        "cap",
        c,
        c,
        r,
        draw::radial_at(
            0.45,
            0.35,
            0.72,
            &[
                (0.0, f.hi),
                (0.55, f.mid),
                (0.86, f.lo),
                (0.95, f.mid),
                (1.0, f.hi),
            ],
        ),
    );
    let cap = draw::stroked(cap, &alpha(f.lo, 0.9), 0.7 * k);
    b.add(
        knob,
        draw::with_effects(
            cap,
            vec![
                draw::inner_shadow("#ffffff66", 0.0, 1.0 * k, 1.2 * k),
                draw::drop_shadow("#00000059", 0.0, 1.0 * k, 1.5 * k),
            ],
        ),
    );
    let body = index(b, knob, side, c, r * 0.62, r * 0.12, f.mark);
    let glint = draw::circle(
        "glint",
        c - r * 0.3,
        c - r * 0.38,
        r * 0.42,
        draw::radial(&[(0.0, "#ffffff70"), (1.0, "#ffffff00")]),
    );
    b.add(knob, glint);
    let label = legend(b, root, p, c, side + lsize * 0.7, lsize);
    finish_knob(b, root, p, body, label, c, d)
}

/// A head's knob seen straight on: a dark grooved rim, a metallic cap, a
/// dot, and (`ticks=N`) a printed tick ring on the faceplate.
fn amp_cap(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(14.0);
    let r = d / 2.0;
    let k = d / 40.0;
    let variant = p.variant.as_deref().unwrap_or("pink");
    let ink = p.ink_color("#3b3b3b");
    let ticks: usize = p.opt("ticks").and_then(|v| v.parse().ok()).unwrap_or(0);
    let sweep = p.sweep.unwrap_or(300.0);
    let tick_out = r + 11.0 * k;
    let half = if ticks > 0 {
        tick_out + 2.0 * k
    } else {
        r + 8.0 * k
    };
    let side = 2.0 * half;
    let c = half;
    let lsize = (8.0 * k).max(7.0);
    let name = if p.name.is_empty() {
        "Amp knob"
    } else {
        p.name.as_str()
    };
    let root = b.add(
        parent,
        draw::frame(name, 0.0, 0.0, side, side + lsize * 1.6),
    );
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));

    if ticks > 1 {
        let mut dd = String::new();
        for i in 0..ticks {
            let t = i as f64 / (ticks - 1) as f64;
            let a = (t - 0.5) * sweep;
            let major = i % 2 == 0;
            let r0 = r + 4.0 * k;
            let r1 = if major { tick_out } else { r + 8.0 * k };
            let (x0, y0) = draw::polar(c, c, r0, a);
            let (x1, y1) = draw::polar(c, c, r1, a);
            let w = 0.45 * k.max(0.8);
            let (nx, ny) = (a.to_radians().cos() * w, a.to_radians().sin() * w);
            dd.push_str(&draw::polygon(&[
                (x0 - nx, y0 - ny),
                (x0 + nx, y0 + ny),
                (x1 + nx, y1 + ny),
                (x1 - nx, y1 - ny),
            ]));
            dd.push(' ');
        }
        b.add(
            knob,
            draw::path("ticks", 0.0, 0.0, side, side, dd, draw::solid(&ink)),
        );
    }

    // (rim top, rim bottom, cap finish, dot, cap radius, ridge colour)
    let (rim_hi, rim_lo, capf, dot, cap_r, ridge) = match variant {
        "white" => (
            "#f4f4f2",
            "#b9b9b6",
            finish(Some("white")),
            "#4a4a4a",
            0.8,
            "#00000014",
        ),
        "black" => (
            "#3a3a3e",
            "#050506",
            finish(Some("black")),
            "#f0f0f0",
            0.72,
            "#ffffff22",
        ),
        "eq_pink" => (
            "#e7a7bb",
            "#b56b84",
            finish(Some("pink")),
            "#2a0f19",
            0.9,
            "#8d4a6099",
        ),
        _ => (
            "#3b2a31",
            "#0b0708",
            finish(Some("pink")),
            "#1e0c12",
            0.7,
            "#ffffff1c",
        ),
    };
    let sh = draw::circle(
        "shadow",
        c + 1.0 * k,
        c + 3.5 * k,
        r,
        draw::solid("#00000070"),
    );
    b.add(
        knob,
        draw::with_effects(
            sh,
            vec![draw::drop_shadow("#00000059", 0.0, 2.0 * k, 5.0 * k)],
        ),
    );
    let rim = draw::circle(
        "rim",
        c,
        c,
        r,
        draw::linear(165.0, &[(0.0, rim_hi), (1.0, rim_lo)]),
    );
    b.add(knob, draw::stroked(rim, "#00000055", 0.6 * k));
    let n = (std::f64::consts::PI * d / (2.2 * k)).clamp(30.0, 110.0) as u32;
    let band = if variant == "eq_pink" {
        (r * 0.93, r - 0.4 * k)
    } else {
        (r * cap_r + 0.5 * k, r - 0.5 * k)
    };
    b.add(
        knob,
        draw::path(
            "grooves",
            0.0,
            0.0,
            side,
            side,
            ridges(c, band.0, band.1, n, 0.8 * k),
            draw::solid(ridge),
        ),
    );
    let rc = r * cap_r;
    let cap = draw::circle(
        "cap",
        c,
        c,
        rc,
        draw::radial_at(
            0.4,
            0.32,
            0.8,
            &[(0.0, capf.hi), (0.55, capf.mid), (1.0, capf.lo)],
        ),
    );
    b.add(
        knob,
        draw::with_effects(
            draw::stroked(cap, "#00000040", 0.5 * k),
            vec![draw::inner_shadow("#00000040", 0.0, -k, 2.0 * k)],
        ),
    );
    if variant == "pink" || variant == "black" {
        let sheen = draw::conic(
            200.0,
            &[
                (0.0, "#ffffff55"),
                (0.13, "#ffffff00"),
                (0.25, "#00000026"),
                (0.38, "#ffffff00"),
                (0.5, "#ffffff4a"),
                (0.63, "#ffffff00"),
                (0.75, "#00000024"),
                (0.88, "#ffffff00"),
                (1.0, "#ffffff55"),
            ],
        );
        b.add(knob, draw::circle("sheen", c, c, rc - 0.3 * k, sheen));
    }
    let at = if variant == "eq_pink" {
        r * 0.62
    } else {
        rc * 0.66
    };
    let body = index(b, knob, side, c, at, (r * 0.09).max(1.2), dot);
    b.add(
        knob,
        draw::circle(
            "glint",
            c - rc * 0.3,
            c - rc * 0.36,
            rc * 0.45,
            draw::radial(&[(0.0, "#ffffff55"), (1.0, "#ffffff00")]),
        ),
    );
    let label = legend(b, root, p, c, side + lsize * 0.7, lsize);
    finish_knob(b, root, p, body, label, c, d)
}

/// A software-style knob: a flat dark disc with a white index line inside
/// a track ring whose white value arc grows from the start of travel.
fn arc_disc(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(14.0);
    let r = d / 2.0;
    let k = d / 40.0;
    let ring = r + 6.0 * k;
    let side = 2.0 * (ring + 1.0);
    let c = side / 2.0;
    let sweep = p.sweep.unwrap_or(270.0);
    let ink = p.ink_color("#f2f2f2");
    let lsize = (8.0 * k).max(7.0);
    let name = if p.name.is_empty() {
        "Arc knob"
    } else {
        p.name.as_str()
    };
    let root = b.add(
        parent,
        draw::frame(name, 0.0, 0.0, side, side + lsize * 1.6),
    );
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));
    let thick = 3.2 * k;
    let track = draw::sector(c, c, ring - thick, ring, -sweep / 2.0, sweep / 2.0);
    b.add(
        knob,
        draw::path(
            "track",
            0.0,
            0.0,
            side,
            side,
            track,
            draw::solid("#ffffff1f"),
        ),
    );
    let mut arc = document_proto::Node::new(NodeKind::Vector, "value arc");
    arc.size = document_proto::Size {
        width: 2.0 * ring,
        height: 2.0 * ring,
    };
    arc.transform = document_proto::Transform::translate(c - ring, c - ring);
    arc.fills = vec![draw::solid(&ink)];
    let arc = b.add(knob, arc);
    let disc = draw::circle(
        "disc",
        c,
        c,
        r,
        draw::linear(165.0, &[(0.0, "#3a3a3d"), (1.0, "#1c1c1e")]),
    );
    b.add(
        knob,
        draw::with_effects(
            draw::stroked(disc, "#00000080", 0.6 * k),
            vec![
                draw::drop_shadow("#00000080", 0.0, 2.0 * k, 4.0 * k),
                draw::inner_shadow("#ffffff1a", 0.0, 1.0 * k, 0.8 * k),
            ],
        ),
    );
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let lw = (2.0 * k).max(1.2);
    b.add(
        body,
        draw::rounded(
            draw::rect(
                "index",
                c - lw / 2.0,
                c - r * 0.9,
                lw,
                r * 0.55,
                draw::solid(&ink),
            ),
            lw / 2.0,
        ),
    );
    let label = legend(b, root, p, c, side + lsize * 0.7, lsize);
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: sweep,
        })
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::ValueArc, arc.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, root, (c, c), d, touch);
    }
    root
}

pub(super) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "knob.arc_disc",
            family: Family::Knob,
            title: "Arc disc (software knob)",
            description: "A software-style knob from a modern amp-sim's chrome and screens: a flat dark disc with a white index line, inside a faint track ring whose white value arc grows from the start of travel (270° by default). Size = disc diameter.",
            variants: &["dark"],
            default_size: 40.0,
            binds: "param",
            build: arc_disc,
        },
        ComponentDef {
            id: "knob.gold_knurl",
            family: Family::Knob,
            title: "Knurled metal knob (boutique pedal)",
            description: "A machined boutique-pedal knob seen a little from the front: a smooth domed cap with an index dot over a finely knurled cylindrical skirt that shows below it. Satin gold (`gold`), rose gold, black, pearl, magenta, chrome, pink. No printed scale — the pedal prints the legend. Size = cap diameter.",
            variants: &[
                "gold", "rose", "black", "pearl", "magenta", "chrome", "pink",
            ],
            default_size: 34.0,
            binds: "param",
            build: gold_knurl,
        },
        ComponentDef {
            id: "knob.amp_cap",
            family: Family::Knob,
            title: "Capped amp knob",
            description: "A boutique head's knob seen straight on: a grooved rim, a metallic cap with an index dot, and (`options` `ticks=N`) a ring of long/short printed ticks on the faceplate. `pink` (black rim, pink metal cap), `white` (all white, matte), `black` (black knurled), `eq_pink` (a big pink knurled tabletop knob, dot off-centre). Size = rim diameter.",
            variants: &["pink", "white", "black", "eq_pink"],
            default_size: 40.0,
            binds: "param",
            build: amp_cap,
        },
    ]
}
