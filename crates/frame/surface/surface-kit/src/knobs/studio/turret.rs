//! The class-A console module's knobs, drawn from photographs.
//!
//! `knob.turret`: a small dark fluted turret with a flat grey crown and a
//! white line, standing in a clear, silvered skirt — or, in `red` / `blue`,
//! a coloured cap with a flat wing through it (the gain and high-pass
//! selectors) — inside the module's dotted scale.
//!
//! `knob.turret_pair`: the module's concentric pair — a knurled aluminium
//! collar (the outer control: the stepped frequency) with the grey turret
//! in it (the inner control: the band's gain).
//!
//! ```text
//! knob
//!   scale marks (dots), numerals            (fixed, panel print)
//!   cast shadow, skirt / collar tiers       (fixed)
//!   body ← Needle: flutes, crown, index (or the collar's knurl and line)
//!   light                                   (fixed)
//! ```

use document_proto::NodeId;
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{Mark, Scale, bar, caption, close, knurl, open, ring};
use crate::controls::parts::{rad, shade, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place, TOUCH_MIN};

const INK: &str = "#eef0f2";

/// The clear, silvered skirt a turret stands in (fixed: it is round).
fn skirt(b: &mut Builder<'_>, knob: NodeId, side: f64, c: f64, r: f64, k: f64) {
    b.add(
        knob,
        draw::circle(
            "cast shadow",
            c + 1.2 * k,
            c + 2.8 * k,
            r * 1.02,
            draw::radial(&[(0.0, "#000000a6"), (0.72, "#00000066"), (1.0, "#00000000")]),
        ),
    );
    let glass = draw::radial_at(
        0.5,
        0.5,
        0.5,
        &[
            (0.0, "#5a606aee"),
            (0.62, "#8d949ddd"),
            (0.86, "#c9ced4ee"),
            (0.95, "#eef0f2"),
            (1.0, "#7d838b"),
        ],
    );
    b.add(
        knob,
        draw::with_effects(
            draw::circle("skirt", c, c, r, glass),
            vec![draw::drop_shadow("#00000073", 0.0, 1.0 * k, 2.0 * k)],
        ),
    );
    b.add(
        knob,
        draw::path(
            "skirt light",
            0.0,
            0.0,
            side,
            side,
            ring(c, r * 0.64, r),
            draw::linear(
                150.0,
                &[(0.0, "#ffffff59"), (0.45, "#ffffff00"), (1.0, "#0000004d")],
            ),
        ),
    );
}

/// The turret itself, radius `rt`, in `parent` (a square of `side`
/// centred at `c`): the flutes, crown and index turn in the returned body.
fn turret(b: &mut Builder<'_>, parent: NodeId, side: f64, c: f64, rt: f64, k: f64) -> NodeId {
    b.add(
        parent,
        draw::with_effects(
            draw::circle("turret", c, c, rt, draw::solid("#2a2d32")),
            vec![draw::drop_shadow("#000000b3", 0.6 * k, 1.8 * k, 2.6 * k)],
        ),
    );
    let body = b.add(parent, draw::frame("body", 0.0, 0.0, side, side));
    b.add(
        body,
        draw::path(
            "flutes",
            0.0,
            0.0,
            side,
            side,
            knurl(c, rt * 0.8, rt, 22, 1.4 * k),
            draw::solid("#15171a"),
        ),
    );
    let crown = draw::circle(
        "crown",
        c,
        c,
        rt * 0.78,
        draw::radial_at(
            0.5,
            0.5,
            0.5,
            &[(0.0, "#9ba0a6"), (0.85, "#8d9298"), (1.0, "#6b7076")],
        ),
    );
    b.add(
        body,
        draw::with_effects(
            crown,
            vec![draw::drop_shadow("#00000066", 0.0, 0.0, 1.0 * k)],
        ),
    );
    b.add(body, bar(c, rt * 0.2, rt * 0.74, (rt * 0.12).max(1.2)));
    b.add(
        parent,
        draw::path(
            "flute light",
            0.0,
            0.0,
            side,
            side,
            ring(c, rt * 0.78, rt),
            draw::linear(
                155.0,
                &[(0.0, "#ffffff2e"), (0.5, "#ffffff00"), (1.0, "#00000040")],
            ),
        ),
    );
    b.add(
        parent,
        draw::circle(
            "crown light",
            c,
            c,
            rt * 0.78,
            draw::linear(
                160.0,
                &[(0.0, "#ffffff33"), (0.55, "#ffffff00"), (1.0, "#00000026")],
            ),
        ),
    );
    body
}

/// `(cap, wing)` for a coloured variant.
fn wing_colors(v: &str) -> (&'static str, &'static str) {
    if v == "blue" {
        ("#26467f", "#2c5190")
    } else {
        ("#86191d", "#9b2024")
    }
}

fn single(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(56.0).max(20.0);
    let r = d / 2.0;
    let k = d / 56.0;
    let ink = p.ink_color(INK);
    let scale = Scale::of(p, &["0", "", "", "", "", "5", "", "", "", "", "10"], 300.0);
    let size = (d * 0.17).max(draw::SCALE_TYPE_MIN);
    let dot_r = r + 6.0 * k;
    let f = open(
        b,
        parent,
        p,
        &scale,
        (Mark::Dot, dot_r, 3.2 * k),
        dot_r + 4.0 * k,
        size,
        r,
        &ink,
    );
    let (c, side, knob) = (f.c, f.side, f.knob);
    skirt(b, knob, side, c, r, k);
    let body = match p.variant.as_deref() {
        Some(v @ ("red" | "blue")) => {
            let (cap_c, wing_c) = wing_colors(v);
            let rc = r * 0.58;
            let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
            let wing = draw::polygon(&[
                (c - r * 0.26, c + r * 0.8),
                (c - r * 0.26, c - r * 0.88),
                (c - r * 0.14, c - r * 1.06),
                (c + r * 0.14, c - r * 1.06),
                (c + r * 0.26, c - r * 0.88),
                (c + r * 0.26, c + r * 0.8),
            ]);
            let wing = draw::path(
                "wing",
                0.0,
                0.0,
                side,
                side,
                wing,
                rad(
                    0.5,
                    0.5,
                    0.5,
                    &[(0.0, tint(wing_c, 0.12)), (1.0, shade(wing_c, 0.25))],
                ),
            );
            b.add(
                body,
                draw::with_effects(
                    wing,
                    vec![draw::drop_shadow("#000000b3", 0.0, 0.0, 2.2 * k)],
                ),
            );
            let cap = draw::circle(
                "cap",
                c,
                c,
                rc,
                rad(
                    0.5,
                    0.5,
                    0.5,
                    &[
                        (0.0, tint(cap_c, 0.1)),
                        (0.85, cap_c.to_string()),
                        (1.0, shade(cap_c, 0.35)),
                    ],
                ),
            );
            b.add(
                body,
                draw::with_effects(cap, vec![draw::drop_shadow("#00000080", 0.0, 0.0, 1.5 * k)]),
            );
            b.add(
                body,
                draw::with_opacity(bar(c, rc * 1.05, r * 0.98, (r * 0.07).max(1.2)), 0.9),
            );
            b.add(
                knob,
                draw::circle(
                    "cap light",
                    c,
                    c,
                    rc,
                    draw::linear(
                        155.0,
                        &[(0.0, "#ffffff47"), (0.5, "#ffffff00"), (1.0, "#00000040")],
                    ),
                ),
            );
            body
        }
        _ => turret(b, knob, side, c, r * 0.62, k),
    };
    close(b, p, &f, body, scale.sweep, d, &ink)
}

#[allow(clippy::too_many_lines)]
fn pair(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(72.0).max(28.0);
    let r = d / 2.0;
    let k = d / 72.0;
    let inner = p.inner.as_deref().cloned().unwrap_or_default();
    let ink = p.ink_color(INK);
    let scale = Scale::of(p, &["0", "", "", "", "", "5", "", "", "", "", "10"], 300.0);
    let size = (d * 0.14).max(draw::SCALE_TYPE_MIN);
    let dot_r = r + 6.0 * k;
    let f = open(
        b,
        parent,
        p,
        &scale,
        (Mark::Dot, dot_r, 3.2 * k),
        dot_r + 4.0 * k,
        size,
        r,
        &ink,
    );
    let (c, side, knob) = (f.c, f.side, f.knob);

    // ── the collar: turned aluminium, knurled at its rim ──
    b.add(
        knob,
        draw::circle(
            "cast shadow",
            c + 1.4 * k,
            c + 3.0 * k,
            r * 1.02,
            draw::radial(&[(0.0, "#000000a6"), (0.72, "#00000066"), (1.0, "#00000000")]),
        ),
    );
    let metal = draw::conic(
        0.0,
        &[
            (0.0, "#e9ebee"),
            (0.12, "#9ea3a9"),
            (0.3, "#f4f5f6"),
            (0.5, "#8d9298"),
            (0.7, "#e2e4e7"),
            (0.88, "#979ca2"),
            (1.0, "#e9ebee"),
        ],
    );
    b.add(
        knob,
        draw::with_effects(
            draw::circle("collar", c, c, r, metal),
            vec![draw::drop_shadow("#00000080", 0.0, 1.0 * k, 2.0 * k)],
        ),
    );
    let collar_body = b.add(knob, draw::frame("collar body", 0.0, 0.0, side, side));
    b.add(
        collar_body,
        draw::path(
            "knurl",
            0.0,
            0.0,
            side,
            side,
            knurl(c, r * 0.84, r, 60, 1.0 * k),
            draw::solid("#3b3f4459"),
        ),
    );
    b.add(
        collar_body,
        draw::rounded(
            draw::rect(
                "collar index",
                c - 1.1 * k,
                c - r * 0.84,
                2.2 * k,
                r * 0.2,
                draw::solid("#1c1e22"),
            ),
            1.1 * k,
        ),
    );
    b.add(
        knob,
        draw::path(
            "collar light",
            0.0,
            0.0,
            side,
            side,
            ring(c, r * 0.62, r),
            draw::linear(
                150.0,
                &[(0.0, "#ffffff40"), (0.5, "#ffffff00"), (1.0, "#0000004d")],
            ),
        ),
    );
    b.add(
        knob,
        draw::path(
            "collar step",
            0.0,
            0.0,
            side,
            side,
            ring(c, r * 0.6, r * 0.64),
            draw::solid("#00000040"),
        ),
    );

    // ── the turret in it: its own control ──
    let rt = r * 0.5;
    let bi = (2.0 * rt).max(inner.touch.unwrap_or(TOUCH_MIN)).min(side);
    let cap_root = b.add(
        f.root,
        draw::frame(
            &if inner.name.is_empty() {
                format!("{} cap", p.name)
            } else {
                inner.name.clone()
            },
            c - bi / 2.0,
            c - bi / 2.0,
            bi,
            bi,
        ),
    );
    let dial = b.add(cap_root, draw::frame("dial", 0.0, 0.0, bi, bi));
    let cap_body = turret(b, dial, bi, bi / 2.0, rt, k);

    // ── legends ──
    let legend_size = (9.0 * k).max(8.0);
    let legend = b.add(
        f.root,
        caption(
            "legend",
            p.legend.as_deref().unwrap_or(""),
            c,
            f.legend_y,
            legend_size,
            &ink,
            p.case(),
        ),
    );
    let inner_ink = inner.ink_color(&ink);
    let inner_y = f.legend_y + legend_size * 1.35 - (c - bi / 2.0);
    let inner_legend = b.add(
        cap_root,
        caption(
            "inner legend",
            inner.legend.as_deref().unwrap_or(""),
            bi / 2.0,
            inner_y,
            legend_size * 0.9,
            &inner_ink,
            inner.case(),
        ),
    );
    if let Some(n) = b.scene.node_mut(f.root) {
        n.size.height = n.size.height.max(f.legend_y + legend_size * 2.4);
    }

    let outer = p
        .control(ControlKind::Knob {
            sweep_degrees: scale.sweep,
        })
        .slot(SlotRole::Needle, collar_body.0)
        .slot(SlotRole::Label, legend.0);
    b.controls.insert(f.root, outer);
    let cap = inner
        .control(ControlKind::Knob {
            sweep_degrees: inner.sweep.unwrap_or(300.0),
        })
        .slot(SlotRole::Needle, cap_body.0)
        .slot(SlotRole::Label, inner_legend.0);
    b.controls.insert(cap_root, cap);
    if let Some(t) = p.touch {
        crate::registry::tighten(b, f.root, (c, c), d, t);
    }
    f.root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "knob.turret",
            family: Family::Knob,
            title: "Turret",
            description: "A class-A console module's knob: a small dark fluted turret with a flat grey crown and a white line, standing in a clear silvered skirt, inside a dotted scale (`grey`). `red` / `blue`: a coloured cap with a flat pointer wing through it — the module's gain and high-pass selectors.",
            variants: &["grey", "red", "blue"],
            default_size: 56.0,
            binds: "param",
            build: single,
        },
        ComponentDef {
            id: "knob.turret_pair",
            family: Family::Knob,
            title: "Turret pair",
            description: "A class-A console module's concentric pair: a knurled, turned-aluminium collar with a dark index (the outer control — the stepped band frequency, printed with dots) and the grey fluted turret in it (the inner control via `Place::inner` — the band's gain). Size = the collar's diameter.",
            variants: &["silver"],
            default_size: 72.0,
            binds: "param",
            build: pair,
        },
    ]
}
