//! `switch.stomp_box`: the plain 3PDT stomp switch of a hand-built pedal,
//! seen from above — a chrome hex nut round a squarish polished button with
//! a step in its top — with a big bare status LED (a domed lens in a dark
//! ring, no bezel) wherever the pedal puts it.
//!
//! ```text
//! <root control=Footswitch>   box round the switch and its LED
//!   led ring, unlit lens, lit lens ← Led          (lit while the param is on)
//!   shadow, nut, washer, button, light
//!   legend ← Label (under the switch)
//! ```
//!
//! Size = the nut's width across the flats. Options: `color=#hex` the LED
//! colour (variant `none` = no LED); `led_dx=`, `led_dy=` where the LED sits
//! from the switch's centre (default straight above, 1.5 × size up);
//! `led_size=` the lens diameter; `momentary`.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, cased, ink as parts_ink, shade, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

fn num(p: &Place, key: &str) -> Option<f64> {
    p.opt(key).and_then(|v| v.parse().ok())
}

#[allow(clippy::too_many_lines)]
fn stomp_box(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(16.0);
    let k = d / 40.0;
    let led = match p.variant.as_deref() {
        Some("none") => None,
        Some("red") => Some("#ff3326".to_string()),
        Some("blue") => Some("#6fb8ff".to_string()),
        Some("amber") => Some("#ffb21e".to_string()),
        _ => Some("#3dff6a".to_string()),
    };
    let led = led.map(|c| p.lamp_color(0).unwrap_or(c));
    let (ldx, ldy) = (
        num(p, "led_dx").unwrap_or(0.0),
        num(p, "led_dy").unwrap_or(-1.5 * d),
    );
    let led_d = num(p, "led_size").unwrap_or(d * 0.4);
    let led_r = led_d / 2.0 + 2.0 * k;
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
            draw::circle("led ring", lx, ly, lr + 1.6 * k, draw::solid("#050505")),
        );
        let unlit = draw::circle(
            "lens",
            lx,
            ly,
            lr,
            draw::radial_at(
                0.4,
                0.34,
                0.9,
                &[
                    (0.0, "#ffffff38"),
                    (0.5, &shade(color, 0.72)),
                    (1.0, "#000000e0"),
                ],
            ),
        );
        b.add(root, unlit);
        let hot = draw::circle(
            "lamp",
            lx,
            ly,
            lr,
            draw::radial_at(
                0.48,
                0.44,
                0.72,
                &[
                    (0.0, &tint(color, 0.75)),
                    (0.3, &tint(color, 0.35)),
                    (0.7, color),
                    (1.0, &shade(color, 0.35)),
                ],
            ),
        );
        let lit = b.add(
            root,
            draw::emissive(hot, draw::hex(color), 1.0, led_d * 1.8),
        );
        c = c.slot(SlotRole::Led, lit.0);
        b.add(
            root,
            draw::circle(
                "lens glint",
                lx - lr * 0.3,
                ly - lr * 0.35,
                lr * 0.28,
                draw::radial(&[(0.0, "#ffffffa0"), (1.0, "#ffffff00")]),
            ),
        );
    }

    // Shadow, the hex nut, the washer, the button.
    b.add(
        root,
        draw::circle(
            "shadow",
            cx + 1.0 * k,
            cy + 4.0 * k,
            r * 1.05,
            draw::radial(&[(0.0, "#000000b8"), (0.72, "#00000070"), (1.0, "#00000000")]),
        ),
    );
    let nut = draw::path(
        "nut",
        0.0,
        0.0,
        w,
        h,
        draw::regular(cx, cy, r / 30f64.to_radians().cos(), 6, 0.0),
        draw::linear(
            160.0,
            &[
                (0.0, "#f6f6f4"),
                (0.35, "#b4b5b3"),
                (0.55, "#7d7e7c"),
                (0.75, "#d8d9d6"),
                (1.0, "#8f908d"),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(nut, "#3a3a3c", (0.7 * k).max(0.5)),
            vec![draw::inner_shadow("#ffffff80", 0.0, 1.0 * k, 1.0 * k)],
        ),
    );
    b.add(
        root,
        draw::circle(
            "washer",
            cx,
            cy,
            r * 0.82,
            draw::conic(
                10.0,
                &[
                    (0.0, "#eeeeec"),
                    (0.25, "#8a8b89"),
                    (0.5, "#e2e2e0"),
                    (0.75, "#7a7b79"),
                    (1.0, "#eeeeec"),
                ],
            ),
        ),
    );
    let s = r * 1.08;
    let body = draw::rounded(
        draw::rect(
            "button",
            cx - s / 2.0,
            cy - s / 2.0 - 1.0 * k,
            s,
            s * 1.02,
            draw::linear(
                170.0,
                &[
                    (0.0, "#fbfbfa"),
                    (0.45, "#c9cac8"),
                    (0.7, "#8f908e"),
                    (1.0, "#b8b9b7"),
                ],
            ),
        ),
        s * 0.22,
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(body, "#2c2c2e", (0.7 * k).max(0.5)),
            vec![draw::drop_shadow("#00000099", 0.0, 1.5 * k, 2.5 * k)],
        ),
    );
    let top = draw::rounded(
        draw::rect(
            "button top",
            cx - s * 0.36,
            cy - s * 0.42,
            s * 0.72,
            s * 0.6,
            draw::linear(175.0, &[(0.0, "#ffffff"), (1.0, "#b0b1af")]),
        ),
        s * 0.16,
    );
    b.add(root, draw::stroked(top, "#7c7d7b", (0.5 * k).max(0.4)));
    b.add(
        root,
        draw::rect(
            "button step",
            cx - s * 0.36,
            cy + s * 0.22,
            s * 0.72,
            1.2 * k,
            draw::solid("#55565480"),
        ),
    );

    if let Some(legend) = p.legend.as_deref().filter(|l| !l.is_empty()) {
        let inkc = p.ink_color(PANEL_INK);
        let size = (9.0 * k).max(8.0);
        let t = cased(
            parts_ink(
                "legend",
                legend,
                cx - 2.0 * w,
                cy + r * 1.2 + size * 0.9,
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
        id: "switch.stomp_box",
        family: Family::Switch,
        title: "Stomp switch (plain 3PDT)",
        description: "A hand-built pedal's plain stomp switch from above: a chrome hex nut round a squarish polished button, with a big bare status LED (a domed lens in a dark ring) where the pedal puts it (`led_dx`/`led_dy` from the switch centre, default straight above; `led_size`). Variant = LED colour (`green` default, `red`, `blue`, `amber`, `none`), `color=#hex` for any other. Latching; `momentary` for tap tempo. Size = nut width.",
        variants: &["green", "red", "blue", "amber", "none"],
        default_size: 40.0,
        binds: "toggle param",
        build: stomp_box,
    }]
}
