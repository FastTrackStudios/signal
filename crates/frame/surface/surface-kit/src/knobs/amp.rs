//! Amp-head knobs of a modern boutique amp-sim, seen straight on:
//!
//! - `knob.amp_ticked` — a small black dome with a grey index flat at its
//!   crown, inside a printed ring of radial ticks (long and short).
//! - `knob.cup` — a white moulded cup: a thick rounded rim round a shallow
//!   dish, an orange index dot at the rim, a soft shadow to the lower right.
//! - `knob.slotted` — a flat black disc with a grooved slot running across
//!   it and its own legend printed along the slot (it turns with the knob),
//!   an orange index at the rim.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     printed ticks                                  (fixed, panel ink)
//!     shadow, the round body lit by the room          (fixed)
//!     body  ← Needle: index, slot, the knob's own legend
//!     light                                           (fixed)
//!   legend ← Label (above the knob; `knob.slotted` prints none outside)
//! ```
//!
//! Size = the knob body's diameter. Options: `ticks=N` (`amp_ticked`,
//! default 21); `index=#hex` the index colour (`cup`, `slotted`).

use document_proto::{NodeId, NodeKind, TextAlign, TextCase};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::PANEL_INK;
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

fn caption(
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    ink: &str,
    case: TextCase,
) -> document_proto::Node {
    let w = crate::controls::parts::text_w(s, size) * 1.25 + size * 2.0;
    let mut n = draw::text(
        "legend",
        s,
        cx - w / 2.0,
        cy - size * 0.68,
        w,
        size,
        ink,
        TextAlign::Center,
    );
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(700.0);
        t.text_case = case;
        t.letter_spacing = size * 0.08;
    }
    n
}

fn seg(a: (f64, f64), b: (f64, f64), w: f64) -> String {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / len * w / 2.0, dx / len * w / 2.0);
    format!(
        "{} ",
        draw::polygon(&[
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny)
        ])
    )
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

fn scaffold(
    b: &mut Builder<'_>,
    parent: NodeId,
    p: &Place,
    d: f64,
    ring: f64,
    fallback: &str,
    legend_outside: bool,
) -> Frame {
    let k = d / 40.0;
    let ink = p.ink_color(PANEL_INK);
    let sweep = p.sweep.unwrap_or(300.0);
    let half = ring.max(d / 2.0) + 3.0 * k;
    let side = 2.0 * half;
    let legend_size = (8.0 * k).max(7.0);
    let has = legend_outside && p.legend.as_deref().is_some_and(|l| !l.is_empty());
    let top = if has { legend_size * 1.9 } else { 0.0 };
    let name = if p.name.is_empty() {
        fallback
    } else {
        p.name.as_str()
    };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side + top));
    let knob = b.add(root, draw::frame("knob", 0.0, top, side, side));
    Frame {
        root,
        knob,
        c: half,
        side,
        top,
        k,
        ink,
        sweep,
    }
}

fn finish(
    b: &mut Builder<'_>,
    p: &Place,
    f: &Frame,
    body: NodeId,
    d: f64,
    legend: Option<NodeId>,
) -> NodeId {
    let legend = legend.unwrap_or_else(|| {
        let size = (8.0 * f.k).max(7.0);
        b.add(
            f.root,
            caption(
                p.legend.as_deref().unwrap_or(""),
                f.c,
                size * 0.85,
                size,
                &f.ink,
                p.case(),
            ),
        )
    });
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: f.sweep,
        })
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::Label, legend.0);
    b.controls.insert(f.root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, f.root, (f.c, f.c + f.top), d, touch);
    }
    f.root
}

fn amp_ticked(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(14.0);
    let r = d / 2.0;
    let ticks: usize = p
        .opt("ticks")
        .and_then(|v| v.parse().ok())
        .unwrap_or(21)
        .max(2);
    let k = d / 40.0;
    let ring_out = r + 11.0 * k;
    let f = scaffold(b, parent, p, d, ring_out, "Amp knob", true);
    let c = f.c;
    // Printed: radial ticks, every other one long.
    let dticks: String = (0..ticks)
        .map(|i| {
            let a = (i as f64 / (ticks - 1) as f64 - 0.5) * f.sweep;
            let long = i % 2 == 0;
            let r0 = r + if long { 3.0 } else { 4.5 } * k;
            let r1 = r + if long { 10.5 } else { 8.0 } * k;
            seg(
                draw::polar(c, c, r0, a),
                draw::polar(c, c, r1, a),
                (1.1 * k).max(0.7),
            )
        })
        .collect();
    if p.scale.as_ref().is_none_or(|s| !s.is_empty()) {
        b.add(
            f.knob,
            draw::path(
                "scale majors",
                0.0,
                0.0,
                f.side,
                f.side,
                dticks,
                draw::solid(&f.ink),
            ),
        );
    }
    // The knob: its shadow, the skirt, the dome.
    b.add(
        f.knob,
        draw::circle(
            "shadow",
            c + 1.5 * k,
            c + 3.5 * k,
            r * 1.02,
            draw::radial(&[(0.0, "#000000b0"), (0.75, "#00000066"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        f.knob,
        draw::stroked(
            draw::circle("skirt", c, c, r, draw::solid("#0c0c0d")),
            "#000000",
            0.6 * k,
        ),
    );
    b.add(
        f.knob,
        draw::circle(
            "dome",
            c,
            c,
            r * 0.9,
            draw::radial_at(
                0.42,
                0.36,
                0.75,
                &[(0.0, "#3b3c40"), (0.55, "#1d1e21"), (1.0, "#08080a")],
            ),
        ),
    );
    // Turning: the grey index flat at the crown.
    let body = b.add(f.knob, draw::frame("body", 0.0, 0.0, f.side, f.side));
    let flat = draw::sector(c, c, r * 0.45, r * 0.9, -26.0, 26.0);
    b.add(
        body,
        draw::path(
            "index",
            0.0,
            0.0,
            f.side,
            f.side,
            flat,
            draw::linear(180.0, &[(0.0, "#9a9ca1"), (1.0, "#56585d")]),
        ),
    );
    // The room's light.
    b.add(
        f.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            f.side,
            f.side,
            draw::sector(c, c, r * 0.88, r, -80.0, 20.0),
            draw::solid("#ffffff26"),
        ),
    );
    b.add(
        f.knob,
        draw::circle(
            "spec",
            c - r * 0.3,
            c - r * 0.35,
            r * 0.35,
            draw::radial(&[(0.0, "#ffffff30"), (1.0, "#ffffff00")]),
        ),
    );
    finish(b, p, &f, body, d, None)
}

fn cup(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(14.0);
    let r = d / 2.0;
    let index = p.opt("index").unwrap_or("#e0662a").to_string();
    let f = scaffold(b, parent, p, d, r + 4.0 * (d / 40.0), "Cup knob", true);
    let (c, k) = (f.c, f.k);
    b.add(
        f.knob,
        draw::circle(
            "shadow",
            c + 4.0 * k,
            c + 5.0 * k,
            r * 1.02,
            draw::radial(&[(0.0, "#00000055"), (0.7, "#00000026"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        f.knob,
        draw::stroked(
            draw::circle(
                "rim",
                c,
                c,
                r,
                draw::radial_at(
                    0.42,
                    0.34,
                    0.8,
                    &[(0.0, "#ffffff"), (0.7, "#eeeff1"), (1.0, "#c9cbd0")],
                ),
            ),
            "#b5b8be",
            0.6 * k,
        ),
    );
    // The dish: a shallow hollow, shaded on its upper-left wall.
    let dish = draw::circle(
        "dish",
        c,
        c,
        r * 0.62,
        draw::radial_at(
            0.56,
            0.62,
            0.75,
            &[(0.0, "#f7f7f8"), (0.7, "#eceef0"), (1.0, "#c3c6cc")],
        ),
    );
    b.add(
        f.knob,
        draw::with_effects(
            dish,
            vec![draw::inner_shadow("#0000002e", 1.0 * k, 1.6 * k, 2.6 * k)],
        ),
    );
    let body = b.add(f.knob, draw::frame("body", 0.0, 0.0, f.side, f.side));
    b.add(
        body,
        draw::rounded(
            draw::rect(
                "index",
                c - 0.9 * k,
                c - r * 0.96,
                1.8 * k,
                r * 0.22,
                draw::solid(&index),
            ),
            0.9 * k,
        ),
    );
    b.add(
        f.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            f.side,
            f.side,
            draw::sector(c, c, r * 0.64, r * 0.97, -110.0, 10.0),
            draw::solid("#ffffff66"),
        ),
    );
    finish(b, p, &f, body, d, None)
}

fn slotted(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(48.0).max(16.0);
    let r = d / 2.0;
    let index = p.opt("index").unwrap_or("#e8663a").to_string();
    let f = scaffold(b, parent, p, d, r + 2.0 * (d / 48.0), "Slotted knob", false);
    let (c, k) = (f.c, d / 48.0);
    b.add(
        f.knob,
        draw::circle(
            "shadow",
            c + 1.5 * k,
            c + 4.0 * k,
            r * 1.03,
            draw::radial(&[(0.0, "#000000aa"), (0.75, "#00000055"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        f.knob,
        draw::stroked(
            draw::circle(
                "disc",
                c,
                c,
                r,
                draw::radial_at(
                    0.45,
                    0.4,
                    0.8,
                    &[(0.0, "#2a2b2e"), (0.8, "#1c1d20"), (1.0, "#121315")],
                ),
            ),
            "#050506",
            1.2 * k,
        ),
    );
    // Turning: the slot across the top, the index, the knob's own legend.
    let body = b.add(f.knob, draw::frame("body", 0.0, 0.0, f.side, f.side));
    let sw = 3.2 * k;
    b.add(
        body,
        draw::rounded(
            draw::rect(
                "slot",
                c + 1.5 * k,
                c - r * 0.55,
                sw,
                r * 1.45,
                draw::solid("#0a0a0b"),
            ),
            sw / 2.0,
        ),
    );
    b.add(
        body,
        draw::rect(
            "slot bevel",
            c + 1.5 * k + sw,
            c - r * 0.5,
            0.7 * k,
            r * 1.35,
            draw::solid("#ffffff22"),
        ),
    );
    b.add(
        body,
        draw::rounded(
            draw::rect(
                "index",
                c + 1.8 * k,
                c - r * 0.9,
                2.6 * k,
                r * 0.2,
                draw::solid(&index),
            ),
            1.0 * k,
        ),
    );
    let size = (8.5 * k).max(7.0);
    let text = p.legend.as_deref().unwrap_or("");
    let len = r * 1.5;
    let mut t = draw::text(
        "legend",
        text,
        c - len / 2.0 - 5.0 * k,
        c - size * 0.62,
        len,
        size,
        &p.ink_color("#d9dadc"),
        TextAlign::Center,
    );
    if let NodeKind::Text(td) = &mut t.kind {
        td.font_weight = Some(500.0);
        td.letter_spacing = size * 0.02;
    }
    // Read bottom to top, along the slot's left.
    let legend = b.add(body, draw::rotated(t, -90.0));
    b.add(
        f.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            f.side,
            f.side,
            draw::sector(c, c, r * 0.9, r, -80.0, 10.0),
            draw::solid("#ffffff1c"),
        ),
    );
    finish(b, p, &f, body, d, Some(legend))
}

/// `knob.plugin_ring`: an amp-sim's own chrome knob — a flat disc in a
/// track ring, the value arc drawn over the track from the start of travel
/// (from noon when the param is bipolar), a line pointer, the value printed
/// under it and the legend under that (or above with `label_top`).
fn plugin_ring(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(44.0).max(16.0);
    let k = d / 44.0;
    let (disc, edge, track, arc, pointer, ink) = match p.variant.as_deref() {
        Some("dark") => (
            "#2b2c30", "#3e3f44", "#3a3b40", "#e8e9ec", "#e8e9ec", "#dfe0e3",
        ),
        Some("green") => (
            "#1c1d20", "#2c2d31", "#34363a", "#8fe39a", "#e8e9ec", "#e6e7ea",
        ),
        _ => (
            "#dadde8", "#b9bdd0", "#c6c9d4", "#121316", "#121316", "#23262c",
        ),
    };
    let arc = p.opt("arc").unwrap_or(arc).to_string();
    let ink = p.ink_color(ink);
    let sweep = p.sweep.unwrap_or(270.0);
    let size = (9.5 * k).max(8.0);
    let label_top = p.flag("label_top");
    let top = if label_top { size * 1.9 } else { 0.0 };
    let below = if label_top { size * 1.6 } else { size * 3.4 };
    let name = if p.name.is_empty() {
        "Ring knob"
    } else {
        p.name.as_str()
    };
    // A margin either side, where the value and legend overhang the ring.
    let m = d * 0.15;
    let root = b.add(
        parent,
        draw::frame(name, 0.0, 0.0, d + 2.0 * m, top + d + below),
    );
    let knob = b.add(root, draw::frame("knob", m, top, d, d));
    let (c, r) = (d / 2.0, d / 2.0);
    let ring = (r * 0.14).max(2.0);
    let half = sweep / 2.0;
    b.add(
        knob,
        draw::path(
            "track",
            0.0,
            0.0,
            d,
            d,
            draw::sector(c, c, r - ring, r, -half, half),
            draw::solid(track),
        ),
    );
    let mut arc_n = draw::circle("value arc", c, c, r, draw::solid(&arc));
    arc_n.name = "value arc".into();
    let arc_id = b.add(knob, arc_n);
    let cap = draw::stroked(
        draw::circle(
            "cap",
            c,
            c,
            r - ring - 2.2 * k,
            draw::radial_at(0.45, 0.4, 0.8, &[(0.0, disc), (1.0, disc)]),
        ),
        edge,
        0.8 * k,
    );
    b.add(knob, cap);
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, d, d));
    let pw = (1.8 * k).max(1.2);
    b.add(
        body,
        draw::rounded(
            draw::rect(
                "pointer",
                c - pw / 2.0,
                c - (r - ring - 4.0 * k),
                pw,
                r - ring - 4.0 * k,
                draw::solid(pointer),
            ),
            pw / 2.0,
        ),
    );
    let text = |nm: &str, s: &str, cy: f64, weight: f64| {
        let mut n = draw::text(
            nm,
            s,
            m - d,
            cy - size * 0.68,
            3.0 * d,
            size,
            &ink,
            TextAlign::Center,
        );
        if let NodeKind::Text(t) = &mut n.kind {
            t.font_weight = Some(weight);
            t.text_case = if nm == "legend" {
                p.case()
            } else {
                TextCase::Original
            };
            t.letter_spacing = size * 0.04;
        }
        n
    };
    // `legend_first`: the legend right under the knob, the value under it.
    let swap = p.flag("legend_first") && !label_top;
    let value_y = if swap {
        top + d + size * 2.55
    } else {
        top + d + size * 0.95
    };
    let value = b.add(root, text("value", "", value_y, 600.0));
    let legend_y = if label_top {
        size * 0.9
    } else if swap {
        top + d + size * 0.95
    } else {
        top + d + size * 2.55
    };
    let legend = b.add(
        root,
        text("legend", p.legend.as_deref().unwrap_or(""), legend_y, 700.0),
    );
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: sweep,
        })
        .slot(SlotRole::ValueArc, arc_id.0)
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::Value, value.0)
        .slot(SlotRole::Label, legend.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, root, (m + c, c + top), d, touch);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "knob.plugin_ring",
            family: Family::Knob,
            title: "Plugin ring knob",
            description: "An amp-sim's own chrome knob (header, cab mics, room): a flat disc in a track ring with the value arc drawn over it (from noon when bipolar), a line pointer, the value printed under it and the legend under that (`label_top` puts it above; `legend_first` swaps the two). `light` (pale disc, black arc), `dark`, `green` (a green arc on black); `arc=#hex` any arc colour. Size = ring diameter.",
            variants: &["light", "dark", "green"],
            default_size: 44.0,
            binds: "param",
            build: plugin_ring,
        },
        ComponentDef {
            id: "knob.amp_ticked",
            family: Family::Knob,
            title: "Amp knob (ticked)",
            description: "A modern amp head's small black dome knob with a grey index flat at its crown, inside a printed ring of long and short radial ticks (`ticks=N`, default 21); legend above. For black control strips. Size = knob diameter.",
            variants: &["black"],
            default_size: 40.0,
            binds: "param",
            build: amp_ticked,
        },
        ComponentDef {
            id: "knob.cup",
            family: Family::Knob,
            title: "Cup knob (white)",
            description: "A white moulded cup knob seen straight on: a thick rounded rim round a shallow dish, an orange index dot at the rim (`index=#hex`), a soft shadow to the lower right; legend above. For white or light machined amp panels. Size = knob diameter.",
            variants: &["white"],
            default_size: 40.0,
            binds: "param",
            build: cup,
        },
        ComponentDef {
            id: "knob.slotted",
            family: Family::Knob,
            title: "Slotted knob (legend on the knob)",
            description: "A flat black disc knob with a grooved slot across it and its legend printed on the knob along the slot, reading bottom to top (it turns with the knob), an orange index at the rim (`index=#hex`). For dark industrial amp panels. Size = disc diameter.",
            variants: &["black"],
            default_size: 48.0,
            binds: "param",
            build: slotted,
        },
    ]
}
