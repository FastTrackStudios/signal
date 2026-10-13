//! `knob.console_cap`: the small pot of a large-format console's channel
//! EQ, drawn from photographs — a dark grey collet skirt with a fluted rim,
//! a flat, matte coloured cap filling most of it, and one broad index bar
//! from the middle of the cap to its edge; a dot at every position of the
//! panel's scale with the numerals beside them.
//!
//! ```text
//! knob
//!   scale marks (dots), numerals   (fixed, panel print)
//!   cast shadow                    (fixed)
//!   body ← Needle: skirt + flutes, cap, index (all round the shaft)
//!   skirt light, cap light         (fixed)
//! ```

use document_proto::NodeId;
use surface_widgets::Builder;

use super::parts::{Mark, Scale, bar, close, knurl, open, ring};
use crate::controls::parts::{rad, shade, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

const INK: &str = "#eceae4";

/// `(cap, index)` for a variant.
fn colors(v: Option<&str>) -> (&'static str, &'static str) {
    match v {
        Some("red") => ("#d83b2d", "#fbf6f2"),
        Some("green") => ("#2d7a45", "#f4f7f2"),
        Some("blue") => ("#3b7cc7", "#f3f6fa"),
        Some("brown") => ("#6e4029", "#f6efe8"),
        Some("grey") => ("#cdcac1", "#18191b"),
        Some("yellow") => ("#e0b93a", "#1b1b1b"),
        _ => ("#1d1d1f", "#f2f2f0"),
    }
}

fn console_cap(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(44.0).max(20.0);
    let r = d / 2.0;
    let k = d / 44.0;
    let ink = p.ink_color(INK);
    let (cap_c, index_c) = colors(p.variant.as_deref());
    let scale = Scale::of(p, &["0", "", "", "", "", "5", "", "", "", "", "10"], 300.0);
    let size = (d * 0.2).max(draw::SCALE_TYPE_MIN);
    let dot_r = r + 5.0 * k;
    let f = open(b, parent, p, &scale, (Mark::Dot, dot_r, 2.8 * k), dot_r + 3.5 * k, size, r, &ink);
    let (c, side, knob) = (f.c, f.side, f.knob);
    let rc = r * 0.77;

    b.add(knob, draw::circle("cast shadow", c + 1.2 * k, c + 2.6 * k, r * 1.02, draw::radial(&[(0.0, "#000000a6"), (0.7, "#00000070"), (1.0, "#00000000")])));
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let skirt = draw::circle("skirt", c, c, r, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#5b5c60"), (0.8, "#4a4b4f"), (1.0, "#2b2c2f")]));
    b.add(body, draw::with_effects(skirt, vec![draw::drop_shadow("#00000080", 0.0, 0.0, 1.5 * k)]));
    b.add(body, draw::path("flutes", 0.0, 0.0, side, side, knurl(c, rc + 1.2 * k, r - 0.4 * k, 30, 1.1 * k), draw::solid("#0000004d")));
    let cap = draw::circle("cap", c, c, rc, rad(0.5, 0.5, 0.5, &[(0.0, tint(cap_c, 0.05)), (0.85, cap_c.to_string()), (1.0, shade(cap_c, 0.18))]));
    b.add(body, draw::with_effects(cap, vec![draw::drop_shadow("#00000059", 0.0, 0.0, 1.0 * k)]));
    let mut index = bar(c, rc * 0.12, rc * 0.93, (rc * 0.2).max(1.6));
    index.fills = vec![draw::solid(index_c)];
    b.add(body, index);

    b.add(knob, draw::path("skirt light", 0.0, 0.0, side, side, ring(c, rc, r), draw::linear(160.0, &[(0.0, "#ffffff40"), (0.5, "#ffffff00"), (1.0, "#00000040")])));
    b.add(knob, draw::circle("cap light", c, c, rc, draw::linear(160.0, &[(0.0, "#ffffff33"), (0.5, "#ffffff00"), (1.0, "#0000002e")])));
    close(b, p, &f, body, scale.sweep, d, &ink)
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.console_cap",
        family: Family::Knob,
        title: "Console cap",
        description: "A large-format console's channel-EQ pot, seen from above: a dark grey collet skirt with a fluted rim, a flat matte coloured cap and one broad index bar, inside a dotted scale with numerals. Cap colours code the bands (red HF, green HMF, blue LMF, black or brown LF, grey filters).",
        variants: &["black", "red", "green", "blue", "brown", "grey", "yellow"],
        default_size: 44.0,
        binds: "param",
        build: console_cap,
    }]
}
