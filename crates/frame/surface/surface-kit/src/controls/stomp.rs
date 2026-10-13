//! `switch.stomp`: a modern pedal's stomp switch seen from above — a big
//! chrome hex nut, a knurled collar and a spun-metal cap — with its status
//! LED wherever the pedal puts it (above it in the bottom band, or beside
//! it on a twin-switch box).
//!
//! ```text
//! <root control=Footswitch>   box round the switch and its LED
//!   led bezel, unlit lens, lit lens ← Led          (lit while the param is on)
//!   shadow, nut, collar + knurl, cap, light
//!   legend ← Label (under the switch)
//! ```
//!
//! Size = the nut's width across the flats. Options: `color=#hex` the LED
//! colour (variant `none` = no LED); `led_dx=`, `led_dy=` where the LED sits
//! from the switch's centre (default straight above, 1.55 × size up);
//! `led_size=` its lens diameter; `led_style=bezel|dot` (a chrome-rimmed
//! lens, or a bare pin-point); `momentary`.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, cased, ink as parts_ink};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

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

/// A hexagon (flats top and bottom) with its corners rounded by `round`.
fn rounded_hex(cx: f64, cy: f64, r: f64, round: f64) -> String {
    let pts: Vec<(f64, f64)> = (0..6)
        .map(|i| draw::polar(cx, cy, r, 30.0 + 60.0 * f64::from(i)))
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

fn num(p: &Place, key: &str) -> Option<f64> {
    p.opt(key).and_then(|v| v.parse().ok())
}

#[allow(clippy::too_many_lines)]
fn stomp(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(44.0).max(16.0);
    let k = d / 44.0;
    let led = match p.variant.as_deref() {
        Some("none") => None,
        Some("red") => Some("#ff3b2f".to_string()),
        Some("green") => Some("#3dff6a".to_string()),
        Some("purple") => Some("#b98cff".to_string()),
        _ => Some("#6fb8ff".to_string()),
    };
    let led = led.map(|c| p.lamp_color(0).unwrap_or(c));
    let (ldx, ldy) = (
        num(p, "led_dx").unwrap_or(0.0),
        num(p, "led_dy").unwrap_or(-1.55 * d),
    );
    let led_d = num(p, "led_size").unwrap_or(d * 0.36);
    let dot = p.opt("led_style") == Some("dot");
    let led_r = if dot {
        led_d / 2.0 + 2.0
    } else {
        led_d / 2.0 + d * 0.1
    };
    let r = d / 2.0;
    let touch = p.touch.unwrap_or(0.0).max(d);
    // The box round the switch (and its LED), padded to a fingertip.
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
        if dot {
            b.add(
                root,
                draw::circle("led socket", lx, ly, lr + 1.2, draw::solid("#00000066")),
            );
        } else {
            let bez = draw::circle(
                "led bezel",
                lx,
                ly,
                lr + d * 0.1,
                draw::conic(
                    20.0,
                    &[
                        (0.0, "#f2f2f0"),
                        (0.25, "#6c6d6f"),
                        (0.5, "#dcdcda"),
                        (0.75, "#4c4d50"),
                        (1.0, "#f2f2f0"),
                    ],
                ),
            );
            b.add(
                root,
                draw::with_effects(
                    bez,
                    vec![draw::drop_shadow("#000000aa", 0.0, 1.5 * k, 2.5 * k)],
                ),
            );
            b.add(
                root,
                draw::circle("led well", lx, ly, lr + d * 0.02, draw::solid("#050608")),
            );
        }
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
                    (0.0, "#ffffff30"),
                    (0.5, &super::parts::shade(color, 0.7)),
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
                0.45,
                0.42,
                0.75,
                &[
                    (0.0, "#ffffff"),
                    (0.35, &super::parts::tint(color, 0.6)),
                    (0.75, color),
                    (1.0, &super::parts::shade(color, 0.4)),
                ],
            ),
        );
        let lit = b.add(
            root,
            draw::emissive(hot, draw::hex(color), 0.9, led_d * 1.6),
        );
        c = c.slot(SlotRole::Led, lit.0);
    }

    // The nut's shadow, the nut, the knurled collar, the spun cap.
    b.add(
        root,
        draw::circle(
            "shadow",
            cx + 1.0 * k,
            cy + 5.0 * k,
            r * 1.02,
            draw::radial(&[(0.0, "#000000b8"), (0.72, "#00000070"), (1.0, "#00000000")]),
        ),
    );
    let nut = draw::path(
        "nut",
        0.0,
        0.0,
        w,
        h,
        rounded_hex(cx, cy, r / (30f64.to_radians().cos()), r * 0.18),
        draw::linear(
            160.0,
            &[
                (0.0, "#fdfdfb"),
                (0.3, "#c4c5c3"),
                (0.5, "#8d8e8c"),
                (0.68, "#e2e3e0"),
                (1.0, "#9fa09d"),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(nut, "#4a4a4c", (0.7 * k).max(0.5)),
            vec![draw::inner_shadow("#ffffff80", 0.0, 1.0 * k, 1.0 * k)],
        ),
    );
    let rk = r * 0.8;
    b.add(
        root,
        draw::circle("collar", cx, cy, rk, draw::solid("#3d3e41")),
    );
    let ribs = 40u32;
    let ridge: String = (0..ribs)
        .map(|i| {
            let a = 360.0 * f64::from(i) / f64::from(ribs);
            seg(
                draw::polar(cx, cy, r * 0.6, a),
                draw::polar(cx, cy, rk - 0.4 * k, a),
                1.3 * k,
            )
        })
        .collect();
    b.add(
        root,
        draw::path("knurl", 0.0, 0.0, w, h, ridge, draw::solid("#b9babc")),
    );
    let cap = draw::circle(
        "cap",
        cx,
        cy,
        r * 0.58,
        draw::conic(
            25.0,
            &[
                (0.0, "#fafaf8"),
                (0.12, "#8d8e8c"),
                (0.26, "#e4e4e2"),
                (0.4, "#6f706e"),
                (0.52, "#f2f2f0"),
                (0.66, "#8a8b89"),
                (0.8, "#dcdcda"),
                (0.92, "#7a7b79"),
                (1.0, "#fafaf8"),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(cap, "#1f1f21", (0.7 * k).max(0.5)),
            vec![draw::drop_shadow("#00000080", 0.0, 1.0 * k, 2.0 * k)],
        ),
    );
    b.add(
        root,
        draw::circle(
            "cap light",
            cx - r * 0.18,
            cy - r * 0.22,
            r * 0.34,
            draw::radial(&[(0.0, "#ffffff70"), (1.0, "#ffffff00")]),
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

/// `switch.push_dome`: a small domed metal push button on a pedal (a latching
/// mode switch): a turned bezel, the dome catching the room's light; it
/// sinks a little and darkens when latched.
fn dome(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(22.0).max(10.0);
    let k = d / 22.0;
    let box_ = d.max(p.touch.unwrap_or(0.0));
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, box_, box_));
    let c = box_ / 2.0;
    let r = d / 2.0;
    let (hi, lo) = if p.variant.as_deref() == Some("black") {
        ("#6a6b6f", "#101012")
    } else {
        ("#ffffff", "#8a8c90")
    };
    b.add(
        root,
        draw::circle(
            "shadow",
            c + 0.5 * k,
            c + 2.5 * k,
            r * 1.05,
            draw::radial(&[(0.0, "#00000099"), (0.8, "#00000044"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        root,
        draw::stroked(
            draw::circle(
                "bezel",
                c,
                c,
                r,
                draw::conic(
                    30.0,
                    &[
                        (0.0, "#f4f4f2"),
                        (0.25, "#7c7d7f"),
                        (0.5, "#e2e2e0"),
                        (0.75, "#6a6b6d"),
                        (1.0, "#f4f4f2"),
                    ],
                ),
            ),
            "#2a2a2c",
            0.6,
        ),
    );
    let cap = draw::circle(
        "cap",
        c,
        c,
        r * 0.74,
        draw::radial_at(
            0.4,
            0.34,
            0.85,
            &[
                (0.0, hi),
                (0.55, &super::parts::mixs(hi, lo, 0.5)),
                (1.0, lo),
            ],
        ),
    );
    b.add(
        root,
        draw::with_effects(cap, vec![draw::inner_shadow("#00000055", 0.0, -k, 2.0 * k)]),
    );
    // Latched: the cap sits lower in its bezel, in its own shade.
    let pressed = b.add(
        root,
        draw::circle(
            "latched",
            c,
            c,
            r * 0.74,
            draw::radial(&[(0.0, "#00000000"), (0.7, "#00000026"), (1.0, "#00000073")]),
        ),
    );
    b.add(
        root,
        draw::circle(
            "spec",
            c - r * 0.22,
            c - r * 0.26,
            r * 0.2,
            draw::solid("#ffffffb0"),
        ),
    );
    let mut ctl = p
        .control(ControlKind::Button {
            mode: if p.flag("momentary") {
                ButtonMode::Momentary
            } else {
                ButtonMode::Toggle
            },
        })
        .slot(SlotRole::Led, pressed.0);
    if let Some(legend) = p.legend.as_deref().filter(|l| !l.is_empty()) {
        let inkc = p.ink_color(PANEL_INK);
        let size = (8.0 * k).max(7.0);
        let t = b.add(
            root,
            cased(
                parts_ink(
                    "legend",
                    legend,
                    c - 3.0 * box_,
                    c + r + size * 1.1,
                    6.0 * box_,
                    size,
                    &inkc,
                    TextAlign::Center,
                    700.0,
                ),
                p.case(),
            ),
        );
        ctl = ctl.slot(SlotRole::Label, t.0);
    }
    b.controls.insert(root, ctl);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "switch.stomp",
            family: Family::Switch,
            title: "Stomp switch (knurled)",
            description: "A modern pedal's stomp switch from above: a big chrome hex nut, a knurled collar and a spun-metal cap, with its status LED where the pedal puts it (`led_dx`/`led_dy` from the switch centre, default straight above; `led_style=bezel` chrome-rimmed lens or `dot` pin-point; `led_size`). Variant = LED colour (`blue` default, `red`, `green`, `purple`, `none`), `color=#hex` for any other. Latching; `momentary` option for tap tempo. Size = nut width.",
            variants: &["blue", "red", "green", "purple", "none"],
            default_size: 44.0,
            binds: "toggle param",
            build: stomp,
        },
        ComponentDef {
            id: "switch.push_dome",
            family: Family::Switch,
            title: "Push dome (pedal)",
            description: "A small domed metal push button on a stompbox (a latching mode or intensity switch): a turned chrome bezel and a polished dome (`chrome`, or `black`), shaded when latched; `momentary` option. Legend under it. Size = bezel diameter.",
            variants: &["chrome", "black"],
            default_size: 22.0,
            binds: "toggle param",
            build: dome,
        },
    ]
}
