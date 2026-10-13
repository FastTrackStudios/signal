//! `lever.eq_slider`: a graphic EQ's band slider seen straight on — a
//! deep, narrow slot with turned metal lips and a short cap riding in it
//! that is lit from inside (a lamp-cap graphic EQ) or a plain moulding.
//!
//! ```text
//! <root control=Fader{Vertical}>   the touch box round the slot
//!   slot: lips, the dark channel
//!   lane (the travel)
//!     cap ← Thumb (drawn at the top; the runtime slides it down)
//! ```
//!
//! Size = the slot's length. Variants: `pearl` (a white cap with a warm
//! glow), `lit` (a translucent cap glowing `color=#hex`, default blue),
//! `dark` (a black cap, unlit). Options: `w=` slot width, `cap_h=` cap
//! height.

use document_proto::NodeId;
use surface_proto::{ControlKind, Orientation, SlotRole};
use surface_widgets::Builder;

use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

fn num(p: &Place, key: &str) -> Option<f64> {
    p.opt(key).and_then(|v| v.parse().ok())
}

fn eq_slider(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let len = p.size.unwrap_or(140.0).max(40.0);
    let sw = num(p, "w").unwrap_or(14.0);
    let cap_h = num(p, "cap_h").unwrap_or(sw * 1.9);
    let cap_w = sw * 0.72;
    let bw = p.touch.unwrap_or(0.0).max(sw + 12.0);
    let name = if p.name.is_empty() { "Band" } else { p.name.as_str() };
    let root = b.add(parent, draw::frame(name, 0.0, 0.0, bw, len));
    let x = (bw - sw) / 2.0;
    // The slot: turned lips round a dark channel.
    let lips = draw::rounded(draw::rect("slot lips", x, 0.0, sw, len, draw::linear(90.0, &[(0.0, "#8d9096"), (0.25, "#e6e7ea"), (0.5, "#6d7076"), (0.75, "#d9dadd"), (1.0, "#7b7e84")])), sw / 2.0);
    b.add(root, draw::with_effects(lips, vec![draw::drop_shadow("#00000059", 0.0, 1.0, 2.0)]));
    let ch = sw * 0.42;
    let chan = draw::rounded(draw::rect("channel", x + (sw - ch) / 2.0, 3.0, ch, len - 6.0, draw::linear(90.0, &[(0.0, "#050506"), (0.5, "#242528"), (1.0, "#050506")])), ch / 2.0);
    b.add(root, draw::with_effects(chan, vec![draw::inner_shadow("#000000", 0.0, 2.0, 3.0)]));
    // The lane the cap travels (its box = the travel plus the cap).
    let lane = b.add(root, draw::frame("lane", (bw - cap_w) / 2.0, 2.0, cap_w, len - 4.0));
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap_h));
    let variant = p.variant.as_deref().unwrap_or("pearl");
    let color = p.lamp_color(0).unwrap_or_else(|| "#6fb6ff".to_string());
    match variant {
        "lit" => {
            let body = draw::rounded(draw::rect("cap body", 0.0, 0.0, cap_w, cap_h, draw::linear(90.0, &[(0.0, &crate::controls::parts::shade(&color, 0.2)), (0.5, &crate::controls::parts::tint(&color, 0.55)), (1.0, &crate::controls::parts::shade(&color, 0.2))])), 2.0);
            b.add(cap, draw::emissive(draw::stroked(body, &crate::controls::parts::tint(&color, 0.7), 0.6), draw::hex(&color), 0.9, cap_w * 1.6));
            b.add(cap, draw::rect("cap line", 0.0, cap_h / 2.0 - 0.4, cap_w, 0.8, draw::solid("#ffffffaa")));
        }
        "dark" => {
            b.add(cap, draw::rounded(draw::rect("cap body", 0.0, 0.0, cap_w, cap_h, draw::linear(90.0, &[(0.0, "#141416"), (0.5, "#4a4b50"), (1.0, "#141416")])), 2.0));
            b.add(cap, draw::rect("cap line", 0.0, cap_h / 2.0 - 0.5, cap_w, 1.0, draw::solid("#e8e9ec")));
        }
        _ => {
            let body = draw::rounded(draw::rect("cap body", 0.0, 0.0, cap_w, cap_h, draw::linear(90.0, &[(0.0, "#d9d6d0"), (0.35, "#ffffff"), (0.7, "#f3f1ec"), (1.0, "#c9c5be")])), cap_w * 0.3);
            b.add(cap, draw::emissive(draw::stroked(body, "#b9b4ab", 0.5), draw::hex("#ffd9b0"), 0.35, cap_w));
        }
    }
    let ctl = p.control(ControlKind::Fader { orientation: Orientation::Vertical }).slot(SlotRole::Thumb, cap.0);
    b.controls.insert(root, ctl);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "lever.eq_slider",
        family: Family::Lever,
        title: "Graphic EQ slider",
        description: "A graphic EQ band slider seen straight on: a deep narrow slot with turned metal lips and a short cap riding in it — `pearl` (white, a warm glow), `lit` (a translucent cap glowing `color=#hex`, default blue — lamp-cap EQs), `dark` (black, a white line). Vertical `Fader`. Size = slot length; `w=` slot width, `cap_h=` cap height.",
        variants: &["pearl", "lit", "dark"],
        default_size: 140.0,
        binds: "param",
        build: eq_slider,
    }]
}
