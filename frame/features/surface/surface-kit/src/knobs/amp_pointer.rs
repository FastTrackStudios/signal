//! Amp-head knobs from a modern boutique head's control strip, seen
//! straight on.
//!
//! - `knob.amp_pointer` — a round moulded boss with a pointer blade through
//!   it: a narrow tip up to the scale, a broad paddle below to turn it by
//!   (`black`, `cream`). The panel prints 0–10 round it (`numerals`) or a
//!   ring of ticks (`ticks=N`, majors every fifth).
//! - `knob.spun_cap` — a squat knob with a spun-metal face (`black`,
//!   `silver`) and a white index line from the rim, inside a printed ring
//!   of ticks (`ticks=N`, default 21).
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     printed numerals / ticks, shadow              (fixed)
//!     body  ← Needle: boss + blade / spun face + index
//!     light                                          (fixed)
//!   legend ← Label (above)
//! ```
//!
//! Size = the boss's diameter.

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
        t.letter_spacing = size * 0.06;
    }
    n
}

fn seg(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
    format!("{} ", draw::polygon(&[(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]))
}

struct Frame {
    root: NodeId,
    knob: NodeId,
    c: f64,
    side: f64,
    top: f64,
    k: f64,
    ink: String,
    sweep: f64,
}

/// Root and square `knob` frame, with the printed numerals or ticks.
fn scaffold(b: &mut Builder<'_>, parent: NodeId, p: &Place, d: f64, reach: f64, fallback: &str) -> Frame {
    let k = d / 40.0;
    let ink = p.ink_color(PANEL_INK);
    let sweep = p.sweep.unwrap_or(300.0);
    let numerals = p.flag("numerals");
    let ticks: usize = p.opt("ticks").and_then(|v| v.parse().ok()).unwrap_or(0);
    // An empty authored scale prints nothing round the knob.
    let (ticks, numerals) = if p.scale.as_ref().is_some_and(Vec::is_empty) { (0, false) } else { (ticks, numerals) };
    let r_print = reach + 4.0 * k;
    let num_size = (6.2 * k).max(6.0);
    let half = r_print + if numerals { num_size * 1.6 } else if ticks > 0 { 7.0 * k } else { 2.0 * k };
    let side = 2.0 * half;
    let legend_size = (8.0 * k).max(7.5);
    let has_legend = p.legend.as_deref().is_some_and(|l| !l.is_empty());
    let top = if has_legend { legend_size * 1.8 } else { 0.0 };
    let name = if p.name.is_empty() { fallback } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side + top));
    let knob = b.add(root, draw::frame("knob", 0.0, top, side, side));
    let c = half;
    if numerals {
        for i in 0..=10 {
            let a = (f64::from(i) / 10.0 - 0.5) * sweep;
            let s = i.to_string();
            b.add(knob, draw::radial_label("numeral", &s, (c, c), r_print + num_size * 0.55, a, num_size, &ink));
        }
    }
    if ticks > 1 {
        let path: String = (0..ticks)
            .map(|i| {
                let a = (i as f64 / (ticks - 1) as f64 - 0.5) * sweep;
                let major = i % 5 == 0;
                let len = if major { 5.5 * k } else { 3.2 * k };
                seg(draw::polar(c, c, r_print, a), draw::polar(c, c, r_print + len, a), if major { 1.3 * k } else { 0.9 * k })
            })
            .collect();
        b.add(knob, draw::path("scale majors", 0.0, 0.0, side, side, path, draw::solid(&ink)));
    }
    Frame { root, knob, c, side, top, k, ink, sweep }
}

fn finish(b: &mut Builder<'_>, p: &Place, f: &Frame, body: NodeId, d: f64) -> NodeId {
    let legend_size = (8.0 * f.k).max(7.5);
    let legend = b.add(f.root, caption(p.legend.as_deref().unwrap_or(""), f.c, legend_size * 0.8, legend_size, &f.ink, p.case()));
    let ctl = p.control(ControlKind::Knob { sweep_degrees: f.sweep }).slot(SlotRole::Needle, body.0).slot(SlotRole::Label, legend.0);
    b.controls.insert(f.root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, f.root, (f.c, f.c + f.top), d, touch);
    }
    f.root
}

fn amp_pointer(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(14.0);
    let r = d / 2.0;
    let f = scaffold(b, parent, p, d, r * 1.12, "Pointer knob");
    let (c, k, side) = (f.c, f.k, f.side);
    let cream = p.variant.as_deref() == Some("cream");
    let (edge, mid, blade_dark, blade_mid) = if cream { ("#b8b3a6", "#f1eee6", "#c9c4b6", "#f7f5ef") } else { ("#050506", "#232427", "#08080a", "#2c2d31") };

    b.add(f.knob, draw::circle("shadow", c + 1.0 * k, c + 3.0 * k, r * 1.05, draw::radial(&[(0.0, "#000000a0"), (0.7, "#00000055"), (1.0, "#00000000")])));
    // Turning: the boss and the blade (drawn at noon: tip up, paddle down).
    let body = b.add(f.knob, draw::frame("body", 0.0, 0.0, side, side));
    b.add(body, draw::stroked(draw::circle("boss", c, c, r, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, mid), (0.75, mid), (1.0, edge)])), "#00000080", (0.6 * k).max(0.5)));
    let blade = draw::polygon(&[
        (c - 0.9 * k, c - r * 1.12),
        (c + 0.9 * k, c - r * 1.12),
        (c + r * 0.2, c - r * 0.2),
        (c + r * 0.3, c + r * 0.5),
        (c + r * 0.44, c + r * 1.28),
        (c - r * 0.44, c + r * 1.28),
        (c - r * 0.3, c + r * 0.5),
        (c - r * 0.2, c - r * 0.2),
    ]);
    let blade_n = draw::path("blade", 0.0, 0.0, side, side, blade, draw::radial_at(0.5, 0.5, 0.75, &[(0.0, blade_mid), (1.0, blade_dark)]));
    b.add(body, draw::with_effects(draw::stroked(blade_n, if cream { "#9e998c" } else { "#000000" }, (0.6 * k).max(0.5)), vec![draw::drop_shadow("#00000099", 0.0, 0.0, 2.5 * k)]));
    // The blade's ridge catching light (turns with it — a thin centre line).
    b.add(body, draw::path("ridge", 0.0, 0.0, side, side, seg((c, c - r * 1.05), (c, c + r * 1.2), 0.9 * k), draw::solid(if cream { "#ffffffcc" } else { "#ffffff38" })));

    // Fixed light: a sheen on the boss from the top left.
    b.add(f.knob, draw::circle("light", c - r * 0.3, c - r * 0.35, r * 0.6, draw::radial(&[(0.0, if cream { "#ffffff70" } else { "#ffffff26" }), (1.0, "#ffffff00")])));
    finish(b, p, &f, body, d)
}

fn spun_cap(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(14.0);
    let r = d / 2.0;
    let mut p2 = p.clone();
    if p2.opt("ticks").is_none() {
        let mut o = p2.options.clone().unwrap_or_default();
        o.push("ticks=21".into());
        p2.options = Some(o);
    }
    let f = scaffold(b, parent, &p2, d, r * 1.02, "Spun cap");
    let (c, k, side) = (f.c, f.k, f.side);
    let silver = p.variant.as_deref() == Some("silver");
    b.add(f.knob, draw::circle("shadow", c + 1.0 * k, c + 3.5 * k, r * 1.05, draw::radial(&[(0.0, "#000000b0"), (0.7, "#00000060"), (1.0, "#00000000")])));
    b.add(f.knob, draw::stroked(draw::circle("rim", c, c, r, draw::linear(180.0, &[(0.0, "#3a3b3f"), (1.0, "#050506")])), "#000000", (0.7 * k).max(0.5)));
    // Fixed: the spun face (its rings are round, so they needn't turn).
    let spun: &[(f64, &str)] = if silver {
        &[(0.0, "#f4f4f2"), (0.12, "#9c9d9b"), (0.25, "#e2e2e0"), (0.38, "#7e7f7d"), (0.5, "#f0f0ee"), (0.62, "#8d8e8c"), (0.75, "#dcdcda"), (0.88, "#7a7b79"), (1.0, "#f4f4f2")]
    } else {
        &[(0.0, "#5a5c60"), (0.12, "#1a1b1e"), (0.25, "#44464a"), (0.38, "#121315"), (0.5, "#505256"), (0.62, "#18191b"), (0.75, "#3e4044"), (0.88, "#111214"), (1.0, "#5a5c60")]
    };
    b.add(f.knob, draw::circle("face", c, c, r * 0.9, draw::conic(20.0, spun)));
    let body = b.add(f.knob, draw::frame("body", 0.0, 0.0, side, side));
    b.add(body, draw::path("index", 0.0, 0.0, side, side, seg((c, c - r * 0.92), (c, c - r * 0.42), 1.8 * k), draw::solid(if silver { "#141416" } else { "#f2f2f0" })));
    b.add(f.knob, draw::circle("light", c - r * 0.2, c - r * 0.3, r * 0.7, draw::radial(&[(0.0, "#ffffff1e"), (1.0, "#ffffff00")])));
    finish(b, p, &f, body, d)
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "knob.amp_pointer",
            family: Family::Knob,
            title: "Amp pointer knob",
            description: "A boutique amp head's knob, straight on: a round moulded boss with a pointer blade through it — narrow tip up to the scale, broad paddle below to turn it by (`black` on a cream strip, `cream` on a dark one). `numerals` prints 0–10 round it, `ticks=N` a tick ring (majors every fifth). Legend above. Size = boss diameter.",
            variants: &["black", "cream"],
            default_size: 40.0,
            binds: "param",
            build: amp_pointer,
        },
        ComponentDef {
            id: "knob.spun_cap",
            family: Family::Knob,
            title: "Spun-cap knob",
            description: "A squat amp knob with a spun-metal face (`black` anodised or `silver`) and a white index line in from the rim, inside a printed ring of ticks (`ticks=N`, default 21; majors every fifth). Legend above. Size = knob diameter.",
            variants: &["black", "silver"],
            default_size: 40.0,
            binds: "param",
            build: spun_cap,
        },
    ]
}
