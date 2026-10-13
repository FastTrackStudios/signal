//! Knobs of modern boutique pedals and amp faceplates.
//!
//! - `knob.teardrop`: a black fluted skirt under a glossy teardrop cap
//!   whose point is the index — the knob of candy-coloured stompboxes and
//!   of a slate-capped studio amp, inside a ring of printed dots.
//! - `knob.dot_knurl`: a round knurled knob read by a small dot near its
//!   rim (a white one on a green amp strip, a black one on a cream strip, a
//!   smooth taupe dome on a graphic EQ), inside a ring of dots or none.
//! - `knob.fluted_line`: a deeply fluted knob (teal, black, maroon) with a
//!   painted line from the hub to the rim, inside a fine tick ring with
//!   optional `min` / `max` legends — the soft boutique pedal and amp knob.
//!
//! All three follow `KIT.md`: a square `knob` frame centred on the shaft
//! holding the printed scale and the room's light (fixed) and a `body`
//! frame (the `Needle`) holding everything that turns, drawn pointing up.

use document_proto::{NodeId, NodeKind, TextAlign};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::{alpha, lin, rad, shade, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// The dotted scale's default ink: white silkscreen.
const DOT_INK: &str = "#f4f2ec";

/// What every knob here shares: the frame, the scale ring, the legend.
struct Shell {
    root: NodeId,
    knob: NodeId,
    c: f64,
    side: f64,
    sweep: f64,
}

fn circle_d(cx: f64, cy: f64, r: f64) -> String {
    format!(
        "M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 1 {:.3} {cy:.3} Z ",
        cx + r,
        cx - r,
        cx + r
    )
}

/// The printed scale: `dots` round dots (or `ticks` radial ticks) over the
/// sweep at `ring` px from the shaft, and `min`/`max` legends at its ends.
#[derive(Clone, Copy)]
enum Ring {
    Dots { n: usize, r: f64 },
    Ticks { n: usize, len: f64, w: f64 },
    None,
}

fn shell(
    b: &mut Builder<'_>,
    parent: NodeId,
    p: &Place,
    d: f64,
    ring: Ring,
    gap: f64,
    default_name: &str,
) -> Shell {
    let r = d / 2.0;
    let sweep = p.sweep.unwrap_or(300.0);
    let ink = p.ink_color(DOT_INK);
    let minmax = p.flag("minmax");
    // An explicitly empty scale prints no ring.
    let ring = if p.scale.as_ref().is_some_and(Vec::is_empty) {
        Ring::None
    } else {
        ring
    };
    let pad = d / 44.0;
    let reach = match ring {
        Ring::Dots { r: dr, .. } => r + gap + dr * 2.0,
        Ring::Ticks { len, .. } => r + gap + len,
        Ring::None => r,
    };
    let label_size = d * 0.13;
    let half = if minmax {
        reach + label_size * 1.2
    } else {
        reach + pad
    };
    let side = 2.0 * half;
    let c = half;
    let name = if p.name.is_empty() {
        default_name
    } else {
        p.name.as_str()
    };
    let legend_size = (d * 0.19).max(8.0);
    let root = b.add(
        parent,
        draw::frame(name, 0.0, 0.0, side, side + legend_size * 1.6),
    );
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));
    let angle = |t: f64| (t - 0.5) * sweep;
    match ring {
        Ring::Dots { n, r: dr } => {
            let mut dd = String::new();
            for i in 0..n {
                let t = if n < 2 {
                    0.5
                } else {
                    i as f64 / (n - 1) as f64
                };
                let (x, y) = draw::polar(c, c, r + gap + dr, angle(t));
                dd.push_str(&circle_d(x, y, dr));
            }
            b.add(
                knob,
                draw::path("scale dots", 0.0, 0.0, side, side, dd, draw::solid(&ink)),
            );
        }
        Ring::Ticks { n, len, w } => {
            let mut dd = String::new();
            for i in 0..n {
                let t = if n < 2 {
                    0.5
                } else {
                    i as f64 / (n - 1) as f64
                };
                let a = angle(t);
                let (x0, y0) = draw::polar(c, c, r + gap, a);
                let (x1, y1) = draw::polar(c, c, r + gap + len, a);
                let (nx, ny) = (
                    a.to_radians().cos() * w / 2.0,
                    a.to_radians().sin() * w / 2.0,
                );
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
                draw::path("scale ticks", 0.0, 0.0, side, side, dd, draw::solid(&ink)),
            );
        }
        Ring::None => {}
    }
    if minmax {
        for (s, t) in [("min", 0.0), ("max", 1.0)] {
            let (x, y) = draw::polar(c, c, reach + label_size * 0.2, angle(t));
            let w = label_size * 3.0;
            let mut n = draw::text(
                "numeral",
                s,
                x - w / 2.0,
                y - label_size * 0.1,
                w,
                label_size,
                &ink,
                TextAlign::Center,
            );
            if let NodeKind::Text(td) = &mut n.kind {
                td.font_weight = Some(500.0);
            }
            b.add(knob, n);
        }
    }
    Shell {
        root,
        knob,
        c,
        side,
        sweep,
    }
}

fn finish(b: &mut Builder<'_>, p: &Place, s: &Shell, body: NodeId, d: f64) -> NodeId {
    // The legend under the knob (`Place::legend`, else the param's name).
    let size = (d * 0.19).max(8.0);
    let w = s.side * 2.0;
    let mut t = draw::text(
        "legend",
        p.legend.as_deref().unwrap_or(""),
        s.c - w / 2.0,
        s.side + size * 0.2,
        w,
        size,
        &p.ink_color(DOT_INK),
        TextAlign::Center,
    );
    if let NodeKind::Text(td) = &mut t.kind {
        td.font_weight = Some(700.0);
        td.text_case = p.case();
    }
    let legend = b.add(s.root, t);
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: s.sweep,
        })
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::Label, legend.0);

    b.controls.insert(s.root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, s.root, (s.c, s.c), d, touch);
    }
    s.root
}

/// A teardrop outline pointing up: a round head of radius `rh` about the
/// shaft whose top runs out to a point `tip` px above the shaft.
fn teardrop(c: f64, rh: f64, tip: f64) -> String {
    // Tangent points from the tip to the head circle.
    let dist = tip;
    let half = (rh / dist).asin().to_degrees();
    let (tx, ty) = (c, c - tip);
    let a0 = 90.0 - half; // kit angle of the tangent point on the right
    let (rx, ry) = draw::polar(c, c, rh, a0);
    let (lx, ly) = draw::polar(c, c, rh, -a0);
    format!("M {tx:.3} {ty:.3} L {rx:.3} {ry:.3} A {rh:.3} {rh:.3} 0 1 1 {lx:.3} {ly:.3} Z")
}

fn teardrop_knob(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(44.0).max(16.0);
    let r = d / 2.0;
    let k = d / 44.0;
    let dots: usize = p.opt("dots").and_then(|v| v.parse().ok()).unwrap_or(11);
    let ring = if dots == 0 {
        Ring::None
    } else {
        Ring::Dots {
            n: dots,
            r: (1.2 * k).max(0.8),
        }
    };
    let s = shell(b, parent, p, d, ring, 3.5 * k, "Teardrop");
    let (c, side) = (s.c, s.side);
    let cap = match p.variant.as_deref() {
        Some("slate") => "#5d7178",
        Some("cream") => "#ece4d2",
        _ => "#f1efea",
    };
    // Fixed: the skirt's shadow and its round shading.
    b.add(
        s.knob,
        draw::circle("shadow", c, c + 1.6 * k, r, draw::solid("#00000070")),
    );
    let skirt = draw::circle(
        "skirt",
        c,
        c,
        r,
        draw::linear(
            170.0,
            &[(0.0, "#3a3a3d"), (0.5, "#161618"), (1.0, "#050506")],
        ),
    );
    b.add(
        s.knob,
        draw::with_effects(
            skirt,
            vec![draw::drop_shadow("#00000080", 0.0, 2.0 * k, 4.0 * k)],
        ),
    );
    // Turning: the flutes, the cap and its point.
    let body = b.add(s.knob, draw::frame("body", 0.0, 0.0, side, side));
    b.add(
        body,
        draw::path(
            "flutes",
            0.0,
            0.0,
            side,
            side,
            draw::lobed(c, c, r - 0.4 * k, 16, 2.8 * k),
            draw::solid("#2b2b2f"),
        ),
    );
    b.add(
        body,
        draw::path(
            "flute shade",
            0.0,
            0.0,
            side,
            side,
            draw::lobed(c, c, r - 3.4 * k, 16, 1.6 * k),
            draw::solid("#0a0a0b"),
        ),
    );
    let head = r * 0.47;
    let tip = r * 0.9;
    let capd = teardrop(c, head, tip);
    let stops = [
        (0.0, tint(cap, 0.35)),
        (0.55, cap.to_string()),
        (1.0, shade(cap, 0.28)),
    ];
    let cap_node = draw::path(
        "cap",
        0.0,
        0.0,
        side,
        side,
        capd,
        rad(0.5, 0.5, head / side * 1.9, &stops),
    );
    b.add(
        body,
        draw::with_effects(
            cap_node,
            vec![draw::drop_shadow("#00000066", 0.0, 0.0, 1.8 * k)],
        ),
    );
    // Fixed light over the cap: a soft glint up and to the left.
    b.add(
        s.knob,
        draw::circle(
            "glint",
            c - head * 0.3,
            c - head * 0.25,
            head * 0.55,
            draw::radial(&[(0.0, "#ffffff70"), (1.0, "#ffffff00")]),
        ),
    );
    let lit = draw::sector(c, c, r - 1.2 * k, r - 0.2 * k, -80.0, 30.0);
    b.add(
        s.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            side,
            side,
            lit,
            draw::solid("#ffffff24"),
        ),
    );
    finish(b, p, &s, body, d)
}

fn dot_knurl(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(14.0);
    let r = d / 2.0;
    let k = d / 40.0;
    let dots: usize = p.opt("dots").and_then(|v| v.parse().ok()).unwrap_or(11);
    let ring = if dots == 0 {
        Ring::None
    } else {
        Ring::Dots {
            n: dots,
            r: (1.1 * k).max(0.8),
        }
    };
    let s = shell(b, parent, p, d, ring, 3.2 * k, "Dot knurl");
    let (c, side) = (s.c, s.side);
    let (base, dot) = match p.variant.as_deref() {
        Some("black") => ("#1c1c1e", "#f2f0ea"),
        Some("taupe") => ("#b9a898", "#1a1614"),
        _ => ("#eeece6", "#1b1b1c"),
    };
    let smooth = p.flag("smooth");
    b.add(
        s.knob,
        draw::circle(
            "shadow",
            c + 0.6 * k,
            c + 2.0 * k,
            r,
            draw::solid("#00000066"),
        ),
    );
    let stops = [
        (0.0, tint(base, 0.3)),
        (0.5, base.to_string()),
        (1.0, shade(base, 0.35)),
    ];
    let rim = draw::circle("rim", c, c, r, lin(165.0, &stops));
    b.add(
        s.knob,
        draw::with_effects(
            rim,
            vec![draw::drop_shadow("#00000070", 0.0, 2.0 * k, 4.0 * k)],
        ),
    );
    let body = b.add(s.knob, draw::frame("body", 0.0, 0.0, side, side));
    if !smooth {
        let n = (std::f64::consts::PI * d / (2.2 * k)).clamp(24.0, 72.0) as u32;
        b.add(
            body,
            draw::path(
                "knurl",
                0.0,
                0.0,
                side,
                side,
                draw::lobed(c, c, r, n, 1.2 * k),
                draw::solid(&shade(base, 0.22)),
            ),
        );
    }
    let face = r * if smooth { 0.94 } else { 0.84 };
    let face_stops = [
        (0.0, tint(base, 0.25)),
        (0.7, base.to_string()),
        (1.0, shade(base, 0.12)),
    ];
    b.add(
        body,
        draw::circle("face", c, c, face, rad(0.5, 0.5, 0.5, &face_stops)),
    );
    let (dx, dy) = draw::polar(c, c, face * 0.66, 0.0);
    b.add(
        body,
        draw::circle("index", dx, dy, (face * 0.13).max(1.0), draw::solid(dot)),
    );
    b.add(
        s.knob,
        draw::circle(
            "glint",
            c - face * 0.3,
            c - face * 0.35,
            face * 0.6,
            draw::radial(&[(0.0, "#ffffff5a"), (1.0, "#ffffff00")]),
        ),
    );
    finish(b, p, &s, body, d)
}

fn fluted_line(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(44.0).max(16.0);
    let r = d / 2.0;
    let k = d / 44.0;
    let ticks: usize = p.opt("ticks").and_then(|v| v.parse().ok()).unwrap_or(31);
    let lobes: u32 = p.opt("lobes").and_then(|v| v.parse().ok()).unwrap_or(10);
    let tick_len = p
        .opt("ticklen")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(3.6)
        * k;
    let gap = 2.2 * k;
    let ring = if ticks == 0 {
        Ring::None
    } else {
        Ring::Ticks {
            n: ticks,
            len: tick_len,
            w: (0.6 * k).max(0.5),
        }
    };
    // `min` / `max` sit level with the ends of the scale, outboard of it —
    // printed here rather than by the shell, which sets them radially.
    let mut q = p.clone();
    q.options = q
        .options
        .map(|o| o.into_iter().filter(|x| x != "minmax").collect());
    let s = shell(b, parent, &q, d, ring, gap, "Fluted");
    let (c, side) = (s.c, s.side);
    if p.flag("minmax") {
        let ink = p.ink_color(DOT_INK);
        let size = (d * 0.13).max(5.0);
        for (label, t) in [("min", 0.0_f64), ("max", 1.0)] {
            let a = (t - 0.5) * s.sweep;
            let (x, y) = draw::polar(c, c, r + gap + tick_len, a);
            let w = size * 2.4;
            let x0 = if t < 0.5 {
                x - w - size * 0.1
            } else {
                x + size * 0.1
            };
            let align = if t < 0.5 {
                TextAlign::Right
            } else {
                TextAlign::Left
            };
            let mut n = draw::text("numeral", label, x0, y - size * 0.1, w, size, &ink, align);
            if let NodeKind::Text(td) = &mut n.kind {
                td.font_weight = Some(500.0);
            }
            b.add(s.knob, n);
        }
    }
    let (base, line) = match p.variant.as_deref() {
        Some("black") => ("#1d1b1b", "#d9b27a"),
        Some("maroon") => ("#3a1f22", "#e5d6c4"),
        Some("brown") => ("#3b2a24", "#e8c79a"),
        Some("orange") => ("#e8841c", "#7a3a08"),
        _ => ("#4f8584", "#f4f3ee"),
    };
    let line = p
        .opt("line")
        .filter(|l| l.starts_with('#'))
        .map_or_else(|| line.to_string(), ToString::to_string);
    b.add(
        s.knob,
        draw::circle(
            "shadow",
            c + 0.8 * k,
            c + 2.6 * k,
            r,
            draw::solid("#00000066"),
        ),
    );
    // Rounded scallops with sharp valleys between them.
    let scallop = |rr: f64, depth: f64| {
        let n = (lobes * 16) as usize;
        let pts: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let t = i as f64 / n as f64;
                let dip = 1.0 - (t * f64::from(lobes) * std::f64::consts::PI).cos().abs();
                draw::polar(c, c, rr - depth * dip.powf(0.6), t * 360.0)
            })
            .collect();
        draw::polygon(&pts)
    };
    // Turning: the scalloped skirt (a fill radial about the shaft, so it
    // turns without swinging its light), the valleys darker.
    let body = b.add(s.knob, draw::frame("body", 0.0, 0.0, side, side));
    let stops = [
        (0.0, tint(base, 0.1)),
        (0.7, base.to_string()),
        (1.0, shade(base, 0.45)),
    ];
    let skirt = draw::path(
        "skirt",
        0.0,
        0.0,
        side,
        side,
        scallop(r, 3.2 * k),
        rad(0.5, 0.5, r / side * 1.02, &stops),
    );
    b.add(
        body,
        draw::with_effects(
            skirt,
            vec![draw::drop_shadow("#00000073", 0.0, 0.0, 2.5 * k)],
        ),
    );
    b.add(
        body,
        draw::path(
            "flutes",
            0.0,
            0.0,
            side,
            side,
            scallop(r - 1.2 * k, 2.4 * k),
            draw::solid(&alpha(&shade(base, 0.6), 0.3)),
        ),
    );
    let top = r * 0.72;
    let top_stops = [
        (0.0, tint(base, 0.22)),
        (0.75, base.to_string()),
        (1.0, shade(base, 0.25)),
    ];
    let crown = draw::circle("top", c, c, top, rad(0.5, 0.5, 0.5, &top_stops));
    b.add(
        body,
        draw::with_effects(
            crown,
            vec![draw::drop_shadow("#0000004d", 0.0, 0.0, 1.5 * k)],
        ),
    );
    let lw = (2.8 * k).max(1.0);
    let ln = draw::rounded(
        draw::rect(
            "index",
            c - lw / 2.0,
            c - top * 0.86,
            lw,
            top * 0.56,
            draw::solid(&line),
        ),
        lw / 2.0,
    );
    b.add(body, ln);
    // Fixed light: a soft glint up-left on the crown, a lit rim on top.
    b.add(
        s.knob,
        draw::circle(
            "glint",
            c - top * 0.3,
            c - top * 0.38,
            top * 0.75,
            draw::radial(&[(0.0, "#ffffff5e"), (1.0, "#ffffff00")]),
        ),
    );
    let lit = draw::sector(c, c, r - 2.2 * k, r - 0.6 * k, -70.0, 40.0);
    b.add(
        s.knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            side,
            side,
            lit,
            draw::solid("#ffffff2e"),
        ),
    );
    finish(b, p, &s, body, d)
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "knob.teardrop",
            family: Family::Knob,
            title: "Teardrop cap",
            description: "A modern stompbox knob: a black fluted skirt under a glossy teardrop cap whose point is the index, inside a ring of printed dots (`dots=N`, 0 = none). `white` for candy-coloured pedals, `slate` for a studio amp's blue-grey caps, `cream`. Size = skirt diameter.",
            variants: &["white", "slate", "cream"],
            default_size: 44.0,
            binds: "param",
            build: teardrop_knob,
        },
        ComponentDef {
            id: "knob.dot_knurl",
            family: Family::Knob,
            title: "Dot knurl",
            description: "A round knurled amp knob read by a small dot near its rim, inside a ring of printed dots (`dots=N`). `white` (green or wood amp strip), `black` (cream faceplate), `taupe` with the `smooth` flag for a graphic EQ's soft dome. Size = diameter.",
            variants: &["white", "black", "taupe"],
            default_size: 40.0,
            binds: "param",
            build: dot_knurl,
        },
        ComponentDef {
            id: "knob.fluted_line",
            family: Family::Knob,
            title: "Fluted line",
            description: "A deeply fluted boutique knob with a painted line from hub to rim, inside a fine tick ring (`ticks=N`, 0 = none; `ticklen=` at 44 px) with `min` / `max` level with its ends (`minmax` flag); `lobes=N` flutes (default 10). `teal` (soft pastel pedals), `black` with a gold line (pink or brown amp heads), `maroon`, `brown`, `orange`; `line=#hex` recolours the line. Size = diameter.",
            variants: &["teal", "black", "maroon", "brown", "orange"],
            default_size: 44.0,
            binds: "param",
            build: fluted_line,
        },
    ]
}
