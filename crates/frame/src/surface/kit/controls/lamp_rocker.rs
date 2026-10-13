//! `switch.lamp_rocker`: an illuminated power rocker, straight on — a
//! black bezel round a translucent coloured rocker that glows from inside
//! while on (two neon dots and the lens's own light), dark red-brown when
//! off. An amp head's POWER switch.
//!
//! ```text
//! <root control=Button{Toggle}>
//!   bezel, dark rocker, its moulding
//!   glow ← Led (the lit rocker and its neon dots; emissive)
//!   legend ← Label (under it)
//! ```
//!
//! Size = the bezel's height (width 0.7 × it). Variants: `red`, `amber`,
//! `green`.

use crate::document::proto::{NodeId, TextAlign};
use crate::surface::proto::{ButtonMode, ControlKind, SlotRole};
use crate::surface::widgets::Builder;

use super::parts::{PANEL_INK, cased, ink as parts_ink, shade, tint};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

fn lamp_rocker(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let h = p.size.unwrap_or(60.0).max(20.0);
    let k = h / 60.0;
    let w = h * 0.7;
    let color = match p.variant.as_deref() {
        Some("amber") => "#ff9a1f",
        Some("green") => "#45e36a",
        _ => "#ff2a1f",
    };
    let bw = w.max(p.touch.unwrap_or(0.0));
    let bh = h.max(p.touch.unwrap_or(0.0));
    let legend = p.legend.clone().filter(|l| !l.is_empty());
    let size = (8.0 * k).max(7.0);
    let cap = if legend.is_some() { size * 1.8 } else { 0.0 };
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, bw, bh + cap));
    let (x0, y0) = ((bw - w) / 2.0, (bh - h) / 2.0);
    let bezel = draw::rounded(
        draw::rect(
            "bezel",
            x0,
            y0,
            w,
            h,
            draw::linear(
                180.0,
                &[(0.0, "#3a3b3e"), (0.5, "#151517"), (1.0, "#050506")],
            ),
        ),
        4.0 * k,
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(bezel, "#000000", k.max(0.8)),
            vec![draw::drop_shadow("#000000a0", 0.0, 2.0 * k, 4.0 * k)],
        ),
    );
    let inset = 6.0 * k;
    let (rx, ry, rw, rh) = (x0 + inset, y0 + inset, w - 2.0 * inset, h - 2.0 * inset);
    let dark = draw::rect(
        "rocker",
        rx,
        ry,
        rw,
        rh,
        draw::linear(
            180.0,
            &[
                (0.0, &shade(color, 0.55)),
                (0.45, &shade(color, 0.7)),
                (1.0, &shade(color, 0.85)),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            draw::rounded(dark, 2.0 * k),
            vec![draw::inner_shadow("#000000c0", 0.0, 1.0 * k, 3.0 * k)],
        ),
    );
    // The lit rocker: its upper face brighter, the neon dots hot.
    let glow = b.add(root, draw::frame("glow", rx, ry, rw, rh));
    let lit = draw::rect(
        "lit",
        0.0,
        0.0,
        rw,
        rh,
        draw::linear(
            180.0,
            &[
                (0.0, &tint(color, 0.15)),
                (0.42, color),
                (0.5, &shade(color, 0.2)),
                (1.0, &shade(color, 0.35)),
            ],
        ),
    );
    b.add(
        glow,
        draw::emissive(draw::rounded(lit, 2.0 * k), draw::hex(color), 0.8, h * 0.5),
    );
    for dx in [0.3, 0.7] {
        let dot = draw::circle(
            "neon",
            rw * dx,
            rh * 0.36,
            2.2 * k,
            draw::radial(&[
                (0.0, "#ffffff"),
                (0.5, &tint(color, 0.6)),
                (1.0, &tint(color, 0.2)),
            ]),
        );
        b.add(glow, draw::emissive(dot, draw::hex(color), 1.0, 6.0 * k));
    }
    b.add(
        root,
        draw::rect(
            "moulding",
            rx + 1.0,
            ry + rh * 0.47,
            rw - 2.0,
            1.2 * k,
            draw::solid("#00000066"),
        ),
    );
    b.add(
        root,
        draw::rect(
            "sheen",
            rx + 1.5 * k,
            ry + 1.5 * k,
            rw - 3.0 * k,
            rh * 0.18,
            draw::linear(180.0, &[(0.0, "#ffffff40"), (1.0, "#ffffff00")]),
        ),
    );
    let mut c = p
        .control(ControlKind::Button {
            mode: ButtonMode::Toggle,
        })
        .slot(SlotRole::Led, glow.0);
    if let Some(l) = legend {
        let inkc = p.ink_color(PANEL_INK);
        let t = b.add(
            root,
            cased(
                parts_ink(
                    "legend",
                    &l,
                    -bw,
                    bh + cap * 0.5,
                    3.0 * bw,
                    size,
                    &inkc,
                    TextAlign::Center,
                    700.0,
                ),
                p.case(),
            ),
        );
        c = c.slot(SlotRole::Label, t.0);
    }
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "switch.lamp_rocker",
        family: Family::Switch,
        title: "Illuminated rocker",
        description: "An amp head's illuminated power rocker, straight on: a black bezel round a translucent rocker (`red`, `amber`, `green`) that glows from inside with two neon dots while the toggle is on, dark when off. Legend under it. Size = bezel height (width 0.7 × it).",
        variants: &["red", "amber", "green"],
        default_size: 60.0,
        binds: "toggle param",
        build: lamp_rocker,
    }]
}
