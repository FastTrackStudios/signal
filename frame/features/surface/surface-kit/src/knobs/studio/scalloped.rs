//! `knob.scalloped`: the black phenolic knob of the 1950s passive program
//! equalizers, drawn from photographs — a wide skirt with nine soft
//! scallops, a raised round boss with a turned groove, a glossy crown, and
//! a thin cream index at the skirt's edge. The panel prints bare numerals
//! round it (no ticks).
//!
//! ```text
//! knob
//!   numerals                       (fixed, panel print)
//!   cast shadow                    (fixed)
//!   body ← Needle: scalloped skirt (radial fill), cream index
//!   skirt light                    (fixed)
//!   boss, groove, crown, crown light (fixed: round, so they read the same turned)
//! ```

use document_proto::NodeId;
use surface_widgets::Builder;

use super::parts::{Mark, Scale, bar, close, open, ring};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

const INK: &str = "#ebe6d6";
const ZERO_TEN: [&str; 11] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"];

fn scalloped(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(72.0).max(24.0);
    let r = d / 2.0;
    let k = d / 72.0;
    let ink = p.ink_color(INK);
    let scale = Scale::of(p, &ZERO_TEN, 300.0);
    let size = (d * 0.15).max(draw::SCALE_TYPE_MIN);
    let f = open(b, parent, p, &scale, (Mark::Bare, 0.0, 0.0), r + 4.0 * k, size, r, &ink);
    let (c, side, knob) = (f.c, f.side, f.knob);

    b.add(knob, draw::circle("cast shadow", c + 1.5 * k, c + 3.5 * k, r, draw::radial(&[(0.0, "#00000099"), (0.75, "#00000066"), (1.0, "#00000000")])));
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let skirt = draw::path(
        "skirt",
        0.0,
        0.0,
        side,
        side,
        draw::lobed(c, c, r, 9, r * 0.1),
        draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#232325"), (0.62, "#161617"), (0.9, "#0d0d0e"), (1.0, "#050506")]),
    );
    b.add(body, draw::with_effects(skirt, vec![draw::drop_shadow("#000000a6", 0.0, 0.0, 2.0 * k)]));
    b.add(body, draw::with_opacity(bar(c, r * 0.7, r * 0.95, (2.2 * k).max(1.2)), 0.92));
    let light = draw::linear(155.0, &[(0.0, "#ffffff30"), (0.45, "#ffffff08"), (0.6, "#00000000"), (1.0, "#00000059")]);
    b.add(knob, draw::circle("skirt light", c, c, r * 0.9, light));

    // The boss: a turned cylinder standing out of the skirt, its crown glossy.
    let boss = draw::circle("boss", c, c, r * 0.64, draw::linear(165.0, &[(0.0, "#3b3b3e"), (0.5, "#161618"), (1.0, "#050506")]));
    b.add(knob, draw::with_effects(draw::stroked(boss, "#000000cc", (0.9 * k).max(0.6)), vec![draw::drop_shadow("#000000b3", 0.8 * k, 2.0 * k, 3.5 * k)]));
    b.add(knob, draw::path("groove", 0.0, 0.0, side, side, ring(c, r * 0.49, r * 0.535), draw::solid("#000000c0")));
    b.add(knob, draw::path("groove light", 0.0, 0.0, side, side, ring(c, r * 0.535, r * 0.56), draw::solid("#ffffff1f")));
    let crown = draw::circle("crown", c, c, r * 0.47, draw::radial_at(0.4, 0.32, 0.85, &[(0.0, "#3d3d41"), (0.45, "#18181a"), (1.0, "#070708")]));
    b.add(knob, crown);
    b.add(knob, draw::circle("crown light", c - r * 0.14, c - r * 0.2, r * 0.2, draw::radial(&[(0.0, "#ffffff4d"), (1.0, "#ffffff00")])));
    close(b, p, &f, body, scale.sweep, d, &ink)
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.scalloped",
        family: Family::Knob,
        title: "Scalloped phenolic",
        description: "Black phenolic knob of a vintage passive program equalizer: a wide skirt with nine soft scallops, a raised round boss with a turned groove and a glossy crown, a thin cream index at the skirt's edge; the panel prints bare numerals 0–10 round it. For boost/cut and bandwidth on blue-grey valve-era panels.",
        variants: &["black"],
        default_size: 72.0,
        binds: "param",
        build: scalloped,
    }]
}
