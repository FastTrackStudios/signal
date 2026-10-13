//! `knob.concentric`: two controls on one shaft — a wide knurled collar
//! and a cap sitting in it (an API 550's frequency ring round its gain
//! cap, a dual-concentric pot on a console).
//!
//! ```text
//! <root control=Knob (outer)>          the collar's box: its diameter (≥ touch)
//!   knob   (square, centred on the shaft)
//!     scale printed round the collar                         (fixed)
//!     collar tiers, lit by the room                          (fixed)
//!     collar body  ← outer Needle: knurl + index line
//!     collar light                                           (fixed)
//!   <cap control=Knob (inner)>         the cap's box, inside the collar's
//!     dial   (square, centred on the shaft)
//!       cap tier                                             (fixed)
//!       cap body   ← inner Needle: index line
//!       cap light                                            (fixed)
//!     inner legend ← inner Label
//!   legend ← outer Label
//! ```
//!
//! The cap's control is *inside* the collar's node and box: that nesting is
//! the declaration `design-lint` reads as a concentric pair (not an
//! overlap), and the runtime's hit test gives the cap its box (it paints
//! later) and the collar the ring round it.

use crate::document::proto::{NodeId, NodeKind, TextAlign, TextCase};
use crate::surface::proto::{ControlKind, SlotRole};
use crate::surface::widgets::Builder;

use crate::surface::kit::controls::parts::{PANEL_INK, lin, rad, shade, text_w, tint};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// Cap diameter, as a fraction of the collar's.
const CAP: f64 = 0.56;

/// `(collar, collar index, cap, cap index, cap is metal)` for a variant.
fn colors(variant: Option<&str>) -> (&'static str, &'static str, &'static str, &'static str, bool) {
    match variant {
        Some("black_red") => ("#1c1c1f", "#f1f1ef", "#b3281f", "#f4ece8", false),
        Some("grey_black") => ("#7a7e84", "#f4f4f2", "#1c1d20", "#f1f1ef", false),
        Some("cream_black") => ("#d9d1bd", "#1c1d20", "#1c1d20", "#f1f1ef", false),
        // The Neve 1073's pair: grey collar, grey cap, white lines on both.
        Some("grey") => ("#71757c", "#f4f4f2", "#8b9098", "#f7f7f5", false),
        _ => ("#1c1c1f", "#f1f1ef", "#c4c6c9", "#1a1b1e", true),
    }
}

fn caption(
    name: &str,
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    ink: &str,
    case: TextCase,
) -> crate::document::proto::Node {
    let w = text_w(s, size) + size * 2.0;
    let mut n = draw::text(
        name,
        s,
        cx - w / 2.0,
        cy - size * 0.68,
        w,
        size,
        ink,
        TextAlign::Center,
    );
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(650.0);
        t.text_case = case;
        t.letter_spacing = size * 0.06;
    }
    n
}

/// Ridges round a band `r0..r1` of a disc centred at `c`: a knurl.
fn knurl(c: f64, r0: f64, r1: f64, n: u32, w: f64) -> String {
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

#[allow(clippy::too_many_lines)]
fn concentric(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(76.0).max(24.0);
    let k = d / 76.0;
    let inner = p.inner.as_deref().cloned().unwrap_or_default();
    let (collar_c, collar_ix, cap_c, cap_ix, metal) = colors(p.variant.as_deref());
    let ink = p.ink_color(PANEL_INK);
    // Options make the collar stepped (it prints them and clicks through
    // exactly its detents); an authored scale or sweep wins.
    let stepped = p.options.as_ref().filter(|o| !o.is_empty());
    let sweep = p.sweep.unwrap_or_else(|| {
        stepped.map_or(300.0, |o| {
            crate::surface::kit::registry::detent_sweep(o.len())
        })
    });
    let labels: Vec<String> = p
        .scale
        .clone()
        .or_else(|| stepped.cloned())
        .unwrap_or_else(|| (0..=10).step_by(2).map(|i| i.to_string()).collect());

    // The drawing, in a square centred on the shaft with room for the scale:
    // labels clear the ticks, at a readable size.
    let r = d / 2.0;
    let (ring_r, label_r) = (r + 4.0 * k, r + 12.0 * k);
    let clear = ring_r + 7.5 * k;
    let size = (7.0 * k).max(draw::SCALE_TYPE_MIN);
    let n = labels.len();
    let at = |i: usize| {
        if n < 2 {
            0.5
        } else {
            i as f64 / (n - 1) as f64
        }
    };
    let angle = |t: f64| (t - 0.5) * sweep;
    let extents: Vec<(f64, f64, f64, f64)> = labels
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .map(|(i, s)| draw::radial_extent(s, clear, angle(at(i)), size))
        .collect();
    let reach = extents
        .iter()
        .map(|e| e.0.abs().max(e.1.abs()).max(e.2.abs()).max(e.3.abs()))
        .fold(0.0, f64::max);
    let bottom = extents.iter().map(|e| e.3).fold(label_r, f64::max);
    let half = (label_r + 8.0 * k).max(reach + 3.0);
    let side = 2.0 * half;
    let c = half;
    let touch = p.touch.unwrap_or(0.0);
    let bx = d.max(touch);
    let o = bx / 2.0 - c;
    let legend_size = (9.0 * k).max(7.5);
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, bx, bx));
    let knob = b.add(root, draw::frame("knob", o, o, side, side));

    // ── the panel's scale, round the collar (none for an empty scale) ──
    let mut ticks = String::new();
    for i in 0..n {
        let a = angle(at(i));
        let (x0, y0) = draw::polar(c, c, ring_r, a);
        let (x1, y1) = draw::polar(c, c, ring_r + 4.0 * k, a);
        let (nx, ny) = (
            a.to_radians().cos() * 0.7 * k,
            a.to_radians().sin() * 0.7 * k,
        );
        ticks.push_str(&draw::polygon(&[
            (x0 - nx, y0 - ny),
            (x0 + nx, y0 + ny),
            (x1 + nx, y1 + ny),
            (x1 - nx, y1 - ny),
        ]));
        ticks.push(' ');
    }
    if n > 0 {
        b.add(
            knob,
            draw::with_opacity(
                draw::path(
                    "scale majors",
                    0.0,
                    0.0,
                    side,
                    side,
                    ticks,
                    draw::solid(&ink),
                ),
                0.9,
            ),
        );
    }
    for (i, s) in labels.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let mut t = draw::radial_label("numeral", s, (c, c), clear, angle(at(i)), size, &ink);
        if let NodeKind::Text(td) = &mut t.kind {
            td.font_weight = Some(700.0);
        }
        b.add(knob, t);
    }

    // ── the collar: fixed tiers, the turning knurl and index, the light ──
    b.add(
        knob,
        draw::circle("collar shadow", c, c + 1.5 * k, r, draw::solid("#00000059")),
    );
    let collar = draw::circle(
        "collar",
        c,
        c,
        r,
        lin(
            160.0,
            &[
                (0.0, tint(collar_c, 0.14)),
                (0.45, collar_c.to_string()),
                (1.0, shade(collar_c, 0.35)),
            ],
        ),
    );
    let collar = draw::stroked(collar, "#00000080", (0.8 * k).max(0.5));
    b.add(
        knob,
        draw::with_effects(
            collar,
            vec![draw::drop_shadow("#00000066", 0.0, 2.0 * k, 4.0 * k)],
        ),
    );
    let collar_body = b.add(knob, draw::frame("collar body", 0.0, 0.0, side, side));
    let rc = r * CAP;
    let ridges = (std::f64::consts::PI * d / (3.2 * k.max(0.5))).clamp(24.0, 72.0) as u32;
    b.add(
        collar_body,
        draw::path(
            "knurl",
            0.0,
            0.0,
            side,
            side,
            knurl(c, rc + 3.0 * k, r - 1.0 * k, ridges, 0.9 * k),
            draw::solid(&shade(collar_c, 0.45)),
        ),
    );
    let lw = (2.2 * k).max(1.2);
    b.add(
        collar_body,
        draw::rounded(
            draw::rect(
                "collar index",
                c - lw / 2.0,
                c - r + 1.0 * k,
                lw,
                r - rc - 3.0 * k,
                draw::solid(collar_ix),
            ),
            lw / 2.0,
        ),
    );
    let light = draw::sector(c, c, rc + 1.0 * k, r - 0.5 * k, -60.0, 20.0);
    b.add(
        knob,
        draw::path(
            "collar light",
            0.0,
            0.0,
            side,
            side,
            light,
            draw::solid("#ffffff1c"),
        ),
    );

    // ── the cap: its own control, nested in the collar's ──
    let dc = 2.0 * rc;
    let bi = dc
        .max(
            inner
                .touch
                .unwrap_or(crate::surface::kit::registry::TOUCH_MIN),
        )
        .min(bx);
    let shaft = bx / 2.0;
    let cap_root = b.add(
        root,
        draw::frame(
            &if inner.name.is_empty() {
                format!("{} cap", p.name)
            } else {
                inner.name.clone()
            },
            shaft - bi / 2.0,
            shaft - bi / 2.0,
            bi,
            bi,
        ),
    );
    let dial = b.add(
        cap_root,
        draw::frame("dial", bi / 2.0 - rc, bi / 2.0 - rc, dc, dc),
    );
    let face = if metal {
        rad(
            0.5,
            0.5,
            0.5,
            &[
                (0.0, "#e8e9eb".into()),
                (0.7, cap_c.to_string()),
                (1.0, shade(cap_c, 0.3)),
            ],
        )
    } else {
        rad(
            0.36,
            0.28,
            0.9,
            &[
                (0.0, tint(cap_c, 0.25)),
                (0.5, cap_c.to_string()),
                (1.0, shade(cap_c, 0.4)),
            ],
        )
    };
    let cap = draw::stroked(
        draw::circle("cap", rc, rc, rc, face),
        "#00000099",
        (0.8 * k).max(0.5),
    );
    b.add(
        dial,
        draw::with_effects(
            cap,
            vec![draw::drop_shadow("#00000080", 0.0, 1.5 * k, 3.0 * k)],
        ),
    );
    let cap_body = b.add(dial, draw::frame("cap body", 0.0, 0.0, dc, dc));
    if metal {
        // Lathe rings: rotation-invariant, so they turn with the cap.
        for f in [0.35, 0.55, 0.75] {
            b.add(
                cap_body,
                draw::stroked(
                    draw::circle("turning", rc, rc, rc * f, draw::solid("#00000000")),
                    "#0000001f",
                    0.6,
                ),
            );
        }
    }
    let iw = (2.0 * k).max(1.1);
    b.add(
        cap_body,
        draw::rounded(
            draw::rect(
                "cap index",
                rc - iw / 2.0,
                2.0 * k,
                iw,
                rc * 0.62,
                draw::solid(cap_ix),
            ),
            iw / 2.0,
        ),
    );
    b.add(
        dial,
        draw::circle(
            "cap light",
            rc * 0.78,
            rc * 0.62,
            rc * 0.42,
            draw::radial(&[(0.0, "#ffffff40"), (1.0, "#ffffff00")]),
        ),
    );

    // ── legends: the collar's, then the cap's under it ──
    let legend_y = o + c + bottom + 4.0 * k + legend_size * 0.6;
    let outer_legend = b.add(
        root,
        caption(
            "legend",
            p.legend.as_deref().unwrap_or(""),
            shaft,
            legend_y,
            legend_size,
            &ink,
            p.case(),
        ),
    );
    let inner_ink = inner.ink_color(&ink);
    let inner_y = legend_y + legend_size * 1.3 - (shaft - bi / 2.0);
    let inner_legend = b.add(
        cap_root,
        caption(
            "inner legend",
            inner.legend.as_deref().unwrap_or(""),
            bi / 2.0,
            inner_y,
            legend_size * 0.85,
            &inner_ink,
            inner.case(),
        ),
    );

    let outer_ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: sweep,
        })
        .slot(SlotRole::Needle, collar_body.0)
        .slot(SlotRole::Label, outer_legend.0);
    b.controls.insert(root, outer_ctl);
    let inner_ctl = inner
        .control(ControlKind::Knob {
            sweep_degrees: inner.sweep.unwrap_or(300.0),
        })
        .slot(SlotRole::Needle, cap_body.0)
        .slot(SlotRole::Label, inner_legend.0);
    b.controls.insert(cap_root, inner_ctl);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.concentric",
        family: Family::Knob,
        title: "Concentric",
        description: "Two controls on one shaft: a wide knurled collar with a white index, and a cap sitting in it with its own index (an API 550's frequency ring round its gain cap). The collar binds `Place::binding`; the cap binds `Place::inner` (its binding, legend, sweep). `options` make the collar stepped (printed, ≤ 60° a detent). Variants: `grey` is the Neve 1073's all-grey pair. The box is tight to the collar and the cap's control is nested in it, so hit-testing gives the cap its disc and the collar the ring. Size = collar diameter.",
        variants: &[
            "black_silver",
            "black_red",
            "grey_black",
            "cream_black",
            "grey",
        ],
        default_size: 76.0,
        binds: "param",
        build: concentric,
    }]
}
