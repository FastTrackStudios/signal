//! A bat toggle mounted straight through the panel — no slot, no plate:
//! the chrome bat, its threaded bushing and a hex nut sitting on the paint,
//! with its two positions' legends printed above and below it (the grey opto
//! leveler's LIMIT / COMPRESS, the vari-mu's toggles, a valve limiter's ON).
//!
//! The bat is the `Thumb` in a lane centred on the bushing: it sits in the
//! lane's top half (leaning up) when the toggle param is on and slides to
//! the bottom half when it is off, so which way it leans is the moving part.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, cased, ink as parts_ink, lin, rad, text_w};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

fn bare_toggle(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let inkc = p.ink_color(PANEL_INK);
    let case = p.case();
    let d = p.size.unwrap_or(30.0).max(10.0);
    let k = d / 30.0;
    let [top, bottom] = match p.options.as_deref() {
        Some([a, b, ..]) => [a.clone(), b.clone()],
        _ => ["On".to_string(), "Off".to_string()],
    };
    let caption = p.legend.clone().unwrap_or_default();
    let size = (7.0 * k).max(6.5);
    let line = (size * 1.4).ceil();
    let widest = [&top, &bottom, &caption].iter().map(|s| text_w(s.trim(), size)).fold(d * 0.7, f64::max);
    let w = widest + 4.0 * k;
    let gap = 3.0 * k;
    let lever = d * 0.62;
    // A position with no legend takes no line: a face that prints its own
    // gets a box tight to the bat's travel.
    let above = if top.trim().is_empty() { 0.0 } else { line + gap };
    let below = if bottom.trim().is_empty() { 0.0 } else { gap + line };
    let (cx, cy) = (w / 2.0, above + lever);
    let under = if caption.trim().is_empty() { 0.0 } else { line };
    let h = cy + lever + below + under;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let ink = |name: &str, s: &str, y: f64| cased(parts_ink(name, s, 0.0, y, w, size, &inkc, TextAlign::Center, 600.0), case);
    b.add(root, ink("on", &top, line / 2.0));
    b.add(root, ink("off", &bottom, cy + lever + gap + line / 2.0));
    let label = b.add(root, ink("label", &caption, cy + lever + below + line / 2.0));

    // The hex nut on the paint, and the threaded bushing through it.
    let nut = draw::path("nut", 0.0, 0.0, w, h, draw::regular(cx, cy, d * 0.30, 6, 30.0), rad(0.34, 0.28, 0.95, &[(0.0, "#f4f4f0".into()), (0.55, "#b4b4ae".into()), (1.0, "#55554f".into())]));
    b.add(root, draw::with_effects(nut, vec![draw::drop_shadow("#00000073", 0.6 * k, 1.4 * k, 2.4 * k)]));
    let bushing = draw::circle("bushing", cx, cy, d * 0.19, rad(0.40, 0.34, 0.85, &[(0.0, "#d8d8d2".into()), (1.0, "#4a4a46".into())]));
    b.add(root, draw::stroked(bushing, "#0000008c", (0.6 * k).max(0.5)));
    b.add(root, draw::circle("bore", cx, cy, d * 0.085, draw::solid("#1a1a1a")));

    // The bat: a chrome lever from the bushing out to a rounded tip.
    let bw = d * 0.22;
    let lane = b.add(root, draw::frame("lane", cx - bw / 2.0, cy - lever, bw, 2.0 * lever));
    let paint: [&str; 4] = match p.variant.as_deref() {
        Some("black") => ["#060607", "#4a4a50", "#1e1e22", "#050506"],
        _ => ["#6f6d68", "#fbfaf6", "#c9c7c0", "#5d5b56"],
    };
    let bat = draw::rect("bat", 0.0, 0.0, bw, lever + bw * 0.4, lin(90.0, &[(0.0, paint[0].into()), (0.35, paint[1].into()), (0.62, paint[2].into()), (1.0, paint[3].into())]));
    let bat = draw::with_effects(draw::rounded(bat, bw / 2.0), vec![draw::drop_shadow("#000000a6", 1.0 * k, 2.2 * k, 3.0 * k)]);
    let bat_id = b.add(lane, bat);

    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, bat_id.0).slot(SlotRole::Label, label.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "switch.toggle_bare",
        family: Family::Switch,
        title: "Bare bat toggle",
        description: "A chrome bat toggle straight through the panel on a hex nut — no slot or plate — its two positions' legends printed above and below it (`Place::options` = [top, bottom], default On/Off) and `legend` under that. The bat leans up when the toggle param is on. Chrome or black bat. Vintage tube gear (a leveling amplifier's LIMIT/COMPRESS, POWER). Size = nut width ×≈3.",
        variants: &["chrome", "black"],
        default_size: 30.0,
        binds: "toggle param",
        build: bare_toggle,
    }]
}
