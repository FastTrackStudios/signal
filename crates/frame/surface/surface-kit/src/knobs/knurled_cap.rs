//! `knob.knurled_cap`: the FET limiter's knob, drawn from photographs of
//! the 1176 in its three finishes.
//!
//! A short black collar knurled all round its rim, a spun-aluminium cap
//! nearly as wide as the collar set flush in it, and — on the big INPUT /
//! OUTPUT knobs — a thin plastic dial disc under the collar that turns with
//! it and carries the index line out to the panel's dotted scale. The disc
//! is smoked on the blackface and clear on the silver units; the small
//! ATTACK / RELEASE knobs have no disc and are read by a small engraved ring
//! near the edge of the cap.
//!
//! ```text
//! <root control=Knob{sweep}>
//!   knob   (square, centred on the shaft)
//!     scale: dots, numerals                          (fixed, panel ink)
//!     disc, collar, cap — lit by the room             (fixed)
//!     body  ← Needle: the knurl, the disc's index line, the cap's ring
//!     light over the cap and collar                  (fixed)
//!   legend ← Label
//! ```
//!
//! Size = the collar's diameter. Variants: `smoke` (dark disc, white line —
//! the blackface), `clear` (clear disc, white line — the early silver unit),
//! `clear_dark` (clear disc, black line — the late silver unit), `bare` (no
//! disc, a ring on the cap — the time knobs). Options: `dots=N` minor dots
//! between printed labels (default 1); an empty scale label is a dot alone.

use document_proto::{NodeId, NodeKind, TextAlign, TextCase};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::{PANEL_INK, text_w};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// The dial disc's radius, in collar radii (measured: 175 px disc on a
/// 125 px collar).
const DISC: f64 = 1.40;
/// The cap's radius, in collar radii (90 px cap in a 125 px collar).
const CAP: f64 = 0.78;
/// The attenuation scale the big knobs print by default.
const ATTEN: &[&str] = &["∞", "48", "36", "30", "24", "18", "12", "6", "0"];

/// A dial disc: its fill stops, its rim, its index line.
type Disc = (&'static [(f64, &'static str)], &'static str, &'static str);

/// A variant's disc, or `None` for a bare knob.
fn disc(variant: Option<&str>) -> Option<Disc> {
    match variant {
        Some("bare") => None,
        Some("clear") => Some((
            &[(0.0, "#ffffff30"), (0.55, "#ffffff1a"), (1.0, "#0000001f")],
            "#ffffff66",
            "#f4f4f0",
        )),
        Some("clear_dark") => Some((
            &[(0.0, "#ffffff2e"), (0.55, "#ffffff18"), (1.0, "#0000001c")],
            "#ffffff5e",
            "#1b1b1d",
        )),
        _ => Some((
            &[(0.0, "#505055f0"), (0.6, "#3c3c41f0"), (1.0, "#29292df0")],
            "#ffffff24",
            "#d8d8d2",
        )),
    }
}

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
        t.letter_spacing = size * 0.04;
    }
    n
}

/// Ridges round the band `r0..r1` about `c`: the knurl, rotation-invariant.
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

fn circle_d(cx: f64, cy: f64, r: f64, ccw: bool) -> String {
    let sweep = u8::from(!ccw);
    format!(
        "M {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 {sweep} {:.3} {cy:.3} A {r:.3} {r:.3} 0 1 {sweep} {:.3} {cy:.3} Z ",
        cx + r,
        cx - r,
        cx + r
    )
}

#[allow(clippy::too_many_lines)]
fn knurled_cap(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(56.0).max(16.0);
    let r = d / 2.0;
    let k = d / 56.0;
    let dsc = disc(p.variant.as_deref());
    let rd = if dsc.is_some() { r * DISC } else { r };
    let ink = p.ink_color(PANEL_INK);
    let sweep = p.sweep.unwrap_or(300.0);
    let labels: Vec<String> = p
        .scale
        .clone()
        .unwrap_or_else(|| ATTEN.iter().map(ToString::to_string).collect());
    let between: usize = p.opt("dots").and_then(|v| v.parse().ok()).unwrap_or(1);

    // The dot ring: every label position, and `between` dots in each gap.
    let n = labels.len();
    let at = |i: usize| {
        if n < 2 {
            0.5
        } else {
            i as f64 / (n - 1) as f64
        }
    };
    let angle = |t: f64| (t - 0.5) * sweep;
    let ring_r = rd + 4.0 * k.max(0.6);
    let dot_r = (1.15 * k).max(0.8);
    let clear = ring_r + dot_r + 2.0 * k;
    let size = (8.2 * k).max(draw::SCALE_TYPE_MIN);
    let extents: Vec<(f64, f64, f64, f64)> = labels
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .map(|(i, s)| draw::radial_extent(s, clear, angle(at(i)), size))
        .collect();
    let reach = extents
        .iter()
        .map(|e| e.0.abs().max(e.1.abs()).max(e.2.abs()).max(e.3.abs()))
        .fold(ring_r + 3.0 * k, f64::max);
    let bottom = extents.iter().map(|e| e.3).fold(ring_r + dot_r, f64::max);
    let half = reach + 2.0 * k;
    let side = 2.0 * half;
    let c = half;
    let legend_size = (8.5 * k).max(8.0);
    let legend_y = c + bottom + 3.0 + legend_size * 0.7;
    let name = if p.name.is_empty() {
        "Knurled cap"
    } else {
        p.name.as_str()
    };
    let root = b.add(
        parent,
        draw::frame(name, 0.0, 0.0, side, legend_y + legend_size),
    );
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));

    // ── the panel's printed scale ──
    let mut dots = String::new();
    let mut positions: Vec<f64> = Vec::new();
    for i in 0..n {
        positions.push(at(i));
        if i + 1 < n {
            for j in 1..=between {
                positions.push(at(i) + (at(i + 1) - at(i)) * j as f64 / (between + 1) as f64);
            }
        }
    }
    for t in positions {
        let (x, y) = draw::polar(c, c, ring_r, angle(t));
        dots.push_str(&circle_d(x, y, dot_r, false));
    }
    if n > 0 {
        b.add(
            knob,
            draw::path("scale dots", 0.0, 0.0, side, side, dots, draw::solid(&ink)),
        );
    }
    for (i, s) in labels.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let mut t = draw::radial_label("numeral", s, (c, c), clear, angle(at(i)), size, &ink);
        if let NodeKind::Text(td) = &mut t.kind {
            td.font_weight = Some(600.0);
        }
        b.add(knob, t);
    }

    // ── fixed: the disc, the collar and the cap, as the room lights them ──
    if let Some((stops, rim, _)) = dsc {
        let disc = draw::circle("disc", c, c, rd, draw::radial_at(0.42, 0.34, 0.75, stops));
        let disc = draw::stroked(disc, rim, (0.8 * k).max(0.6));
        b.add(
            knob,
            draw::with_effects(
                disc,
                vec![draw::drop_shadow("#00000059", 0.0, 1.2 * k, 2.5 * k)],
            ),
        );
        // The disc's bevelled edge catching the light at the top.
        let lip = draw::sector(c, c, rd - 1.6 * k, rd - 0.4 * k, -70.0, 45.0);
        b.add(
            knob,
            draw::path(
                "disc lip",
                0.0,
                0.0,
                side,
                side,
                lip,
                draw::solid("#ffffff2e"),
            ),
        );
    }
    b.add(
        knob,
        draw::circle(
            "collar shadow",
            c + 0.4 * k,
            c + 2.2 * k,
            r,
            draw::solid("#00000080"),
        ),
    );
    let collar = draw::circle(
        "collar",
        c,
        c,
        r,
        draw::linear(
            165.0,
            &[(0.0, "#2c2c30"), (0.45, "#141416"), (1.0, "#060607")],
        ),
    );
    let collar = draw::stroked(collar, "#000000cc", (0.8 * k).max(0.6));
    b.add(
        knob,
        draw::with_effects(
            collar,
            vec![draw::drop_shadow("#0000008c", 0.0, 2.5 * k, 5.0 * k)],
        ),
    );

    // The cap: spun aluminium, flush in the collar. Its concentric turning
    // marks are rotation-invariant; the light on it is the room's.
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
                (0.0, "#e2e3e2"),
                (0.55, "#cfd0cf"),
                (0.9, "#b4b5b3"),
                (1.0, "#8d8e8c"),
            ],
        ),
    );
    b.add(
        knob,
        draw::with_effects(
            draw::stroked(cap, "#00000080", (0.6 * k).max(0.5)),
            vec![draw::inner_shadow("#00000059", 0.0, -0.8 * k, 1.5 * k)],
        ),
    );
    let sheen = draw::conic(
        200.0,
        &[
            (0.0, "#ffffff5c"),
            (0.13, "#ffffff00"),
            (0.25, "#0000002e"),
            (0.38, "#ffffff00"),
            (0.5, "#ffffff52"),
            (0.63, "#ffffff00"),
            (0.75, "#00000029"),
            (0.88, "#ffffff00"),
            (1.0, "#ffffff5c"),
        ],
    );
    b.add(knob, draw::circle("cap sheen", c, c, rc - 0.4 * k, sheen));
    for f in [0.3, 0.52, 0.76] {
        b.add(
            knob,
            draw::stroked(
                draw::circle("turning", c, c, rc * f, draw::solid("#00000000")),
                "#00000014",
                0.5,
            ),
        );
    }

    // ── turning: the knurl round the cap, the disc's line, the cap's ring ──
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let ridges = (std::f64::consts::PI * d / (2.6 * k.max(0.5))).clamp(28.0, 84.0) as u32;
    b.add(
        body,
        draw::path(
            "knurl",
            0.0,
            0.0,
            side,
            side,
            knurl(c, rc + 0.6 * k, r - 0.3 * k, ridges, 0.9 * k),
            draw::solid("#4c4c52"),
        ),
    );
    b.add(
        body,
        draw::path(
            "knurl shade",
            0.0,
            0.0,
            side,
            side,
            knurl(c, rc + 0.6 * k, r - 0.3 * k, ridges, 0.35 * k),
            draw::solid("#000000"),
        ),
    );
    if let Some((_, _, line)) = dsc {
        let lw = (1.5 * k).max(1.0);
        let n = draw::rounded(
            draw::rect(
                "index",
                c - lw / 2.0,
                c - rd + 1.2 * k,
                lw,
                rd - r - 1.8 * k,
                draw::solid(line),
            ),
            lw / 2.0,
        );
        b.add(body, n);
    } else {
        // The time knobs' index: a small ring engraved near the cap's edge.
        let (x, y) = draw::polar(c, c, rc * 0.64, 0.0);
        let ring = draw::circle("index", x, y, rc * 0.17, draw::solid("#00000000"));
        b.add(body, draw::stroked(ring, "#1a1a1c", (1.0 * k).max(0.8)));
    }
    // The room's light on the collar's top edge and the cap's rim.
    let lit = draw::sector(c, c, r - 1.2 * k, r - 0.2 * k, -80.0, 30.0);
    b.add(
        knob,
        draw::path(
            "collar light",
            0.0,
            0.0,
            side,
            side,
            lit,
            draw::solid("#ffffff2a"),
        ),
    );
    let glint = draw::circle(
        "cap light",
        c - rc * 0.28,
        c - rc * 0.34,
        rc * 0.5,
        draw::radial(&[(0.0, "#ffffff4d"), (1.0, "#ffffff00")]),
    );
    b.add(knob, glint);

    let legend = b.add(
        root,
        caption(
            p.legend.as_deref().unwrap_or(""),
            c,
            legend_y,
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
        crate::registry::tighten(b, root, (c, c), d, touch);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.knurled_cap",
        family: Family::Knob,
        title: "Knurled cap (FET limiter)",
        description: "The FET limiter's knob: a short black collar knurled round its rim with a spun-aluminium cap nearly as wide set flush in it. The big knobs sit on a thin dial disc that turns with them and carries the index line out to a dotted scale (`smoke` disc for a black panel, `clear` / `clear_dark` for silver); `bare` has no disc and a small engraved ring on the cap (attack/release). Size = collar diameter; `options` `dots=N` sets the dots between printed labels, an empty label prints a dot alone.",
        variants: &["smoke", "clear", "clear_dark", "bare"],
        default_size: 56.0,
        binds: "param",
        build: knurled_cap,
    }]
}
