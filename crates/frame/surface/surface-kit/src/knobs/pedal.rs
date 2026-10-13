//! Stompbox knobs, seen from straight above as a modern boutique pedal
//! shows them in an amp-sim suite.
//!
//! - `knob.pedal_lobed` — the glossy black "jedi" knob: seven soft lobes
//!   round a domed top, one white line from the centre to the edge.
//! - `knob.pedal_grip` — a tall knurled grip with a flat satin cap and a
//!   white dot index; the pedal prints a ring of fine dots round it.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     printed dots                                   (fixed, panel ink)
//!     shadow, round tiers lit by the room             (fixed)
//!     body  ← Needle: lobes / knurl, the index
//!     light: the dome's gloss                          (fixed)
//!   legend ← Label (above the knob)
//! ```
//!
//! Size = the knob's outer diameter. Options: `dots=N` prints N dots round
//! the travel (0 = none; `knob.pedal_grip` defaults to 21, `pedal_lobed`
//! to none); `legend_below` puts the legend under the knob.

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
        t.letter_spacing = size * 0.04;
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

fn dot_ring(cx: f64, cy: f64, r: f64, n: usize, sweep: f64, dot: f64) -> String {
    (0..n)
        .map(|i| {
            let a = (i as f64 / (n.max(2) - 1) as f64 - 0.5) * sweep;
            let (x, y) = draw::polar(cx, cy, r, a);
            let k = 0.5522847498 * dot;
            format!(
                "M {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} Z ",
                x + dot, y,
                x + dot, y + k, x + k, y + dot, x, y + dot,
                x - k, y + dot, x - dot, y + k, x - dot, y,
                x - dot, y - k, x - k, y - dot, x, y - dot,
                x + k, y - dot, x + dot, y - k, x + dot, y
            )
        })
        .collect()
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

/// The shared scaffold: root and a square `knob` frame with room for the
/// printed dots and the legend above (or below).
fn scaffold(
    b: &mut Builder<'_>,
    parent: NodeId,
    p: &Place,
    d: f64,
    dots: usize,
    fallback: &str,
) -> Frame {
    let r = d / 2.0;
    let k = d / 48.0;
    let ink = p.ink_color(PANEL_INK);
    let sweep = p.sweep.unwrap_or(300.0);
    let printed = p.scale.as_ref().is_none_or(|s| !s.is_empty());
    let dots = if printed { dots } else { 0 };
    let ring = if dots > 0 { r + 6.0 * k } else { r };
    let half = ring + if dots > 0 { 4.0 } else { 7.0 } * k;
    let side = 2.0 * half;
    let legend_size = (9.0 * k).max(8.0);
    let below = p.flag("legend_below");
    let has_legend = p.legend.as_deref().is_some_and(|l| !l.is_empty());
    let top = if has_legend && !below {
        legend_size * 1.8
    } else {
        0.0
    };
    let extra = if has_legend && below {
        legend_size * 1.8
    } else {
        0.0
    };
    let name = if p.name.is_empty() {
        fallback
    } else {
        p.name.as_str()
    };
    let root = b.add(
        parent,
        draw::frame(name, 0.0, 0.0, side, side + top + extra),
    );
    let knob = b.add(root, draw::frame("knob", 0.0, top, side, side));
    let c = half;
    if dots > 0 {
        let d = dot_ring(c, c, ring, dots, sweep, (0.9 * k).max(0.6));
        b.add(
            knob,
            draw::path("scale dots", 0.0, 0.0, side, side, d, draw::solid(&ink)),
        );
    }
    Frame {
        root,
        knob,
        c,
        side,
        top,
        k,
        ink,
        sweep,
    }
}

fn finish(b: &mut Builder<'_>, p: &Place, f: &Frame, body: NodeId, d: f64) -> NodeId {
    let legend_size = (9.0 * f.k).max(8.0);
    let below = p.flag("legend_below");
    let cy = if below {
        f.top + f.side + legend_size * 0.9
    } else {
        legend_size * 0.8
    };
    let legend = b.add(
        f.root,
        caption(
            p.legend.as_deref().unwrap_or(""),
            f.c,
            cy,
            legend_size,
            &f.ink,
            p.case(),
        ),
    );
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

/// Colours of the moulding: (lobe rim, lobe body, top edge, top centre).
fn lobed_paint(v: Option<&str>) -> [&'static str; 4] {
    match v {
        Some("cream") => ["#8f8672", "#d9d0b8", "#c9bfa4", "#f2ecdc"],
        Some("grey") => ["#1e1f22", "#4a4c51", "#3a3c40", "#6b6e74"],
        Some("white") => ["#9a9ea6", "#e6e8ec", "#d4d7dd", "#fbfbfc"],
        _ => ["#000000", "#1a1a1c", "#141416", "#2e2f33"],
    }
}

fn pedal_lobed(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(48.0).max(16.0);
    let dots: usize = p.opt("dots").and_then(|v| v.parse().ok()).unwrap_or(0);
    let f = scaffold(b, parent, p, d, dots, "Pedal knob");
    let (c, k, side) = (f.c, f.k, f.side);
    let r = d / 2.0;
    let [rim, body_c, top_edge, top_mid] = lobed_paint(p.variant.as_deref());
    let light = p
        .variant
        .as_deref()
        .is_some_and(|v| v == "cream" || v == "white");

    // Fixed: the shadow it throws on the enclosure.
    b.add(
        f.knob,
        draw::circle(
            "shadow",
            c + 1.2 * k,
            c + 3.2 * k,
            r * 0.98,
            draw::radial(&[(0.0, "#000000b0"), (0.75, "#00000070"), (1.0, "#00000000")]),
        ),
    );

    // Fixed: the moulding's side, seen past the lobes' lower edge.
    b.add(
        f.knob,
        draw::circle(
            "side",
            c,
            c + 2.2 * k,
            r * 0.94,
            draw::solid(if light { "#6f6a5c" } else { "#000000" }),
        ),
    );
    // Turning: the lobed skirt and the domed top with the index.
    let body = b.add(f.knob, draw::frame("body", 0.0, 0.0, side, side));
    let lobes = draw::lobed(c, c, r, 7, r * 0.13);
    let skirt = draw::path(
        "lobes",
        0.0,
        0.0,
        side,
        side,
        lobes,
        draw::radial_at(0.5, 0.5, 0.5, &[(0.0, body_c), (0.78, body_c), (1.0, rim)]),
    );
    b.add(
        body,
        draw::with_effects(
            draw::stroked(skirt, "#000000", (0.6 * k).max(0.5)),
            vec![draw::drop_shadow("#00000099", 0.0, 0.0, 2.0 * k)],
        ),
    );
    let rt = r * 0.66;
    b.add(
        body,
        draw::circle(
            "top",
            c,
            c,
            rt,
            draw::radial_at(
                0.5,
                0.5,
                0.5,
                &[(0.0, top_mid), (0.85, top_mid), (1.0, top_edge)],
            ),
        ),
    );
    let lw = (2.4 * k).max(1.3);
    let line_c = if light { "#1b1b1d" } else { "#f4f4f2" };
    b.add(
        body,
        draw::rounded(
            draw::rect(
                "index",
                c - lw / 2.0,
                c - r * 0.9,
                lw,
                r * 0.72,
                draw::solid(line_c),
            ),
            lw / 2.0,
        ),
    );

    // Fixed: the room's light on the lobes and the gloss on the dome — a
    // bright crescent on the upper-left lobes, a softer one on the right,
    // the dome's specular spot.
    let rim_light = draw::sector(c, c, r * 0.7, r * 0.93, -95.0, 5.0);
    b.add(
        f.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            side,
            side,
            rim_light,
            draw::radial_at(
                0.3,
                0.2,
                0.7,
                &[
                    (0.0, if light { "#ffffff88" } else { "#ffffff66" }),
                    (1.0, "#ffffff00"),
                ],
            ),
        ),
    );
    let rim_right = draw::sector(c, c, r * 0.72, r * 0.9, 20.0, 80.0);
    b.add(
        f.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            side,
            side,
            rim_right,
            draw::solid(if light { "#ffffff33" } else { "#ffffff17" }),
        ),
    );
    b.add(
        f.knob,
        draw::circle(
            "gloss",
            c - rt * 0.2,
            c - rt * 0.3,
            rt * 0.75,
            draw::radial(&[
                (0.0, if light { "#ffffff66" } else { "#ffffff30" }),
                (1.0, "#ffffff00"),
            ]),
        ),
    );
    b.add(
        f.knob,
        draw::circle(
            "spec",
            c - rt * 0.34,
            c - rt * 0.45,
            rt * 0.2,
            draw::radial(&[(0.0, "#ffffffa0"), (1.0, "#ffffff00")]),
        ),
    );
    b.add(
        f.knob,
        draw::stroked(
            draw::circle("top edge", c, c, rt, draw::solid("#00000000")),
            "#00000066",
            (0.9 * k).max(0.5),
        ),
    );
    finish(b, p, &f, body, d)
}

fn grip_paint(v: Option<&str>) -> [&'static str; 3] {
    // (knurl, cap edge, cap centre)
    match v {
        Some("black") => ["#141416", "#26272a", "#3a3b3f"],
        Some("silver") => ["#2a2b2e", "#9fa2a6", "#dfe1e3"],
        _ => ["#1b1c1e", "#44464a", "#6a6c70"],
    }
}

fn pedal_grip(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(48.0).max(16.0);
    let dots: usize = p.opt("dots").and_then(|v| v.parse().ok()).unwrap_or(21);
    let f = scaffold(b, parent, p, d, dots, "Grip knob");
    let (c, k, side) = (f.c, f.k, f.side);
    let r = d / 2.0;
    let [knurl, cap_edge, cap_mid] = grip_paint(p.variant.as_deref());

    b.add(
        f.knob,
        draw::circle(
            "shadow",
            c + 1.5 * k,
            c + 4.0 * k,
            r,
            draw::radial(&[(0.0, "#000000a8"), (0.7, "#00000066"), (1.0, "#00000000")]),
        ),
    );
    // Fixed: the grip's side, a dark ring (the camera sees it past the cap).
    b.add(
        f.knob,
        draw::stroked(
            draw::circle("grip", c, c, r, draw::solid(knurl)),
            "#000000",
            (0.7 * k).max(0.5),
        ),
    );

    // Turning: the knurl's ribs, the cap and the dot.
    let body = b.add(f.knob, draw::frame("body", 0.0, 0.0, side, side));
    let rc = r * 0.74;
    let ribs = (std::f64::consts::TAU * r / (3.2 * k.max(0.5))).clamp(20.0, 56.0) as u32;
    let ridge: String = (0..ribs)
        .map(|i| {
            let a = 360.0 * f64::from(i) / f64::from(ribs);
            seg(
                draw::polar(c, c, rc + 0.6 * k, a),
                draw::polar(c, c, r - 0.8 * k, a),
                1.3 * k,
            )
        })
        .collect();
    b.add(
        body,
        draw::path("knurl", 0.0, 0.0, side, side, ridge, draw::solid("#3b3c40")),
    );
    b.add(
        body,
        draw::circle(
            "cap",
            c,
            c,
            rc,
            draw::radial_at(
                0.5,
                0.5,
                0.5,
                &[(0.0, cap_mid), (0.8, cap_mid), (1.0, cap_edge)],
            ),
        ),
    );
    b.add(
        body,
        draw::circle(
            "index",
            c,
            c - rc * 0.72,
            (1.9 * k).max(1.1),
            draw::solid("#f6f6f4"),
        ),
    );

    // Fixed: light on the cap and the ribs.
    let rim_light = draw::sector(c, c, rc, r - 0.5 * k, -80.0, 10.0);
    b.add(
        f.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            side,
            side,
            rim_light,
            draw::solid("#ffffff24"),
        ),
    );
    b.add(
        f.knob,
        draw::circle(
            "cap light",
            c - rc * 0.25,
            c - rc * 0.3,
            rc * 0.7,
            draw::radial(&[(0.0, "#ffffff2e"), (1.0, "#ffffff00")]),
        ),
    );
    b.add(
        f.knob,
        draw::stroked(
            draw::circle("cap edge", c, c, rc, draw::solid("#00000000")),
            "#00000066",
            (0.8 * k).max(0.5),
        ),
    );
    finish(b, p, &f, body, d)
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "knob.pedal_lobed",
            family: Family::Knob,
            title: "Pedal knob (lobed)",
            description: "A boutique stompbox's glossy moulded knob seen from above: seven soft lobes round a domed top with one white line from centre to edge (black; `cream`, `grey`, `white` mouldings print a dark line). Legend above (or `legend_below`); `dots=N` prints a ring of dots. Size = outer diameter.",
            variants: &["black", "cream", "grey", "white"],
            default_size: 48.0,
            binds: "param",
            build: pedal_lobed,
        },
        ComponentDef {
            id: "knob.pedal_grip",
            family: Family::Knob,
            title: "Pedal knob (knurled grip)",
            description: "A modern pedal's tall knob from above: a knurled black grip round a flat satin cap (`grey` default, `black`, `silver`) with a white dot index; the pedal prints a ring of fine dots round the travel (`dots=N`, default 21). Legend above (or `legend_below`). Size = grip diameter.",
            variants: &["grey", "black", "silver"],
            default_size: 48.0,
            binds: "param",
            build: pedal_grip,
        },
    ]
}
