//! `panel.stompbox`: a die-cast pedal enclosure seen from above, painted.
//!
//! A rounded lid in one colour with the finish the paint has (`metallic`:
//! a fine flake and a bright sheen; `satin`: a soft, chalky powder coat),
//! a thin darker cast edge showing round it, and a hint of the enclosure's
//! front wall under the bottom edge. No control — a parent for the pedal's
//! knobs, LEDs and footswitch, which read on it at any size.

use crate::document::proto::NodeId;
use crate::surface::widgets::Builder;

use crate::surface::kit::controls::parts::{alpha, lin, shade, tint};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

fn stompbox(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p
        .opt("w")
        .and_then(|v| v.parse().ok())
        .or(p.size)
        .unwrap_or(156.0);
    let h = p.opt("h").and_then(|v| v.parse().ok()).unwrap_or(286.0);
    let color = p
        .opt("color")
        .filter(|c| c.starts_with('#'))
        .unwrap_or("#c85a86")
        .to_string();
    let r = p
        .opt("radius")
        .and_then(|v| v.parse().ok())
        .unwrap_or(w * 0.07);
    let satin = p.variant.as_deref() == Some("satin");
    let name = if p.name.is_empty() {
        "Stompbox"
    } else {
        p.name.as_str()
    };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, w, h));
    let wall = (h * 0.035).max(3.0);
    // The front wall showing under the lid, and the cast edge round it.
    let front = draw::rect(
        "front wall",
        0.0,
        0.0,
        w,
        h,
        lin(
            180.0,
            &[
                (0.0, shade(&color, 0.45)),
                (0.9, shade(&color, 0.55)),
                (1.0, shade(&color, 0.75)),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            draw::rounded(front, r),
            vec![draw::drop_shadow("#00000073", 0.0, 3.0, 8.0)],
        ),
    );
    let edge = 1.4;
    let lid_stops = if satin {
        vec![
            (0.0, tint(&color, 0.16)),
            (0.5, color.clone()),
            (1.0, shade(&color, 0.1)),
        ]
    } else {
        vec![
            (0.0, tint(&color, 0.22)),
            (0.45, color.clone()),
            (1.0, shade(&color, 0.22)),
        ]
    };
    let mut lid = draw::rect(
        "lid",
        edge,
        edge,
        w - 2.0 * edge,
        h - wall - edge,
        lin(168.0, &lid_stops),
    );
    let sheen = if satin {
        draw::radial_at(0.3, 0.12, 0.9, &[(0.0, "#ffffff26"), (1.0, "#ffffff00")])
    } else {
        draw::linear(
            120.0,
            &[
                (0.0, "#ffffff00"),
                (0.35, "#ffffff00"),
                (0.47, "#ffffff2e"),
                (0.6, "#ffffff00"),
                (1.0, "#ffffff00"),
            ],
        )
    };
    lid.fills.push(sheen);
    let lid = draw::rounded(lid, (r - edge).max(1.0));
    let lid = draw::stroked(lid, &alpha(&shade(&color, 0.4), 0.8), 1.0);
    let lid = draw::with_effects(
        lid,
        vec![
            draw::inner_shadow("#ffffff4d", 0.0, 1.2, 0.8),
            draw::inner_shadow("#00000033", 0.0, -2.0, 4.0),
        ],
    );
    let lid = b.add(root, lid);
    if !satin {
        // Metal flake: a sparse speckle of light and dark.
        let (lw, lh) = (w - 2.0 * edge, h - wall - edge);
        let mut light = String::new();
        let mut dark = String::new();
        let mut seed: u32 = 0x9e37_79b9;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            f64::from(seed) / f64::from(u32::MAX)
        };
        let count = (lw * lh / 55.0) as usize;
        for i in 0..count {
            let (x, y) = (
                r * 0.3 + next() * (lw - r * 0.6),
                r * 0.3 + next() * (lh - r * 0.6),
            );
            let s = 0.35 + next() * 0.3;
            let d = format!("M {:.2} {:.2} h {s:.2} v {s:.2} h -{s:.2} Z ", x, y);
            if i % 2 == 0 {
                light.push_str(&d)
            } else {
                dark.push_str(&d)
            }
        }
        b.add(
            lid,
            draw::path("flake", 0.0, 0.0, lw, lh, light, draw::solid("#ffffff30")),
        );
        b.add(
            lid,
            draw::path("flake", 0.0, 0.0, lw, lh, dark, draw::solid("#0000001f")),
        );
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "panel.stompbox",
        family: Family::Panel,
        title: "Stompbox enclosure",
        description: "A painted die-cast pedal enclosure seen from above: a rounded lid in one colour (`color=#hex`) over a sliver of its darker front wall. `metallic` for a flake finish with a bright sheen (candy-coloured pedals), `satin` for a soft powder coat (pastel boutique pedals). No control — a parent for knobs, LEDs and the footswitch. `options`: `w=`, `h=` (default 156 × 286, the standard enclosure at 60 px/in), `radius=`.",
        variants: &["metallic", "satin"],
        default_size: 156.0,
        binds: "none",
        build: stompbox,
    }]
}
