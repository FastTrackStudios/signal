//! `switch.stomp_flat`: a modern pedal's stomp switch with a plain cap —
//! a big rounded hex nut and a flat-topped satin cylinder standing out of
//! it, seen from above and a little in front (its side shows under the
//! top) — with its status LED wherever the pedal puts it.
//!
//! ```text
//! <root control=Footswitch>   box round the switch and its LED
//!   led socket, unlit lens, lit lens ← Led     (lit while the param is on)
//!   shadow, nut, cap side, cap top, light
//!   legend ← Label (under the switch)
//! ```
//!
//! Size = the nut's width across the flats. Options: `color=#hex` LED colour
//! (variant `none` = no LED); `led_dx=`, `led_dy=` where the LED sits from
//! the switch's centre (default above); `led_size=`; `momentary`.

use crate::document::proto::{NodeId, TextAlign};
use crate::surface::proto::{ButtonMode, ControlKind, SlotRole};
use crate::surface::widgets::Builder;

use super::parts::{PANEL_INK, cased, ink as parts_ink, shade, tint};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

fn num(p: &Place, key: &str) -> Option<f64> {
    p.opt(key).and_then(|v| v.parse().ok())
}

/// A hexagon with its corners rounded by `round` (`rot` 0 = a point up).
fn rounded_hex(cx: f64, cy: f64, r: f64, round: f64, rot: f64) -> String {
    let pts: Vec<(f64, f64)> = (0..6)
        .map(|i| draw::polar(cx, cy, r, rot + 60.0 * f64::from(i)))
        .collect();
    let t = (round / r).clamp(0.0, 0.45);
    let mut d = String::new();
    for i in 0..6 {
        let (p0, p1, p2) = (pts[(i + 5) % 6], pts[i], pts[(i + 1) % 6]);
        let a = (p1.0 + (p0.0 - p1.0) * t, p1.1 + (p0.1 - p1.1) * t);
        let q = (p1.0 + (p2.0 - p1.0) * t, p1.1 + (p2.1 - p1.1) * t);
        d.push_str(&format!(
            "{} {:.3} {:.3} Q {:.3} {:.3} {:.3} {:.3} ",
            if i == 0 { "M" } else { "L" },
            a.0,
            a.1,
            p1.0,
            p1.1,
            q.0,
            q.1
        ));
    }
    d.push('Z');
    d
}

#[allow(clippy::too_many_lines)]
fn stomp_flat(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(16.0);
    let k = d / 40.0;
    let led = match p.variant.as_deref() {
        Some("none") => None,
        Some("red") => Some("#ff3b2f".to_string()),
        Some("green") => Some("#3dff6a".to_string()),
        Some("blue") => Some("#6fb8ff".to_string()),
        _ => Some("#c69bff".to_string()),
    };
    let led = led.map(|c| p.lamp_color(0).unwrap_or(c));
    let (ldx, ldy) = (
        num(p, "led_dx").unwrap_or(0.0),
        num(p, "led_dy").unwrap_or(-1.2 * d),
    );
    let led_d = num(p, "led_size").unwrap_or(d * 0.14);
    let led_r = led_d / 2.0 + 2.0;
    let r = d / 2.0;
    let touch = p.touch.unwrap_or(0.0).max(d);
    let (mut x0, mut y0, mut x1, mut y1) = (
        -touch / 2.0,
        -touch / 2.0,
        touch / 2.0,
        touch / 2.0 + 3.0 * k,
    );
    if led.is_some() {
        x0 = x0.min(ldx - led_r - 2.0);
        x1 = x1.max(ldx + led_r + 2.0);
        y0 = y0.min(ldy - led_r - 2.0);
        y1 = y1.max(ldy + led_r + 2.0);
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let (cx, cy) = (-x0, -y0);
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let mut c = p.control(if p.flag("momentary") {
        ControlKind::Button {
            mode: ButtonMode::Momentary,
        }
    } else {
        ControlKind::footswitch()
    });

    if let Some(color) = led.as_deref() {
        let (lx, ly) = (cx + ldx, cy + ldy);
        let lr = led_d / 2.0;
        b.add(
            root,
            draw::circle("led socket", lx, ly, lr + 1.3, draw::solid("#00000073")),
        );
        b.add(
            root,
            draw::circle(
                "lens",
                lx,
                ly,
                lr,
                draw::radial_at(
                    0.4,
                    0.34,
                    0.9,
                    &[
                        (0.0, "#ffffff30"),
                        (0.5, &shade(color, 0.7)),
                        (1.0, "#000000e0"),
                    ],
                ),
            ),
        );
        let hot = draw::circle(
            "lamp",
            lx,
            ly,
            lr,
            draw::radial_at(
                0.45,
                0.42,
                0.75,
                &[
                    (0.0, "#ffffff"),
                    (0.4, &tint(color, 0.55)),
                    (0.8, color),
                    (1.0, &shade(color, 0.3)),
                ],
            ),
        );
        let lit = b.add(
            root,
            draw::emissive(hot, draw::hex(color), 1.0, led_d * 2.4),
        );
        c = c.slot(SlotRole::Led, lit.0);
    }

    // Shadow, the rounded hex nut (flats left and right), the cap.
    b.add(
        root,
        draw::circle(
            "shadow",
            cx + 0.8 * k,
            cy + 6.0 * k,
            r * 1.05,
            draw::radial(&[(0.0, "#000000c0"), (0.72, "#00000070"), (1.0, "#00000000")]),
        ),
    );
    let nut = draw::path(
        "nut",
        0.0,
        0.0,
        w,
        h,
        rounded_hex(cx, cy + 1.5 * k, r / 30f64.to_radians().cos(), r * 0.3, 0.0),
        draw::linear(
            165.0,
            &[
                (0.0, "#f4f4f2"),
                (0.35, "#b5b6b4"),
                (0.55, "#7f807e"),
                (0.75, "#cfd0cd"),
                (1.0, "#8d8e8b"),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(nut, "#3e3e40", (0.7 * k).max(0.5)),
            vec![draw::inner_shadow("#ffffff66", 0.0, 1.0 * k, 1.0 * k)],
        ),
    );
    let rc = r * 0.66;
    let side_h = r * 0.34;
    // The cap's side: a satin cylinder under its top.
    let mut pts: Vec<(f64, f64)> = (0..=24)
        .map(|i| {
            draw::polar(
                cx,
                cy - side_h * 0.5,
                rc,
                270.0 - 180.0 * f64::from(i) / 24.0,
            )
        })
        .collect();
    pts.extend((0..=24).map(|i| {
        let (x, y) = draw::polar(
            cx,
            cy - side_h * 0.5,
            rc,
            90.0 + 180.0 * f64::from(i) / 24.0,
        );
        (x, y + side_h)
    }));
    b.add(
        root,
        draw::path(
            "cap side",
            0.0,
            0.0,
            w,
            h,
            draw::polygon(&pts),
            draw::linear(
                90.0,
                &[
                    (0.0, "#6b6c6e"),
                    (0.3, "#d6d7d8"),
                    (0.55, "#9fa0a2"),
                    (1.0, "#5a5b5d"),
                ],
            ),
        ),
    );
    let top = draw::circle(
        "cap",
        cx,
        cy - side_h * 0.5,
        rc,
        draw::radial_at(
            0.42,
            0.36,
            0.8,
            &[(0.0, "#dcdcdc"), (0.6, "#b2b3b4"), (1.0, "#86878a")],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(top, "#58595b", (0.6 * k).max(0.5)),
            vec![draw::inner_shadow("#00000030", 0.0, -k, 2.0 * k)],
        ),
    );

    if let Some(legend) = p.legend.as_deref().filter(|l| !l.is_empty()) {
        let inkc = p.ink_color(PANEL_INK);
        let size = (8.0 * k).max(7.0);
        let t = cased(
            parts_ink(
                "legend",
                legend,
                cx - 2.0 * w,
                cy + r * 1.25 + size * 0.9,
                4.0 * w,
                size,
                &inkc,
                TextAlign::Center,
                700.0,
            ),
            p.case(),
        );
        let t = b.add(root, t);
        c = c.slot(SlotRole::Label, t.0);
    }
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "switch.stomp_flat",
        family: Family::Switch,
        title: "Stomp switch (flat cap)",
        description: "A modern pedal's stomp switch with a plain cap: a big rounded chrome hex nut and a flat-topped satin cylinder, seen from above and a little in front, with its status LED where the pedal puts it (`led_dx`/`led_dy` from the switch centre, `led_size`; a pin-point lamp). Variant = LED colour (`purple` default, `blue`, `red`, `green`, `none`), `color=#hex` for any other. Latching; `momentary` option for tap tempo. Size = nut width.",
        variants: &["purple", "blue", "red", "green", "none"],
        default_size: 40.0,
        binds: "toggle param",
        build: stomp_flat,
    }]
}
