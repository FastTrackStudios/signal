//! What the Modern Rhythm units share: legends cut as bold lettering and
//! set as a control's `Label`, the pedals' printed arrows and power glyph,
//! the bottom band (the purple pin-point LED over the stomp), and the
//! small LCD windows.

use document_proto::{NodeId, TextAlign};
use surface_kit::draw;
use surface_proto::{SlotRole, ValueFormat};
use surface_widgets::Builder;

use crate::delay::parts::{self, Screen};
use crate::rigkit::{self as rk, Letter};

/// The pedals' status LED.
pub const LED_PURPLE: &str = "#c69bff";

/// A legend cut as lettering, centred on panel (`cx`, `cy`), set as the
/// control's `Label` (so the control is named by the word it prints).
pub fn legend(b: &mut Builder<'_>, control: NodeId, s: &str, (cx, cy): (f64, f64), l: Letter<'_>) -> NodeId {
    let (tx, ty) = b.scene.node(control).map_or((0.0, 0.0), |n| (n.transform.tx, n.transform.ty));
    let id = rk::letter(b, control, s, (cx - tx, cy - ty), l);
    if let Some(c) = b.controls.get_mut(&control) {
        c.slots.retain(|s| s.role != SlotRole::Label);
        c.slots.push(surface_proto::Slot { role: SlotRole::Label, node: id.0 });
    }
    id
}

/// Printed ▶ arrows at both sides (signal in on the right, out on the left
/// is how the boxes print them: both point right).
pub fn arrows(b: &mut Builder<'_>, p: NodeId, (w, h): (f64, f64), y: f64, x_in: f64, color: &str) {
    for x in [x_in, w - x_in] {
        let d = draw::polygon(&[(x - 2.6, y - 3.2), (x + 3.0, y), (x - 2.6, y + 3.2)]);
        rk::shape(b, p, "arrow", (w, h), d, color);
    }
}

/// The printed power glyph: "9VDC" over a centre-negative jack symbol.
pub fn vdc(b: &mut Builder<'_>, p: NodeId, (w, h): (f64, f64), (cx, cy): (f64, f64), color: &str) {
    rk::letter(b, p, "9VDC", (cx, cy - 4.5), Letter::new(3.6, 700.0, color));
    let ring = |x: f64| {
        let r = 1.9;
        let k = 0.5523 * r;
        format!(
            "M {:.2} {cy:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} C {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} C {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} Z ",
            x + r, x + r, cy + k, x + k, cy + r, cy + r, x - k, cy + r, x - r, cy + k, x - r, x - r, cy - k, x - k, cy - r, cy - r, x + k, cy - r, x + r, cy - k, x + r
        )
    };
    let mut n = draw::path("power glyph", 0.0, 0.0, w, h, format!("{}{}{}", ring(cx - 5.2), ring(cx), ring(cx + 5.2)), draw::solid(color));
    n.fills = Vec::new();
    n.stroke = document_proto::Stroke { paints: vec![draw::solid(color)], weight: 0.6, ..document_proto::Stroke::default() };
    b.add(p, n);
    b.add(p, draw::circle("pin", cx, cy, 0.6, draw::solid(color)));
    rk::line(b, p, &[(cx - 3.3, cy), (cx - 1.9, cy)], 0.6, color);
    rk::line(b, p, &[(cx + 1.9, cy), (cx + 3.3, cy)], 0.6, color);
}

/// The bottom band: the purple pin-point LED over the stomp switch.
pub fn band(b: &mut Builder<'_>, p: NodeId, w: f64, addr: &str) -> NodeId {
    stomp(b, p, "Bypass", addr, (w / 2.0, 244.0), 38.0, (0.0, -39.0), false)
}

/// The flat-capped stomp switch (`switch.stomp_flat`), its nut centred on
/// (`cx`, `cy`), its purple LED (`dx`, `dy`) from it.
#[allow(clippy::too_many_arguments)]
pub fn stomp(b: &mut Builder<'_>, p: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64), d: f64, (dx, dy): (f64, f64), momentary: bool) -> NodeId {
    let mut opts = vec![format!("color={LED_PURPLE}"), format!("led_dx={dx}"), format!("led_dy={dy}"), "led_size=5.5".to_string()];
    if momentary {
        opts.push("momentary".into());
    }
    let place = surface_kit::Place { options: Some(opts), ..surface_kit::Place::new(name).size(d).bind(rk::bind(addr)).touch(surface_kit::TOUCH_MIN) };
    let root = surface_kit::place(b, p, "switch.stomp_flat", &place).unwrap_or_else(|e| panic!("{e}"));
    // The nut is centred in the box's switch square: find it by the shadow.
    let at = b.scene.node(root).and_then(|n| n.children.iter().find_map(|c| b.scene.node(*c).filter(|k| k.name == "shadow").map(|k| (k.transform.tx + k.size.width / 2.0, k.transform.ty + k.size.height / 2.0))));
    let (ax, ay) = at.map_or((0.0, 0.0), |(x, y)| (x - 0.8 * d / 40.0, y - 6.0 * d / 40.0));
    parts::move_to(b, root, cx - ax, cy - ay);
    root
}

/// A fine brushed grain over a lid (`w × h` from its top-left), laid over
/// a satin paint: faint hairlines, not stripes.
pub fn brushing(b: &mut Builder<'_>, p: NodeId, (x, y, w, h): (f64, f64, f64, f64), seed: u64) {
    let mut s = seed | 1;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    for (c, a) in [("#ffffff", 0.06), ("#000000", 0.07)] {
        let mut d = String::new();
        let mut xx = rnd() * 1.5;
        while xx < w {
            let t = 0.25 + rnd() * 0.3;
            let (y0, y1) = (rnd() * h * 0.3, h - rnd() * h * 0.3);
            d.push_str(&format!("M {:.2} {:.2} h {t:.2} V {:.2} h {:.2} Z ", x + xx, y + y0, y + y1, -t));
            xx += 0.9 + rnd() * 1.6;
        }
        b.add(p, draw::with_opacity(draw::path("brushing", 0.0, 0.0, x + w, y + h, d, draw::solid(c)), a));
    }
}

/// A small LCD: a dark green-grey window in a black bezel, its reading
/// (the bound param, lit, in fourteen segments) centred.
#[allow(clippy::too_many_arguments)]
pub fn lcd(b: &mut Builder<'_>, p: NodeId, name: &str, addr: &str, (x, y, w, h): (f64, f64, f64, f64), size: f64, format: Option<ValueFormat>) -> NodeId {
    let win = parts::screen(b, p, name, (x, y, w, h), Screen { glass: ("#1c231f", "#27302a"), ink: "#d8e9dc", mesh: false });
    // Fourteen-segment characters over their unlit ghosts, as the LCDs read.
    let seg = rk::lcd::seg14(size * 0.82, "#d8e9dc").track(0.06);
    let cells = ((w - 6.0) / rk::lettering::set_width("~", &seg).max(1.0)).floor() as usize;
    let r = rk::lcd::readout(b, win, name, rk::bind(addr), (2.0, h / 2.0, w - 4.0), seg, TextAlign::Center, Some(("#d8e9dc14", cells)), format.unwrap_or_default());
    capitals(b, r);
    r
}

/// Print a readout's value in capitals, as the LCDs do ("500 MS").
pub fn capitals(b: &mut Builder<'_>, readout: NodeId) {
    let v = b.controls.get(&readout).and_then(|c| c.slots_of(SlotRole::Value).next()).map(NodeId);
    if let Some(v) = v {
        rk::case(b, v, true);
    }
}
