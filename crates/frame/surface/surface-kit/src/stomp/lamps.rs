//! Status lamps and lit faders of modern pedals and desktop EQs.
//!
//! - `meter.status_led`: a small LED in a pale bezel that shows a toggle
//!   param (a pedal's engage LED, an amp's pilot): a `Readout` whose lit
//!   lens is its `On` slot and whose dark lens is its `Off` slot. The
//!   `pulse` option (`pulse=<motion expr for Hz>`) makes the lit lens
//!   breathe with an LFO — a modulation pedal's rate lamp.
//! - `lever.lit_fader`: a graphic-EQ fader: a deep black slot, a bright
//!   white cap lit from within (`Thumb`), the slot glowing faintly round
//!   it — the backlit caps of a modern desktop EQ.

use document_proto::NodeId;
use surface_proto::{ControlKind, Orientation, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::{alpha, rad, shade, tint};
use crate::draw;
use crate::motion;
use crate::registry::{ComponentDef, Family, Place};

fn status_led(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(8.0).max(3.0);
    let color = p.lamp_color(0).unwrap_or_else(|| "#ff3b2f".into());
    let bezel = match p.variant.as_deref() {
        Some("dark") => "#1a1a1c",
        Some("chrome") => "#b9bab7",
        _ => "#e4e2dc",
    };
    let side = d * 2.0;
    let c = side / 2.0;
    let name = if p.name.is_empty() {
        "LED"
    } else {
        p.name.as_str()
    };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side));
    let rim = d * 0.28;
    let bz = draw::circle(
        "bezel",
        c,
        c,
        d / 2.0 + rim,
        rad(
            0.4,
            0.35,
            0.8,
            &[(0.0, tint(bezel, 0.4)), (1.0, shade(bezel, 0.3))],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            bz,
            vec![draw::drop_shadow("#00000059", 0.0, d * 0.08, d * 0.2)],
        ),
    );
    let dark = [
        (0.0, alpha("#ffffff", 0.35)),
        (0.5, tint(&shade(&color, 0.55), 0.3)),
        (1.0, shade(&color, 0.7)),
    ];
    let off = b.add(
        root,
        draw::circle("lens", c, c, d / 2.0, rad(0.4, 0.34, 0.9, &dark)),
    );
    let hot = [
        (0.0, "#ffffff".to_string()),
        (0.3, tint(&color, 0.6)),
        (0.75, color.clone()),
        (1.0, shade(&color, 0.3)),
    ];
    let lit = draw::emissive(
        draw::circle("lamp", c, c, d / 2.0, rad(0.45, 0.4, 0.8, &hot)),
        draw::hex(&color),
        0.9,
        d * 1.8,
    );
    let lit = b.add(
        root,
        draw::with_effects(
            lit,
            vec![draw::drop_shadow(&alpha(&color, 0.8), 0.0, 0.0, d * 0.9)],
        ),
    );
    if let Some(rate) = p.opt("pulse") {
        motion::add(b, lit, motion::pulse(rate.to_string(), 0.35));
    }
    let ctl = p
        .control(ControlKind::Readout)
        .slot(SlotRole::On, lit.0)
        .slot(SlotRole::Off, off.0);
    b.controls.insert(root, ctl);
    root
}

fn lit_fader(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let travel = p.size.unwrap_or(120.0);
    let k = travel / 120.0;
    let (cap_w, cap_h) = (13.0 * k, 22.0 * k);
    let w = (cap_w + 10.0 * k).max(p.touch.unwrap_or(0.0));
    let h = travel + cap_h;
    let name = if p.name.is_empty() {
        "Fader"
    } else {
        p.name.as_str()
    };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, w, h));
    let x0 = w / 2.0;
    let slot_w = 11.0 * k;
    let slot = draw::rect(
        "slot",
        x0 - slot_w / 2.0,
        0.0,
        slot_w,
        h,
        draw::linear(
            90.0,
            &[(0.0, "#050506"), (0.5, "#16161a"), (1.0, "#050506")],
        ),
    );
    let slot = draw::stroked(draw::rounded(slot, slot_w / 2.0), "#ffffff1f", 0.8);
    b.add(
        root,
        draw::with_effects(
            slot,
            vec![
                draw::inner_shadow("#000000", 0.0, 2.0 * k, 3.0 * k),
                draw::drop_shadow("#ffffff26", 0.0, 1.0, 0.0),
            ],
        ),
    );
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "rail",
                x0 - 1.2 * k,
                cap_h / 2.0,
                2.4 * k,
                travel,
                draw::solid("#000000"),
            ),
            1.2 * k,
        ),
    );
    let lane = b.add(root, draw::frame("lane", x0 - cap_w / 2.0, 0.0, cap_w, h));
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap_h));
    let glow = p.lamp_color(0).unwrap_or_else(|| "#f4f6ff".into());
    let body = draw::rect(
        "shell",
        0.0,
        0.0,
        cap_w,
        cap_h,
        draw::linear(
            90.0,
            &[
                (0.0, "#9c9ea3"),
                (0.2, "#f7f7f5"),
                (0.8, "#ffffff"),
                (1.0, "#a3a5aa"),
            ],
        ),
    );
    let body = draw::emissive(
        draw::rounded(body, 1.5 * k),
        draw::hex(&glow),
        0.55,
        9.0 * k,
    );
    b.add(
        cap,
        draw::with_effects(
            body,
            vec![draw::drop_shadow(&alpha(&glow, 0.55), 0.0, 0.0, 6.0 * k)],
        ),
    );
    for y in [0.14, 0.86] {
        b.add(
            cap,
            draw::rect(
                "band",
                0.0,
                cap_h * y - 0.6 * k,
                cap_w,
                1.2 * k,
                draw::solid("#2a2b2e"),
            ),
        );
    }
    let ctl = p
        .control(ControlKind::Fader {
            orientation: Orientation::Vertical,
        })
        .slot(SlotRole::Thumb, cap.0);
    b.controls.insert(root, ctl);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "meter.status_led",
            family: Family::Meter,
            title: "Status LED",
            description: "A small LED in a pale (`light`), `dark` or `chrome` bezel showing a toggle param — a pedal's engage LED set apart from its footswitch, an amp's pilot light. `color=#hex` lights it; `pulse=<Hz expression>` (e.g. `param(chorus/rate)`) makes it breathe like a rate lamp. Size = lens diameter.",
            variants: &["light", "dark", "chrome"],
            default_size: 8.0,
            binds: "toggle param",
            build: status_led,
        },
        ComponentDef {
            id: "lever.lit_fader",
            family: Family::Lever,
            title: "Lit EQ fader",
            description: "A graphic-EQ band fader: a deep rounded black slot and a bright white cap lit from within (`color=#hex` tints its light), the kind on a modern desktop EQ. Vertical `Fader`, no printed scale — print the gain above and the band below. Size = travel.",
            variants: &[],
            default_size: 120.0,
            binds: "param",
            build: lit_fader,
        },
    ]
}
