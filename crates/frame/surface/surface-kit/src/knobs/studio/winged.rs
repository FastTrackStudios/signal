//! `knob.winged_pair`: the concentric pair of a proportional-Q console EQ
//! module, drawn from photographs — a cream moulded knob with a pointer
//! wing (the outer control: boost/cut, its numerals printed in white round
//! it) and, sitting in it, a three-lobed blue ring round a flat grey face
//! (the inner control: the stepped frequency, printed in blue on a wider
//! ring outside the gain's).
//!
//! ```text
//! <root control=Knob (outer)>          side² (tight: the wing's disc, ≥ touch)
//!   knob   (square, centred on the shaft)
//!     outer numerals, inner numerals                      (fixed)
//!     cast shadow                                          (fixed)
//!     wing body  ← outer Needle: cream disc + pointer wing
//!     wing light                                           (fixed)
//!   <cap control=Knob (inner)>         the blue ring's box, in the outer's
//!     dial (square, centred on the shaft)
//!       lobe body ← inner Needle: blue lobes, notch
//!       face, lobe light                                   (fixed)
//!     inner legend ← inner Label
//!   legend ← outer Label
//! ```

use document_proto::NodeId;
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{Mark, Scale, caption};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place, TOUCH_MIN};

const INK: &str = "#f1f1ee";
const BLUE_INK: &str = "#3fb1ec";

/// Radius of the blue ring, as a fraction of the wing's disc.
const LOBES: f64 = 0.64;

fn winged_pair(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(64.0).max(24.0);
    let r = d / 2.0;
    let k = d / 64.0;
    let inner = p.inner.as_deref().cloned().unwrap_or_default();
    let ink = p.ink_color(INK);
    let inner_ink = inner.ink_color(BLUE_INK);
    let outer_scale = Scale::of(
        p,
        &["-12", "", "", "", "", "0", "", "", "", "", "+12"],
        300.0,
    );
    let inner_scale = Scale::of(&inner, &[], 300.0);
    let size = (d * 0.17).max(draw::SCALE_TYPE_MIN);
    let (clear_o, clear_i) = (r * 1.3, r * 1.3 + size * 2.0);
    let (ro, bo) = outer_scale.reach(clear_o, size);
    let (ri, bi_) = inner_scale.reach(clear_i, size * 1.05);
    let half = (r * 1.34 + 4.0 * k).max(ro + 3.0 * k).max(ri + 3.0 * k);
    let side = 2.0 * half;
    let c = half;
    let legend_size = (9.0 * k).max(8.0);
    let legend_y = c + bo.max(bi_).max(r * 1.1) + 4.0 * k + legend_size * 0.6;
    let root = b.add(
        parent,
        draw::frame(
            &p.name,
            0.0,
            0.0,
            side,
            side.max(legend_y + 2.0 * legend_size),
        ),
    );
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));
    outer_scale.print(
        b,
        knob,
        side,
        c,
        (Mark::Bare, 0.0, 0.0),
        clear_o,
        size,
        &ink,
    );
    inner_scale.print(
        b,
        knob,
        side,
        c,
        (Mark::Bare, 0.0, 0.0),
        clear_i,
        size * 1.05,
        &inner_ink,
    );

    // ── the wing: cream moulding, turning; the room's light over it ──
    b.add(
        knob,
        draw::circle(
            "cast shadow",
            c + 1.5 * k,
            c + 3.0 * k,
            r * 1.02,
            draw::radial(&[(0.0, "#000000a6"), (0.72, "#00000073"), (1.0, "#00000000")]),
        ),
    );
    let body = b.add(knob, draw::frame("wing body", 0.0, 0.0, side, side));
    let cream = draw::radial_at(
        0.5,
        0.5,
        0.5,
        &[(0.0, "#f3f1ea"), (0.8, "#e6e3d9"), (1.0, "#bdb9ad")],
    );
    let wing = draw::polygon(&[
        (c - r * 0.3, c - r * 0.8),
        (c - r * 0.17, c - r * 1.26),
        (c + r * 0.17, c - r * 1.26),
        (c + r * 0.3, c - r * 0.8),
    ]);
    let wing = draw::with_effects(
        draw::path("wing", 0.0, 0.0, side, side, wing, cream.clone()),
        vec![draw::drop_shadow("#00000080", 0.0, 0.0, 2.0 * k)],
    );
    b.add(body, wing);
    b.add(
        body,
        draw::circle("wing tip", c, c - r * 1.2, r * 0.17, cream.clone()),
    );
    let disc = draw::with_effects(
        draw::circle("disc", c, c, r, cream),
        vec![draw::drop_shadow("#00000080", 0.0, 0.0, 2.0 * k)],
    );
    b.add(body, disc);
    // The moulding's three shoulders, under the blue lobes.
    b.add(
        body,
        draw::path(
            "shoulders",
            0.0,
            0.0,
            side,
            side,
            draw::lobed(c, c, r * 0.9, 3, r * 0.12),
            draw::solid("#0000000f"),
        ),
    );
    b.add(
        knob,
        draw::circle(
            "wing light",
            c,
            c,
            r,
            draw::linear(
                155.0,
                &[(0.0, "#ffffff40"), (0.5, "#ffffff00"), (1.0, "#0000003d")],
            ),
        ),
    );

    // ── the cap: its own control, nested in the wing's ──
    let rl = r * LOBES;
    let dl = 2.0 * (rl + r * 0.06);
    let bi = dl.max(inner.touch.unwrap_or(TOUCH_MIN)).min(side);
    let cap_root = b.add(
        root,
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
    let cc = bi / 2.0;
    let lobe_body = b.add(dial, draw::frame("lobe body", 0.0, 0.0, bi, bi));
    let blue = draw::radial_at(
        0.5,
        0.5,
        0.5,
        &[(0.0, "#3c6fd0"), (0.8, "#2c5cb8"), (1.0, "#1d3f86")],
    );
    let lobes = draw::with_effects(
        draw::path(
            "lobes",
            0.0,
            0.0,
            bi,
            bi,
            draw::lobed(cc, cc, rl + r * 0.06, 3, r * 0.14),
            blue,
        ),
        vec![draw::drop_shadow("#00000073", 0.0, 0.0, 1.5 * k)],
    );
    b.add(lobe_body, lobes);
    // The index: a notch cut into the top lobe.
    let notch = draw::polygon(&[
        (cc - r * 0.07, cc - rl - r * 0.07),
        (cc + r * 0.07, cc - rl - r * 0.07),
        (cc, cc - rl + r * 0.12),
    ]);
    b.add(
        lobe_body,
        draw::path("notch", 0.0, 0.0, bi, bi, notch, draw::solid("#e9e6dc")),
    );
    b.add(
        dial,
        draw::path(
            "lobe light",
            0.0,
            0.0,
            bi,
            bi,
            draw::lobed(cc, cc, rl + r * 0.06, 3, r * 0.14),
            draw::linear(
                155.0,
                &[(0.0, "#ffffff3d"), (0.5, "#ffffff00"), (1.0, "#00000033")],
            ),
        ),
    );
    let face = draw::circle(
        "face",
        cc,
        cc,
        r * 0.44,
        draw::linear(
            165.0,
            &[(0.0, "#dcdedf"), (0.6, "#c4c6c8"), (1.0, "#a9abad")],
        ),
    );
    b.add(
        dial,
        draw::with_effects(
            draw::stroked(face, "#8a8c8e", (0.8 * k).max(0.5)),
            vec![draw::drop_shadow("#00000059", 0.0, 1.0 * k, 2.0 * k)],
        ),
    );

    // ── legends: the wing's, then the cap's under it ──
    let legend = b.add(
        root,
        caption(
            "legend",
            p.legend.as_deref().unwrap_or(""),
            c,
            legend_y,
            legend_size,
            &ink,
            p.case(),
        ),
    );
    let inner_y = legend_y + legend_size * 1.35 - (c - bi / 2.0);
    let inner_legend = b.add(
        cap_root,
        caption(
            "inner legend",
            inner.legend.as_deref().unwrap_or(""),
            cc,
            inner_y,
            legend_size * 0.9,
            &inner_ink,
            inner.case(),
        ),
    );

    let outer = p
        .control(ControlKind::Knob {
            sweep_degrees: outer_scale.sweep,
        })
        .slot(SlotRole::Needle, body.0)
        .slot(SlotRole::Label, legend.0);
    b.controls.insert(root, outer);
    let cap = inner
        .control(ControlKind::Knob {
            sweep_degrees: inner_scale.sweep,
        })
        .slot(SlotRole::Needle, lobe_body.0)
        .slot(SlotRole::Label, inner_legend.0);
    b.controls.insert(cap_root, cap);
    if let Some(t) = p.touch {
        crate::registry::tighten(b, root, (c, c), d, t);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.winged_pair",
        family: Family::Knob,
        title: "Winged concentric pair",
        description: "A proportional-Q EQ module's concentric pair: a cream moulded knob with a pointer wing (outer control, boost/cut, numerals printed in white) and a three-lobed blue ring round a flat grey face sitting in it (inner control via `Place::inner`, the stepped frequency, printed in blue on a wider ring). For black 500-series style EQ panels. Size = the wing's disc.",
        variants: &["cream"],
        default_size: 64.0,
        binds: "param",
        build: winged_pair,
    }]
}
