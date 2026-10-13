//! `knob.pedal_tall`: a tall knurled pedal knob seen from above and a
//! little in front, as a modern boutique pedal's product shot shows it —
//! the flat satin cap with a white dot, the knurled skirt round it, and the
//! knurled side of the knob showing below the cap. The pedal prints a ring
//! of fine dots round the travel.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     printed dots, shadow, the knurled side below   (fixed)
//!     body  ← Needle: the knurl's ribs, the cap, the dot
//!     light on the cap                               (fixed)
//!   legend ← Label (above, or `legend_below`)
//! ```
//!
//! Size = the knob's diameter. Options: `dots=N` (default 21, 0 = none),
//! `legend_below`. Variants colour the cap: `grey` (default), `black`,
//! `cream`.

use document_proto::{NodeId, NodeKind, TextAlign, TextCase};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::PANEL_INK;
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

fn caption(s: &str, cx: f64, cy: f64, size: f64, ink: &str, case: TextCase) -> document_proto::Node {
    let w = crate::controls::parts::text_w(s, size) * 1.25 + size * 2.0;
    let mut n = draw::text("legend", s, cx - w / 2.0, cy - size * 0.68, w, size, ink, TextAlign::Center);
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(700.0);
        t.text_case = case;
        t.letter_spacing = size * 0.04;
    }
    n
}

fn seg(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
    format!("{} ", draw::polygon(&[(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]))
}

fn dot(x: f64, y: f64, r: f64) -> String {
    let k = 0.5522847498 * r;
    format!(
        "M {:.3} {y:.3} C {:.3} {:.3} {:.3} {:.3} {x:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {y:.3} C {:.3} {:.3} {:.3} {:.3} {x:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {y:.3} Z ",
        x + r, x + r, y + k, x + k, y + r, y + r, x - k, y + r, x - r, y + k, x - r, x - r, y - k, x - k, y - r, y - r, x + k, y - r, x + r, y - k, x + r
    )
}

/// Cap colours: (edge, centre, highlight).
fn cap_paint(v: Option<&str>) -> [&'static str; 3] {
    match v {
        Some("black") => ["#1d1e21", "#34363a", "#55585d"],
        Some("cream") => ["#b9b2a0", "#e4dfd2", "#faf8f2"],
        _ => ["#4b4d51", "#707377", "#9a9da2"],
    }
}

#[allow(clippy::too_many_lines)]
fn pedal_tall(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(38.0).max(16.0);
    let r = d / 2.0;
    let k = d / 38.0;
    let ink = p.ink_color(PANEL_INK);
    let sweep = p.sweep.unwrap_or(300.0);
    let dots: usize = p.opt("dots").and_then(|v| v.parse().ok()).unwrap_or(21);
    // An empty authored scale prints nothing round the knob.
    let dots = if p.scale.as_ref().is_some_and(Vec::is_empty) { 0 } else { dots };
    let side_h = 0.26 * d;
    let ring = r + 5.5 * k;
    let half = ring + 3.0 * k + side_h * 0.5;
    let side = 2.0 * half;
    let c = half;
    let legend_size = (7.0 * k).max(7.0);
    let below = p.flag("legend_below");
    let has_legend = p.legend.as_deref().is_some_and(|l| !l.is_empty());
    let top = if has_legend && !below { legend_size * 1.8 } else { 0.0 };
    let extra = if has_legend && below { legend_size * 1.8 } else { 0.0 };
    let name = if p.name.is_empty() { "Tall knob" } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side + top + extra));
    let knob = b.add(root, draw::frame("knob", 0.0, top, side, side));
    let [edge, mid, hi] = cap_paint(p.variant.as_deref());

    // Fixed: the printed dots, the knob's shadow, its knurled side.
    if dots > 0 {
        let path: String = (0..dots)
            .map(|i| {
                let a = (i as f64 / (dots.max(2) - 1) as f64 - 0.5) * sweep;
                let (x, y) = draw::polar(c, c, ring, a);
                dot(x, y, (0.75 * k).max(0.55))
            })
            .collect();
        b.add(knob, draw::path("scale dots", 0.0, 0.0, side, side, path, draw::solid(&ink)));
    }
    b.add(knob, draw::circle("shadow", c + 1.0 * k, c + side_h + 2.0 * k, r * 1.02, draw::radial(&[(0.0, "#000000b8"), (0.7, "#00000070"), (1.0, "#00000000")])));
    // The side: a cylinder from the cap's rim down by `side_h`, its ribs
    // vertical (the camera looks down its knurl).
    // Outline: the cap's lower half, then the side's lower half `side_h` down.
    let mut pts: Vec<(f64, f64)> = (0..=24).map(|i| draw::polar(c, c, r, 270.0 - 180.0 * f64::from(i) / 24.0)).collect();
    pts.extend((0..=24).map(|i| {
        let (x, y) = draw::polar(c, c, r, 90.0 + 180.0 * f64::from(i) / 24.0);
        (x, y + side_h)
    }));
    let body_d = draw::polygon(&pts);
    b.add(knob, draw::path("side", 0.0, 0.0, side, side, body_d, draw::linear(90.0, &[(0.0, "#0c0c0e"), (0.3, "#2e2f33"), (0.55, "#1a1b1e"), (1.0, "#060607")])));
    let n_ribs = 22;
    let ribs: String = (1..n_ribs)
        .map(|i| {
            let t = f64::from(i) / f64::from(n_ribs);
            let x = c - r + 2.0 * r * t;
            let dy = (r * r - (x - c).powi(2)).max(0.0).sqrt();
            seg((x, c + dy * 0.9), (x, c + dy + side_h - 0.6 * k), 0.7 * k)
        })
        .collect();
    b.add(knob, draw::path("side knurl", 0.0, 0.0, side, side, ribs, draw::solid("#4a4b50")));

    // Turning: the knurled skirt's top, the cap, the dot.
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    b.add(body, draw::stroked(draw::circle("skirt", c, c, r, draw::solid("#161719")), "#000000", (0.6 * k).max(0.5)));
    let rc = r * 0.86;
    let top_ribs: String = (0..36)
        .map(|i| {
            let a = 10.0 * f64::from(i);
            seg(draw::polar(c, c, rc + 0.3 * k, a), draw::polar(c, c, r - 0.5 * k, a), 1.0 * k)
        })
        .collect();
    b.add(body, draw::path("knurl", 0.0, 0.0, side, side, top_ribs, draw::solid("#3a3b3f")));
    b.add(body, draw::circle("cap", c, c, rc, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, mid), (0.82, mid), (1.0, edge)])));
    b.add(body, draw::circle("index", c, c - rc * 0.74, (1.6 * k).max(1.0), draw::solid("#f7f7f5")));

    // Fixed: the light on the cap (brighter to the top), the rim's gleam.
    b.add(knob, draw::circle("cap light", c - rc * 0.1, c - rc * 0.35, rc * 0.85, draw::radial(&[(0.0, &format!("{hi}66")), (1.0, "#ffffff00")])));
    b.add(knob, draw::stroked(draw::circle("cap edge", c, c, rc, draw::solid("#00000000")), "#0000008c", (0.8 * k).max(0.5)));
    let gleam = draw::sector(c, c, rc, r - 0.4 * k, -70.0, 25.0);
    b.add(knob, draw::path("rim light", 0.0, 0.0, side, side, gleam, draw::solid("#ffffff22")));

    let cy = if below { top + side + legend_size * 0.9 } else { legend_size * 0.8 };
    let legend = b.add(root, caption(p.legend.as_deref().unwrap_or(""), c, cy, legend_size, &ink, p.case()));
    let ctl = p.control(ControlKind::Knob { sweep_degrees: sweep }).slot(SlotRole::Needle, body.0).slot(SlotRole::Label, legend.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, root, (c, c + top), d, touch);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.pedal_tall",
        family: Family::Knob,
        title: "Pedal knob (tall, knurled)",
        description: "A tall knurled pedal knob from above and a little in front: a flat satin cap (`grey` default, `black`, `cream`) with a white dot inside a knurled skirt, the knob's knurled side showing under the cap; the pedal prints a ring of fine dots round the travel (`dots=N`, default 21). Legend above (or `legend_below`). Size = knob diameter.",
        variants: &["grey", "black", "cream"],
        default_size: 38.0,
        binds: "param",
        build: pedal_tall,
    }]
}
