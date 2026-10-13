//! `knob.chrome_skirt`: the 1970s Japanese tape echo's knob, drawn from
//! photographs of the Roland RE-201.
//!
//! A polished chrome skirt ring, a black knurled grip inside it, and a
//! spun-aluminium cap with a black line from its centre to its edge; the
//! panel prints eleven short ticks round it with a small triangle over the
//! top one, and no numbers.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     ticks, centre triangle                         (fixed, panel ink)
//!     skirt, grip, cap — lit by the room              (fixed)
//!     body  ← Needle: the grip's knurl, the cap's line
//!     light over the chrome and the cap              (fixed)
//!   legend ← Label
//! ```
//!
//! Size = the chrome skirt's diameter. Options: `ticks=N` (default 11).

use document_proto::{NodeId, NodeKind, TextAlign, TextCase};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::{PANEL_INK, text_w};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// The grip's radius, in skirt radii (measured: 90 px grip in a 125 px skirt).
const GRIP: f64 = 0.74;
/// The cap's radius, in skirt radii.
const CAP: f64 = 0.60;

fn caption(
    s: &str,
    cx: f64,
    cy: f64,
    size: f64,
    ink: &str,
    case: TextCase,
) -> document_proto::Node {
    let w = text_w(s, size) * 1.2 + size * 2.0;
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
        t.letter_spacing = size * 0.06;
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

#[allow(clippy::too_many_lines)]
fn chrome_skirt(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(54.0).max(16.0);
    let r = d / 2.0;
    let k = d / 54.0;
    let ink = p.ink_color(PANEL_INK);
    let sweep = p.sweep.unwrap_or(300.0);
    let n: usize = p
        .opt("ticks")
        .and_then(|v| v.parse().ok())
        .unwrap_or(11)
        .max(2);
    let (t0, t1) = (r + 3.0 * k, r + 7.5 * k);
    let half = t1 + 7.0 * k;
    let side = 2.0 * half;
    let c = half;
    let legend_size = (8.0 * k).max(8.0);
    // The legend sits over the knob, as the unit prints it.
    let top = legend_size * 1.4;
    let name = if p.name.is_empty() {
        "Chrome skirt"
    } else {
        p.name.as_str()
    };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side + top));
    let knob = b.add(root, draw::frame("knob", 0.0, top, side, side));

    // ── the panel's ticks and the triangle over the centre one ──
    let angle = |i: usize| (i as f64 / (n - 1) as f64 - 0.5) * sweep;
    // An empty authored scale prints nothing round the knob.
    if p.scale.as_ref().is_none_or(|s| !s.is_empty()) {
        let ticks: String = (0..n)
            .map(|i| {
                seg(
                    draw::polar(c, c, t0, angle(i)),
                    draw::polar(c, c, t1, angle(i)),
                    (1.1 * k).max(0.8),
                )
            })
            .collect();
        b.add(
            knob,
            draw::path(
                "scale majors",
                0.0,
                0.0,
                side,
                side,
                ticks,
                draw::solid(&ink),
            ),
        );
        let (tx, ty) = (c, c - t1 - 1.2 * k);
        let tri = draw::polygon(&[
            (tx - 2.4 * k, ty - 3.6 * k),
            (tx + 2.4 * k, ty - 3.6 * k),
            (tx, ty),
        ]);
        b.add(
            knob,
            draw::path("centre mark", 0.0, 0.0, side, side, tri, draw::solid(&ink)),
        );
    }

    // ── fixed: the chrome skirt, the grip, the cap ──
    b.add(
        knob,
        draw::circle(
            "skirt shadow",
            c + 0.3 * k,
            c + 2.0 * k,
            r,
            draw::solid("#0000008c"),
        ),
    );
    let chrome = draw::conic(
        215.0,
        &[
            (0.0, "#fbfbfa"),
            (0.10, "#9a9c9f"),
            (0.22, "#e9eaeb"),
            (0.36, "#6d7074"),
            (0.5, "#f4f4f3"),
            (0.62, "#8b8e92"),
            (0.76, "#dfe0e1"),
            (0.88, "#6a6d71"),
            (1.0, "#fbfbfa"),
        ],
    );
    let skirt = draw::stroked(
        draw::circle("skirt", c, c, r, chrome),
        "#2a2b2e",
        (0.8 * k).max(0.6),
    );
    b.add(
        knob,
        draw::with_effects(
            skirt,
            vec![draw::drop_shadow("#00000080", 0.0, 2.0 * k, 4.0 * k)],
        ),
    );
    // The skirt's turned face: a darker band where it steps up to the grip.
    let rg = r * GRIP;
    b.add(
        knob,
        draw::stroked(
            draw::circle("skirt step", c, c, rg + 1.6 * k, draw::solid("#00000000")),
            "#00000073",
            1.4 * k,
        ),
    );
    let grip = draw::circle(
        "grip",
        c,
        c,
        rg,
        draw::linear(
            165.0,
            &[(0.0, "#2e2f33"), (0.5, "#151618"), (1.0, "#060607")],
        ),
    );
    b.add(knob, draw::stroked(grip, "#000000", (0.7 * k).max(0.5)));
    let rc = r * CAP;
    let cap = draw::circle(
        "cap",
        c,
        c,
        rc,
        draw::radial_at(
            0.5,
            0.5,
            0.5,
            &[
                (0.0, "#e9eaea"),
                (0.6, "#d3d4d4"),
                (0.92, "#b3b4b4"),
                (1.0, "#7b7c7d"),
            ],
        ),
    );
    b.add(knob, draw::stroked(cap, "#0000008c", (0.6 * k).max(0.5)));
    let sheen = draw::conic(
        200.0,
        &[
            (0.0, "#ffffff66"),
            (0.13, "#ffffff00"),
            (0.25, "#00000030"),
            (0.38, "#ffffff00"),
            (0.5, "#ffffff5c"),
            (0.63, "#ffffff00"),
            (0.75, "#0000002b"),
            (0.88, "#ffffff00"),
            (1.0, "#ffffff66"),
        ],
    );
    b.add(knob, draw::circle("cap sheen", c, c, rc - 0.4 * k, sheen));

    // ── turning: the grip's ribs and the cap's line ──
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let ribs = (std::f64::consts::TAU * rg / (2.4 * k.max(0.5))).clamp(24.0, 72.0) as u32;
    let ridge: String = (0..ribs)
        .map(|i| {
            let a = 360.0 * f64::from(i) / f64::from(ribs);
            seg(
                draw::polar(c, c, rc + 0.8 * k, a),
                draw::polar(c, c, rg - 0.4 * k, a),
                0.8 * k,
            )
        })
        .collect();
    b.add(
        body,
        draw::path("knurl", 0.0, 0.0, side, side, ridge, draw::solid("#34353a")),
    );
    let lw = (2.0 * k).max(1.2);
    let line = draw::rounded(
        draw::rect(
            "index",
            c - lw / 2.0,
            c - rc + 0.8 * k,
            lw,
            rc - 0.8 * k,
            draw::solid("#141416"),
        ),
        lw / 2.0,
    );
    b.add(body, line);

    // ── fixed: the room's light ──
    let rim = draw::sector(c, c, r - 1.4 * k, r - 0.3 * k, -75.0, 35.0);
    b.add(
        knob,
        draw::path(
            "rim light",
            0.0,
            0.0,
            side,
            side,
            rim,
            draw::solid("#ffffff66"),
        ),
    );
    b.add(
        knob,
        draw::circle(
            "cap light",
            c - rc * 0.3,
            c - rc * 0.35,
            rc * 0.5,
            draw::radial(&[(0.0, "#ffffff47"), (1.0, "#ffffff00")]),
        ),
    );

    let legend = b.add(
        root,
        caption(
            p.legend.as_deref().unwrap_or(""),
            c,
            top * 0.45,
            legend_size,
            &ink,
            p.case(),
        ),
    );
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: sweep,
        })
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::Label, legend.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, root, (c, c + top), d, touch);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.chrome_skirt",
        family: Family::Knob,
        title: "Chrome skirt (tape echo)",
        description: "A 1970s tape echo's knob: a polished chrome skirt ring, a black knurled grip and a spun-aluminium cap with a black line from centre to edge; the panel prints eleven short ticks with a triangle over the centre one and no numbers, the legend above. For dark panels. Size = skirt diameter; `options` `ticks=N`.",
        variants: &["chrome"],
        default_size: 54.0,
        binds: "param",
        build: chrome_skirt,
    }]
}
