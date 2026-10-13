//! Brass hardware of soft boutique pedals and amps.
//!
//! - `switch.brass_stomp`: a footswitch in polished brass — a round brass
//!   washer, a brass hex nut and a coloured (teal or cream) cap — with no
//!   LED of its own (the pedal's LED is set apart).
//! - `switch.brass_toggle`: a brass bat toggle through a brass nut, on a
//!   toggle param; the bat leans up when on (the `Thumb` in its lane).
//! - `switch.brass_toggle3`: a three-way brass toggle over a choice param:
//!   a stepped `Knob` whose bat (the `Needle`) swings between its three
//!   positions — right, down and left, for legends printed round it
//!   (`up`: pointing up instead; `color=#hex`: a painted lever).
//! - `button.glow_key`: a square cap lit from within in a pale bezel.
//! - `lever.pill`: a slim fader with a pill-shaped cap.

use document_proto::NodeId;
use surface_proto::{ButtonMode, ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::{alpha, lin, rad, shade, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

const BRASS: [&str; 4] = ["#fff1c4", "#d8b36a", "#9a7236", "#e9cf8e"];

fn brass(angle: f64) -> document_proto::Paint {
    lin(angle, &[(0.0, BRASS[0].into()), (0.35, BRASS[1].into()), (0.62, BRASS[2].into()), (1.0, BRASS[3].into())])
}

fn brass_stomp(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(24.0).max(10.0);
    let k = d / 24.0;
    let cap = match p.variant.as_deref() {
        Some("cream") => "#e9e3d4",
        Some("black") => "#26262a",
        _ => "#4f8f92",
    };
    let side = (d * 2.3).max(p.touch.unwrap_or(0.0));
    let c = side / 2.0;
    let name = if p.name.is_empty() { "Footswitch" } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side));
    // Washer, nut, barrel, cap — each shadowed on the one below.
    b.add(root, draw::circle("shadow", c + 0.5 * k, c + 3.0 * k, d * 1.1, draw::radial(&[(0.0, "#00000070"), (0.8, "#00000040"), (1.0, "#00000000")])));
    let washer = draw::circle("washer", c, c, d * 1.08, draw::conic(20.0, &[(0.0, BRASS[0]), (0.25, BRASS[2]), (0.5, BRASS[3]), (0.75, BRASS[2]), (1.0, BRASS[0])]));
    b.add(root, draw::with_effects(draw::stroked(washer, "#6b4b1e", 0.8 * k), vec![draw::drop_shadow("#00000066", 0.0, 1.5 * k, 3.0 * k)]));
    b.add(root, draw::circle("washer well", c, c, d * 0.98, draw::radial(&[(0.0, "#00000000"), (0.9, "#00000000"), (1.0, "#0000004d")])));
    let nut = draw::path("nut", 0.0, 0.0, side, side, draw::regular(c, c, d * 0.92, 6, 30.0), brass(150.0));
    b.add(root, draw::with_effects(draw::stroked(nut, "#5e421a", 0.8 * k), vec![draw::drop_shadow("#0000008c", 0.0, 2.0 * k, 3.0 * k)]));
    b.add(root, draw::circle("barrel", c, c, d * 0.62, lin(160.0, &[(0.0, "#2b2116".into()), (0.5, "#6b4e22".into()), (1.0, "#1a140c".into())])));
    let stops = [(0.0, tint(cap, 0.35)), (0.6, cap.to_string()), (1.0, shade(cap, 0.35))];
    let top = draw::circle("cap", c, c - 1.0 * k, d / 2.0, rad(0.4, 0.35, 0.8, &stops));
    b.add(root, draw::with_effects(top, vec![draw::drop_shadow("#00000080", 0.0, 1.5 * k, 2.0 * k), draw::inner_shadow("#ffffff40", 0.0, 1.0 * k, 0.5 * k)]));
    // Engaged, the cap's polished face catches the light (the `Led`: full
    // when on, a fifth when off).
    let sheen = draw::circle("cap sheen", c - d * 0.1, c - d * 0.16, d * 0.28, draw::radial(&[(0.0, "#ffffffa0"), (1.0, "#ffffff00")]));
    let sheen = b.add(root, sheen);
    let ctl = p.control(if p.flag("momentary") { ControlKind::Button { mode: ButtonMode::Momentary } } else { ControlKind::footswitch() }).slot(SlotRole::Led, sheen.0);
    b.controls.insert(root, ctl);
    root
}

/// The nut and washer a brass toggle stands in, centred at (`c`, `c`).
fn toggle_nut(b: &mut Builder<'_>, root: NodeId, side: f64, c: f64, d: f64) {
    b.add(root, draw::circle("washer shadow", c + 0.6, c + 1.6, d * 0.62, draw::radial(&[(0.0, "#00000070"), (1.0, "#00000000")])));
    let washer = draw::circle("washer", c, c, d * 0.56, draw::conic(20.0, &[(0.0, BRASS[0]), (0.25, BRASS[2]), (0.5, BRASS[3]), (0.75, BRASS[2]), (1.0, BRASS[0])]));
    b.add(root, draw::stroked(washer, "#5e421a", 0.8));
    b.add(root, draw::path("nut", 0.0, 0.0, side, side, draw::regular(c, c, d * 0.44, 6, 30.0), brass(150.0)));
    b.add(root, draw::circle("bushing", c, c, d * 0.24, rad(0.4, 0.35, 0.8, &[(0.0, "#fff1c4".into()), (1.0, "#6b4e22".into())])));
}

fn bat_paint() -> document_proto::Paint {
    lin(90.0, &[(0.0, "#7a5a2a".into()), (0.35, "#fff1c4".into()), (0.6, "#c9a45c".into()), (1.0, "#6b4e22".into())])
}

fn brass_toggle(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(20.0).max(8.0);
    let side = (d * 2.4).max(p.touch.unwrap_or(0.0));
    let c = side / 2.0;
    let name = if p.name.is_empty() { "Toggle" } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side));
    toggle_nut(b, root, side, c, d);
    let bw = d * 0.36;
    let lane = b.add(root, draw::frame("lane", c - bw / 2.0, c - d * 1.1, bw, d * 2.2));
    let bat = draw::with_effects(draw::rounded(draw::rect("bat", 0.0, 0.0, bw, d * 1.16, bat_paint()), bw / 2.0), vec![draw::drop_shadow("#000000a6", 1.0, 2.0, 3.0)]);
    let bat = b.add(lane, bat);
    let ctl = p.control(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, bat.0);
    b.controls.insert(root, ctl);
    root
}

fn brass_toggle3(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(20.0).max(8.0);
    let side = (d * 2.4).max(p.touch.unwrap_or(0.0));
    let c = side / 2.0;
    let name = if p.name.is_empty() { "Selector" } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side));
    toggle_nut(b, root, side, c, d);
    // The bat, drawn at the centre of its travel (pointing down, or up with
    // `up`), turns about the nut. `color=#hex` paints it (a lever).
    let body = b.add(root, draw::frame("body", 0.0, 0.0, side, side));
    let bw = d * 0.34;
    let up = p.flag("up");
    let len = d * 0.95;
    let y0 = if up { c - len } else { c };
    let (paint, end) = match p.opt("color").filter(|c| c.starts_with('#')) {
        Some(col) => (lin(90.0, &[(0.0, shade(col, 0.3)), (0.4, tint(col, 0.3)), (1.0, shade(col, 0.25))]), [tint(col, 0.45), shade(col, 0.35)]),
        None => (bat_paint(), ["#fff1c4".to_string(), "#8a6630".to_string()]),
    };
    let bat = draw::rounded(draw::rect("bat", c - bw / 2.0, y0, bw, len, paint), bw / 2.0);
    b.add(body, draw::with_effects(bat, vec![draw::drop_shadow("#000000a6", 0.0, 0.0, 2.5)]));
    let ey = if up { c - len + bw / 2.0 } else { c + len - bw / 2.0 };
    b.add(body, draw::circle("bat end", c, ey, bw * 0.7, rad(0.4, 0.35, 0.8, &[(0.0, end[0].clone()), (1.0, end[1].clone())])));
    let sweep = p.sweep.unwrap_or(180.0);
    let ctl = p.control(ControlKind::Knob { sweep_degrees: sweep }).slot(SlotRole::Needle, body.0);
    b.controls.insert(root, ctl);
    root
}

/// A rounded-square cap lit from within in a pale bezel: the orange POWER
/// and cream TAP keys of a boutique echo.
fn glow_key(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(40.0).max(16.0);
    let k = d / 40.0;
    let color = p.lamp_color(0).unwrap_or_else(|| "#ff8a1c".into());
    let side = (d * 1.3).max(p.touch.unwrap_or(0.0));
    let c = side / 2.0;
    let name = if p.name.is_empty() { "Key" } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side));
    let bz = d * 1.18;
    let bezel = draw::rounded(draw::rect("bezel", c - bz / 2.0, c - bz / 2.0, bz, bz, lin(160.0, &[(0.0, "#fbfbf9".into()), (1.0, "#b9bab8".into())])), 7.0 * k);
    b.add(root, draw::with_effects(bezel, vec![draw::drop_shadow("#00000059", 0.0, 2.0 * k, 4.0 * k)]));
    let stops = [(0.0, tint(&color, 0.2)), (0.6, color.clone()), (1.0, shade(&color, 0.35))];
    let dark = draw::rounded(draw::rect("cap", c - d / 2.0, c - d / 2.0, d, d, rad(0.5, 0.5, 0.7, &[(0.0, shade(&color, 0.3)), (1.0, shade(&color, 0.55))])), 5.0 * k);
    b.add(root, draw::with_effects(dark, vec![draw::inner_shadow("#00000066", 0.0, 1.5 * k, 3.0 * k)]));
    // Lit: the cap glows and its inner square burns brighter.
    // The frame carries the light, so the `Led` dims it with the cap.
    let lit = b.add(root, draw::emissive(draw::rounded(draw::frame("lit", c - d / 2.0, c - d / 2.0, d, d), 5.0 * k), draw::hex(&color), 0.6, d * 0.5));
    let cap = draw::rounded(draw::rect("glow", 0.0, 0.0, d, d, rad(0.5, 0.5, 0.7, &stops)), 5.0 * k);
    b.add(lit, cap);
    let inner = draw::rounded(draw::rect("core", d * 0.22, d * 0.22, d * 0.56, d * 0.56, rad(0.5, 0.5, 0.6, &[(0.0, "#ffffff".into()), (1.0, tint(&color, 0.5))])), 3.0 * k);
    b.add(lit, draw::with_effects(inner, vec![draw::inner_shadow("#00000040", 0.0, 1.0, 2.0)]));
    let mode = if p.flag("momentary") { ButtonMode::Momentary } else { ButtonMode::Toggle };
    let ctl = p.control(ControlKind::Button { mode }).slot(SlotRole::Led, lit.0);
    b.controls.insert(root, ctl);
    root
}

/// A slim fader with a pill-shaped cap (`color=#hex`) grooved across the
/// middle, riding a dark slot between tick scales.
fn pill_fader(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let travel = p.size.unwrap_or(110.0);
    let k = travel / 110.0;
    let color = p.opt("color").filter(|c| c.starts_with('#')).unwrap_or("#ece6dc").to_string();
    let ink = p.ink_color("#9a9b9d");
    let (cw, ch) = (30.0 * k, 22.0 * k);
    let w = (cw + 24.0 * k).max(p.touch.unwrap_or(0.0));
    let h = travel + ch;
    let name = if p.name.is_empty() { "Fader" } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, w, h));
    let x0 = w / 2.0;
    for i in 0..=12 {
        let y = ch / 2.0 + travel * f64::from(i) / 12.0;
        let len = if i % 3 == 0 { 9.0 * k } else { 6.0 * k };
        b.add(root, draw::rect("tick", x0 - 10.0 * k - len, y - 0.5, len, 1.0, draw::solid(&ink)));
        b.add(root, draw::rect("tick", x0 + 10.0 * k, y - 0.5, len, 1.0, draw::solid(&ink)));
    }
    let slot = draw::rounded(draw::rect("slot", x0 - 5.0 * k, 0.0, 10.0 * k, h, lin(90.0, &[(0.0, "#0a0a0b".into()), (0.5, "#2a2a2c".into()), (1.0, "#0a0a0b".into())])), 5.0 * k);
    b.add(root, draw::with_effects(slot, vec![draw::inner_shadow("#000000", 0.0, 1.5, 2.0), draw::drop_shadow("#ffffff40", 0.0, 1.0, 0.0)]));
    let lane = b.add(root, draw::frame("lane", x0 - cw / 2.0, 0.0, cw, h));
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cw, ch));
    let shell = draw::rounded(draw::rect("shell", 0.0, 0.0, cw, ch, lin(180.0, &[(0.0, tint(&color, 0.4)), (0.45, color.clone()), (1.0, shade(&color, 0.3))])), ch / 2.0);
    b.add(cap, draw::with_effects(shell, vec![draw::drop_shadow("#00000080", 0.0, 2.0 * k, 3.0 * k)]));
    for y in [0.36, 0.64] {
        b.add(cap, draw::rect("groove", 3.0 * k, ch * y - 0.6 * k, cw - 6.0 * k, 1.2 * k, draw::solid(&alpha(&shade(&color, 0.5), 0.8))));
    }
    let ctl = p.control(ControlKind::Fader { orientation: surface_proto::Orientation::Vertical }).slot(SlotRole::Thumb, cap.0);
    b.controls.insert(root, ctl);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "button.glow_key",
            family: Family::Button,
            title: "Glow key",
            description: "A rounded-square cap lit from within (`color=#hex`, orange by default) in a pale rounded bezel: a boutique echo's POWER, or with `momentary` its TAP key. The lit cap is the `Led`. Size = cap edge.",
            variants: &[],
            default_size: 40.0,
            binds: "toggle param",
            build: glow_key,
        },
        ComponentDef {
            id: "lever.pill",
            family: Family::Lever,
            title: "Pill fader",
            description: "A slim vertical fader: a pill-shaped cap (`color=#hex`, cream by default) grooved across its middle, riding a dark rounded slot between printed tick scales — a boutique reverb's pre-delay, decay, filters and mix. Size = travel.",
            variants: &[],
            default_size: 110.0,
            binds: "param",
            build: pill_fader,
        },
        ComponentDef {
            id: "switch.brass_stomp",
            family: Family::Switch,
            title: "Brass stomp",
            description: "A boutique pedal's footswitch in polished brass: a round brass washer, a brass hex nut and a coloured cap (`teal`, `cream`, `black`), with no LED of its own — set the pedal's LED apart (`meter.status_led`). Latching (`Footswitch`); `momentary` for a tap switch. Size = cap diameter.",
            variants: &["teal", "cream", "black"],
            default_size: 24.0,
            binds: "toggle param",
            build: brass_stomp,
        },
        ComponentDef {
            id: "switch.brass_toggle",
            family: Family::Switch,
            title: "Brass toggle",
            description: "A brass bat toggle through a brass nut and washer, no legends (print them on the panel): a boutique amp's power or bright switch. The bat leans up when the toggle param is on. Size = the nut.",
            variants: &[],
            default_size: 20.0,
            binds: "toggle param",
            build: brass_toggle,
        },
        ComponentDef {
            id: "switch.brass_toggle3",
            family: Family::Switch,
            title: "Brass three-way toggle",
            description: "A three-way brass toggle over a choice param: the bat swings right, down and left (options in that order) through 180°, legends printed round it by the panel (a modulation pedal's tremolo / chorus / vibrato, an amp's voicing). A stepped `Knob`. Size = the nut.",
            variants: &[],
            default_size: 20.0,
            binds: "choice param",
            build: brass_toggle3,
        },
    ]
}
