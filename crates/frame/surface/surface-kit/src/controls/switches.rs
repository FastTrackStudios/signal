//! Panel switches — the bat toggle, the rocker and the rotary selector.
//! Ported from `fts-audio-ui::hardware::switches` (`ToggleSwitch`) and the
//! rack kit's detented rotary switch (`Ring::Detents`).
//!
//! A toggle is drawn so that the only thing anyone reads about it — which
//! way the bat leans — is the moving part: the bat is a `Thumb` in a lane
//! the height of the slot, so it sits up when the toggle param is on and
//! down when it is off. A rotary selector is a `Knob` over a choice param
//! whose sweep is exactly its detents.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, cased, fill, ink as parts_ink, lin, rad, shade, text_w, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// Two legends, top then bottom, from `Place::options`.
fn two(p: &Place, top: &str, bottom: &str) -> [String; 2] {
    match p.options.as_deref() {
        Some([a, b, ..]) => [a.clone(), b.clone()],
        _ => [top.to_string(), bottom.to_string()],
    }
}

/// A two-position bat toggle (LA-2A COMPRESS/LIMIT, a POWER switch): a
/// chrome bat through a nut in a black slot, legends beside it, caption
/// (`Label`) above.
fn toggle_bat(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let inkc = p.ink_color(PANEL_INK);
    let case = p.case();
    #[allow(clippy::too_many_arguments)]
    let ink = |name: &str, s: &str, x: f64, cy: f64, w: f64, size: f64, color: &str, align: TextAlign, weight: f64| {
        cased(parts_ink(name, s, x, cy, w, size, color, align, weight), case)
    };
    let bh = p.size.unwrap_or(44.0);
    let k = bh / 44.0;
    let bw = 22.0 * k;
    let bat = match p.variant.as_deref() {
        Some("black") => ["#2a2a2e", "#5a5a60", "#3a3a3e", "#141416"],
        _ => ["#7c7a74", "#f6f4ee", "#c9c7c0", "#6c6a64"],
    };
    let [on, off] = two(p, "On", "Off");
    let size = 8.5 * k;
    let legend_w = text_w(&on, size).max(text_w(&off, size)).max(size * 2.0);
    let gap = 6.0 * k;
    let cap_h = (9.0 * k * 1.4).ceil();
    let w = legend_w + gap + bw;
    let h = cap_h + 4.0 * k + bh;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let label = b.add(root, ink("label", p.legend.as_deref().unwrap_or(""), 0.0, cap_h / 2.0, w, 9.0 * k, &inkc, TextAlign::Center, 700.0));
    let top = cap_h + 4.0 * k;
    b.add(root, ink("on", &on, 0.0, top + size * 0.9, legend_w, size, &inkc, TextAlign::Right, 700.0));
    b.add(root, ink("off", &off, 0.0, top + bh - size * 0.9, legend_w, size, &inkc, TextAlign::Right, 700.0));
    let x = legend_w + gap;
    let slot = draw::rect("slot", x, top, bw, bh, lin(180.0, &[(0.0, "#17171a".into()), (1.0, "#0a0a0c".into())]));
    let slot = draw::stroked(draw::rounded(slot, 5.0 * k), "#00000099", k.max(1.0));
    b.add(root, draw::with_effects(slot, vec![draw::inner_shadow("#000000b3", 0.0, 0.0, 5.0 * k)]));
    // The nut the bat comes out of.
    let (cx, cy) = (x + bw / 2.0, top + bh / 2.0);
    b.add(root, draw::path("nut", 0.0, 0.0, w, h, draw::regular(cx, cy, bw * 0.40, 6, 30.0), rad(0.35, 0.3, 0.9, &[(0.0, "#e8e8e4".into()), (1.0, "#5c5c5a".into())])));
    b.add(root, draw::circle("bushing", cx, cy, bw * 0.26, rad(0.4, 0.35, 0.8, &[(0.0, "#bdbdb8".into()), (1.0, "#3a3a38".into())])));
    // The lane the bat travels in: up = on, down = off.
    let lane_y = top + bh * 0.06;
    let lane = b.add(root, draw::frame("lane", x, lane_y, bw, bh * 0.88));
    let bat_node = draw::rect(
        "bat",
        bw * 0.28,
        0.0,
        bw * 0.44,
        bh * 0.46,
        lin(90.0, &[(0.0, bat[0].into()), (0.38, bat[1].into()), (0.62, bat[2].into()), (1.0, bat[3].into())]),
    );
    let bat_node = draw::with_effects(draw::rounded(bat_node, bw * 0.22), vec![draw::drop_shadow("#00000099", 0.0, 1.5 * k, 3.0 * k)]);
    let bat_id = b.add(lane, bat_node);
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, bat_id.0).slot(SlotRole::Label, label.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

/// A rocker: the proud end catches the light, the pressed end is in shade.
/// The light is the `Thumb` (top half when on), `I`/`O` printed over it.
fn rocker(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let inkc = p.ink_color(PANEL_INK);
    let case = p.case();
    #[allow(clippy::too_many_arguments)]
    let ink = |name: &str, s: &str, x: f64, cy: f64, w: f64, size: f64, color: &str, align: TextAlign, weight: f64| {
        cased(parts_ink(name, s, x, cy, w, size, color, align, weight), case)
    };
    let h = p.size.unwrap_or(44.0);
    let k = h / 44.0;
    let w = 26.0 * k;
    let (cap_c, mark) = match p.variant.as_deref() {
        Some("red") => ("#b8261c", "#f6e6e2"),
        Some("white") => ("#dcd8cc", "#23252a"),
        _ => ("#1f2023", "#e8e8e6"),
    };
    let [on, off] = two(p, "I", "O");
    let cap_h = (9.0 * k * 1.4).ceil();
    let top = cap_h + 4.0 * k;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, top + h));
    let label = b.add(root, ink("label", p.legend.as_deref().unwrap_or(""), -12.0 * k, cap_h / 2.0, w + 24.0 * k, 9.0 * k, &inkc, TextAlign::Center, 700.0));
    let bezel = draw::rect("bezel", 0.0, top, w, h, lin(180.0, &[(0.0, "#2a2a2e".into()), (1.0, "#0e0e10".into())]));
    let bezel = draw::stroked(draw::rounded(bezel, 3.5 * k), "#000000b3", k.max(1.0));
    b.add(root, draw::with_effects(bezel, vec![draw::drop_shadow("#00000080", 0.0, k, 3.0 * k)]));
    let inset = 3.0 * k;
    let (cx, cy, cw, ch) = (inset, top + inset, w - 2.0 * inset, h - 2.0 * inset);
    let body = draw::rect("rocker", cx, cy, cw, ch, fill(&shade(cap_c, 0.25)));
    b.add(root, draw::with_effects(draw::rounded(body, 2.0 * k), vec![draw::inner_shadow("#000000a0", 0.0, 0.0, 3.0 * k)]));
    let lane = b.add(root, draw::frame("lane", cx, cy, cw, ch));
    let proud = draw::rect("proud", 0.0, 0.0, cw, ch * 0.5, lin(180.0, &[(0.0, tint(cap_c, 0.22)), (0.85, cap_c.to_string()), (1.0, shade(cap_c, 0.3))]));
    let proud = draw::with_effects(draw::rounded(proud, 2.0 * k), vec![draw::inner_shadow("#ffffff40", 0.0, k, 0.0), draw::drop_shadow("#000000a0", 0.0, 0.0, 2.0 * k)]);
    let proud = b.add(lane, proud);
    let size = 10.0 * k;
    b.add(root, ink("on", &on, cx, cy + ch * 0.25, cw, size, mark, TextAlign::Center, 700.0));
    b.add(root, ink("off", &off, cx, cy + ch * 0.75, cw, size, mark, TextAlign::Center, 700.0));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, proud.0).slot(SlotRole::Label, label.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

/// A detented rotary selector: the kit's chicken-head (`knob.chicken_head`
/// — the same moulding, beak and hub) over a dot at each detent, the
/// option legends printed round it clear of the dots. Black by default;
/// three numbered positions without options.
fn rotary(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let mut p = p.clone();
    if p.variant.is_none() {
        p.variant = Some("black".into());
    }
    if p.scale.is_none() && p.options.as_ref().is_none_or(Vec::is_empty) {
        p.options = Some((1..=3).map(|i| i.to_string()).collect());
    }
    let spec = crate::knobs::spec::KnobSpec { ring: crate::knobs::spec::RingKind::Dots, ..crate::knobs::spec::CHICKEN_HEAD };
    crate::knobs::build(b, parent, &p, &spec, "Rotary selector", 48.0)
}

/// A stompbox footswitch: a domed chrome actuator through a hex nut and
/// washer, with an optional status LED above it (the `Led`). Latching (a
/// `Footswitch`: a tap flips the toggle param, a hold is its own gesture)
/// or, with the `momentary` option, on only while held.
fn footswitch(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(30.0).max(12.0);
    let k = d / 30.0;
    let led = match p.variant.as_deref() {
        Some("none") => None,
        Some("green") => Some("#46e05a"),
        Some("blue") => Some("#4aa8ff"),
        Some("amber") => Some("#ffb21e"),
        _ => Some("#ff3b2f"),
    };
    let custom = p.lamp_color(0);
    let led = if led.is_some() { custom.as_deref().or(led) } else { None };
    let nut = d * 0.78;
    let led_d = d * 0.22;
    let led_gap = d * 1.05;
    let w = (nut * 2.0 + 8.0 * k).max(p.touch.unwrap_or(0.0));
    let top = if led.is_some() { led_gap + led_d / 2.0 + 4.0 * k } else { nut + 4.0 * k };
    let h = (top + nut + 6.0 * k).max(p.touch.unwrap_or(0.0));
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let (x0, y0) = (w / 2.0, top);
    let mut c = p.control(if p.flag("momentary") { ControlKind::Button { mode: ButtonMode::Momentary } } else { ControlKind::footswitch() });
    if let Some(color) = led {
        let lens = super::parts::lamp(b, root, x0, y0 - led_gap, led_d, color);
        c = c.slot(SlotRole::Led, lens.0);
    }
    // The nut's shadow on the enclosure, the nut, the washer, the barrel,
    // then the domed actuator catching the room's light.
    b.add(root, draw::circle("shadow", x0, y0 + 3.0 * k, nut * 0.98, draw::radial(&[(0.0, "#00000080"), (0.8, "#00000050"), (1.0, "#00000000")])));
    let hexnut = lin(160.0, &[(0.0, "#f4f4f1".into()), (0.45, "#9a9b98".into()), (0.55, "#6f706d".into()), (1.0, "#d6d6d2".into())]);
    b.add(root, draw::path("nut", 0.0, 0.0, w, h, draw::regular(x0, y0, nut, 6, 30.0), hexnut));
    b.add(root, draw::circle("washer", x0, y0, nut * 0.8, draw::conic(0.0, &[(0.0, "#e9e9e5"), (0.2, "#8c8d8a"), (0.45, "#f2f2ee"), (0.7, "#7a7b78"), (1.0, "#e9e9e5")])));
    b.add(root, draw::circle("barrel", x0, y0, d / 2.0 + 3.0 * k, draw::solid("#2b2b2d")));
    let cap = draw::circle("cap", x0, y0, d / 2.0, draw::radial_at(0.38, 0.32, 0.85, &[(0.0, "#ffffff"), (0.25, "#d9dad7"), (0.6, "#8e8f8c"), (1.0, "#4a4b49")]));
    b.add(root, draw::with_effects(cap, vec![draw::inner_shadow("#00000066", 0.0, -2.0 * k, 4.0 * k)]));
    b.add(root, draw::circle("spec", x0 - d * 0.16, y0 - d * 0.18, d * 0.12, draw::solid("#ffffffbb")));
    if let Some(legend) = p.legend.as_deref().filter(|l| !l.is_empty()) {
        let inkc = p.ink_color(PANEL_INK);
        let size = (9.0 * k).max(7.5);
        let t = cased(parts_ink("legend", legend, -w, h + size, 3.0 * w, size, &inkc, TextAlign::Center, 700.0), p.case());
        let t = b.add(root, t);
        c = c.slot(SlotRole::Label, t.0);
    }
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "switch.footswitch",
            family: Family::Switch,
            title: "Footswitch",
            description: "A stompbox footswitch: a domed chrome actuator through a hex nut, with a status LED above it that lights while the toggle param is on (variant = LED colour, or `none`; `color=#hex` in `options` for any other). Latching by default (a `Footswitch`: tap flips the param, hold is its own gesture); the `momentary` option makes it on only while held. `legend` prints under it. Size = actuator diameter.",
            variants: &["red", "green", "blue", "amber", "none"],
            default_size: 30.0,
            binds: "toggle param",
            build: footswitch,
        },
        ComponentDef {
            id: "switch.toggle_bat",
            family: Family::Switch,
            title: "Bat toggle",
            description: "A two-position panel toggle — chrome bat through a hex nut in a black slot, legends beside it (LA-2A COMPRESS/LIMIT, POWER). The bat leans up when the toggle param is on. `Place::options` = [top, bottom] legends (default On/Off); `legend` = caption above. Size = slot height.",
            variants: &["chrome", "black"],
            default_size: 44.0,
            binds: "toggle param",
            build: toggle_bat,
        },
        ComponentDef {
            id: "switch.rocker",
            family: Family::Switch,
            title: "Rocker switch",
            description: "A rocker in a black bezel: the raised end catches the light (top = on). Printed I/O, or `Place::options` = [top, bottom]. Power and mode switches. Size = bezel height.",
            variants: &["black", "red", "white"],
            default_size: 44.0,
            binds: "toggle param",
            build: rocker,
        },
        ComponentDef {
            id: "switch.rotary",
            family: Family::Switch,
            title: "Rotary selector",
            description: "A detented rotary switch: the chicken-head moulding (as `knob.chicken_head`; black, cream or red) with a dot and printed legend at each position (LA-2A/Manley meter and mode selectors). Put the choice param's option names in `Place::options` — the sweep is built from their count (≤60° per detent); without them it prints three numbered positions. Size = knob diameter.",
            variants: &["black", "cream", "red"],
            default_size: 48.0,
            binds: "choice param",
            build: rotary,
        },
    ]
}
