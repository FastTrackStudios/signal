//! `knob.flat_line`: the flat software knob of a modern amp-sim's chrome —
//! a pale disc read by one dark line from the centre to the rim, inside a
//! soft track ring. For the utility controls round a hardware page (mic
//! position, distance, level, pan; input and output trims).

use document_proto::{NodeId, NodeKind, TextAlign};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use crate::controls::parts::{alpha, shade};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

fn flat_line(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(48.0).max(16.0);
    let r = d / 2.0;
    let k = d / 48.0;
    let sweep = p.sweep.unwrap_or(270.0);
    let (disc, line, track) = match p.variant.as_deref() {
        Some("dark") => ("#2c2d31", "#f2f2f0", "#ffffff2e"),
        _ => ("#f4f1ee", "#1d1d1f", "#0000001f"),
    };
    let track = p.opt("track").filter(|t| t.starts_with('#')).unwrap_or(track).to_string();
    let ring = r + 5.0 * k;
    let side = 2.0 * (ring + 3.0 * k);
    let c = side / 2.0;
    let name = if p.name.is_empty() { "Flat knob" } else { p.name.as_str() };
    let legend_size = (d * 0.2).max(8.0);
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, side, side + legend_size * 1.6));
    let knob = b.add(root, draw::frame("knob", 0.0, 0.0, side, side));
    b.add(knob, draw::path("track", 0.0, 0.0, side, side, draw::sector(c, c, ring - 2.6 * k, ring, -sweep / 2.0, sweep / 2.0), draw::solid(&track)));
    let face = draw::circle("disc", c, c, r, draw::radial_at(0.5, 0.4, 0.7, &[(0.0, disc), (1.0, &shade(disc, 0.06))]));
    b.add(knob, draw::with_effects(face, vec![draw::drop_shadow(&alpha("#000000", 0.18), 0.0, 1.5 * k, 3.0 * k)]));
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    let lw = (2.2 * k).max(1.0);
    b.add(body, draw::rounded(draw::rect("index", c - lw / 2.0, c - r * 0.92, lw, r * 0.92, draw::solid(line)), lw / 2.0));
    let w = side * 2.0;
    let mut t = draw::text("legend", p.legend.as_deref().unwrap_or(""), c - w / 2.0, side + legend_size * 0.2, w, legend_size, &p.ink_color(line), TextAlign::Center);
    if let NodeKind::Text(td) = &mut t.kind {
        td.font_weight = Some(600.0);
        td.text_case = p.case();
    }
    let legend = b.add(root, t);
    let ctl = p.control(ControlKind::Knob { sweep_degrees: sweep }).slot(SlotRole::Needle, body.0).slot(SlotRole::Label, legend.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        crate::registry::tighten(b, root, (c, c), d, touch);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "knob.flat_line",
        family: Family::Knob,
        title: "Flat line",
        description: "The flat knob of a modern amp-sim's own chrome: a pale (`light`) or `dark` disc read by one line from centre to rim, inside a soft track ring (`track=#hex`). For utility controls beside hardware art — mic position, distance, level, pan, trims. Size = disc diameter.",
        variants: &["light", "dark"],
        default_size: 48.0,
        binds: "param",
        build: flat_line,
    }]
}
