//! Amp furniture: the tolex-covered cabinet and the grille cloths in front
//! of the speakers. Panels — no control.

use crate::document::proto::{CornerRadii, NodeId, Stroke, StrokeCap};
use crate::surface::widgets::Builder;

use super::pedal::{Rng, polylines, strokes};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

fn dims(p: &Place, w: f64, h: f64) -> (f64, f64) {
    (
        p.opt("w")
            .and_then(|v| v.parse().ok())
            .or(p.size)
            .unwrap_or(w),
        p.opt("h").and_then(|v| v.parse().ok()).unwrap_or(h),
    )
}

/// `panel.amp_grille`: a speaker grille cloth.
fn grille(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let (w, h) = dims(p, 800.0, 300.0);
    let v = p.variant.as_deref().unwrap_or("diamond");
    let mut root = draw::frame(&p.name, 0.0, 0.0, w, h);
    root.clips_content = true;
    root.corner_radii =
        CornerRadii::uniform(p.opt("r").and_then(|r| r.parse().ok()).unwrap_or(2.0));
    root.effects = vec![draw::inner_shadow("#000000c0", 0.0, 3.0, 10.0)];
    let mut rng = Rng::new(&format!("{v}{w}{h}"));
    match v {
        "wheat" => {
            // Oxblood-and-wheat weave: a fine checker of warm threads.
            root.fills = vec![draw::solid("#7a4a38")];
            let root = b.add(parent, root);
            let s = 5.0;
            let mut a = Vec::new();
            let mut c = Vec::new();
            let mut y = 0.0;
            let mut row = 0;
            while y < h {
                let mut x = if row % 2 == 0 { 0.0 } else { s };
                while x < w {
                    a.push(vec![(x, y + s * 0.5), (x + s * 0.9, y + s * 0.5)]);
                    c.push(vec![(x + s + s * 0.5, y), (x + s + s * 0.5, y + s * 0.9)]);
                    x += 2.0 * s;
                }
                y += s;
                row += 1;
            }
            b.add(root, strokes("weft", w, h, &a, "#c89a78", 2.6));
            b.add(root, strokes("warp", w, h, &c, "#4a2418", 2.2));
            b.add(
                root,
                draw::rect(
                    "sheen",
                    0.0,
                    0.0,
                    w,
                    h,
                    draw::linear(
                        180.0,
                        &[(0.0, "#ffffff10"), (0.6, "#00000000"), (1.0, "#00000040")],
                    ),
                ),
            );
            root
        }
        "basket" => {
            // Black basketweave: small interlocked cells on dark cloth.
            root.fills = vec![draw::solid("#141315")];
            let root = b.add(parent, root);
            let s = 7.0;
            let mut hz = Vec::new();
            let mut vt = Vec::new();
            let mut y = 0.0;
            let mut row = 0;
            while y < h + s {
                let mut x = if row % 2 == 0 { 0.0 } else { s };
                while x < w + s {
                    hz.push(vec![(x + 1.0, y + s * 0.35), (x + s - 1.0, y + s * 0.35)]);
                    hz.push(vec![(x + 1.0, y + s * 0.7), (x + s - 1.0, y + s * 0.7)]);
                    vt.push(vec![
                        (x + s + s * 0.35, y + 1.0),
                        (x + s + s * 0.35, y + s - 1.0),
                    ]);
                    vt.push(vec![
                        (x + s + s * 0.7, y + 1.0),
                        (x + s + s * 0.7, y + s - 1.0),
                    ]);
                    x += 2.0 * s;
                }
                y += s;
                row += 1;
            }
            b.add(root, strokes("weave h", w, h, &hz, "#3a383d", 1.8));
            b.add(root, strokes("weave v", w, h, &vt, "#2c2a2f", 1.8));
            b.add(
                root,
                draw::rect(
                    "sheen",
                    0.0,
                    0.0,
                    w,
                    h,
                    draw::linear(180.0, &[(0.0, "#ffffff0c"), (1.0, "#00000050")]),
                ),
            );
            root
        }
        "mesh" => {
            // Perforated copper mesh.
            root.fills = vec![draw::linear(
                180.0,
                &[(0.0, "#e2b394"), (0.5, "#c89274"), (1.0, "#a87458")],
            )];
            let root = b.add(parent, root);
            let s = 7.0;
            let mut holes = String::new();
            let mut y = s * 0.5;
            let mut row = 0;
            while y < h {
                let mut x = if row % 2 == 0 { s * 0.5 } else { s };
                while x < w {
                    holes.push_str(&format!(
                        "M {:.2} {y:.2} L {:.2} {:.2} L {:.2} {y:.2} L {:.2} {:.2} Z ",
                        x - 2.4,
                        x,
                        y - 2.0,
                        x + 2.4,
                        x,
                        y + 2.0
                    ));
                    x += s;
                }
                y += s * 0.55;
                row += 1;
            }
            b.add(
                root,
                draw::path("holes", 0.0, 0.0, w, h, holes, draw::solid("#2a1710d0")),
            );
            b.add(
                root,
                draw::rect(
                    "sheen",
                    0.0,
                    0.0,
                    w,
                    h,
                    draw::linear(
                        180.0,
                        &[(0.0, "#ffffff30"), (0.3, "#ffffff00"), (1.0, "#00000040")],
                    ),
                ),
            );
            root
        }
        _ => {
            // Dark cloth with a lattice of piped diamonds, red and pale green.
            root.fills = vec![draw::linear(180.0, &[(0.0, "#1b1819"), (1.0, "#0f0d0e")])];
            let root = b.add(parent, root);
            let mut specks = Vec::new();
            for _ in 0..((w * h) / 90.0) as usize {
                let (x, y) = (rng.range(0.0, w), rng.range(0.0, h));
                specks.push(vec![(x, y), (x + 0.6, y)]);
            }
            b.add(root, strokes("cloth", w, h, &specks, "#2e2a2c", 1.0));
            let rows: f64 = p.opt("rows").and_then(|r| r.parse().ok()).unwrap_or(4.0);
            // Two families of parallel lines make the lattice: diamonds
            // `dh` tall and `dw` wide.
            let dh = h / rows;
            let dw = dh * 0.86;
            let run = h * dw / dh;
            let lines = |off: f64| {
                let mut v = Vec::new();
                let mut k = -run - dw;
                while k < w + run + dw {
                    v.push(vec![(k + off, 0.0), (k + off + run, h)]);
                    v.push(vec![(k + off, 0.0), (k + off - run, h)]);
                    k += dw;
                }
                v
            };
            for (name, off, color) in [
                ("piping red", -4.4, "#c8405c"),
                ("piping green", 0.0, "#d6f2e2"),
                ("piping red 2", 4.4, "#c8405c"),
            ] {
                let mut n = draw::path(
                    name,
                    0.0,
                    0.0,
                    w,
                    h,
                    polylines(&lines(off)),
                    draw::solid("#00000000"),
                );
                n.fills.clear();
                n.stroke = Stroke {
                    paints: vec![draw::solid(color)],
                    weight: 1.5,
                    cap: StrokeCap::None,
                    dash_pattern: vec![3.0, 1.6],
                    ..Stroke::default()
                };
                b.add(root, draw::with_opacity(n, 0.85));
            }
            root
        }
    }
}

/// `panel.amp_cabinet`: a tolex-covered cabinet with piping and corner caps.
fn cabinet(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let (w, h) = dims(p, 1200.0, 510.0);
    let (hi, lo, speck) = match p.variant.as_deref() {
        Some("brown") => ("#4a3024", "#2a1a12", "#6a4a38"),
        Some("tan") => ("#b98a64", "#8a603f", "#d4a882"),
        _ => ("#29282a", "#141315", "#3c3a3e"),
    };
    let r = p.opt("r").and_then(|v| v.parse().ok()).unwrap_or(14.0);
    let mut root = draw::frame(&p.name, 0.0, 0.0, w, h);
    root.fills = vec![draw::linear(180.0, &[(0.0, hi), (1.0, lo)])];
    root.corner_radii = CornerRadii::uniform(r);
    root.clips_content = true;
    root.effects = vec![
        draw::inner_shadow("#ffffff26", 0.0, 1.5, 1.5),
        draw::inner_shadow("#00000090", 0.0, -3.0, 8.0),
    ];
    let root = b.add(parent, root);
    // The tolex's pebbled grain.
    let mut rng = Rng::new(&format!("tolex{w}{h}"));
    let mut dots = Vec::new();
    for _ in 0..((w * h) / 70.0) as usize {
        let (x, y) = (rng.range(0.0, w), rng.range(0.0, h));
        dots.push(vec![(x, y), (x + 0.7, y + 0.4)]);
    }
    b.add(
        root,
        draw::with_opacity(strokes("grain", w, h, &dots, speck, 1.1), 0.6),
    );
    if p.opt("corners") != Some("none") {
        let c = (w.min(h) * 0.07).clamp(18.0, 44.0);
        for (x, y, rot) in [
            (0.0, 0.0, 0.0),
            (w - c, 0.0, 90.0),
            (w - c, h - c, 180.0),
            (0.0, h - c, 270.0),
        ] {
            let cap = draw::path(
                "corner",
                x,
                y,
                c,
                c,
                format!(
                    "M 0 0 L {c} 0 L {c} {:.2} Q {:.2} {:.2} {:.2} {c} L 0 {c} Z",
                    c * 0.35,
                    c * 0.4,
                    c * 0.4,
                    c * 0.35
                ),
                draw::linear(
                    135.0,
                    &[(0.0, "#6a6a6e"), (0.4, "#3a3a3e"), (1.0, "#1a1a1c")],
                ),
            );
            let mut cap = draw::with_effects(
                draw::rotated(cap, rot),
                vec![draw::drop_shadow("#00000080", 0.0, 1.0, 2.0)],
            );
            cap.name = "corner".into();
            b.add(root, cap);
        }
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "panel.amp_grille",
            family: Family::Panel,
            title: "Amp grille cloth",
            description: "Speaker grille cloth: `diamond` (dark cloth, a lattice of red and pale-green piped diamonds — a boutique head), `basket` (black basketweave), `wheat` (oxblood-and-wheat checker — a '60s combo), `mesh` (perforated copper — a tabletop unit). `w=`, `h=`, `rows=` (diamond) in options.",
            variants: &["diamond", "basket", "wheat", "mesh"],
            default_size: 800.0,
            binds: "none",
            build: grille,
        },
        ComponentDef {
            id: "panel.amp_cabinet",
            family: Family::Panel,
            title: "Amp cabinet",
            description: "A tolex-covered amp cabinet front: pebbled vinyl (black, brown or tan), a soft top light, and dark metal corner caps (`corners=none` to leave them off). `w=`, `h=`, `r=` in options. Put the faceplate and grille in it.",
            variants: &["black", "brown", "tan"],
            default_size: 1200.0,
            binds: "none",
            build: cabinet,
        },
    ]
}
