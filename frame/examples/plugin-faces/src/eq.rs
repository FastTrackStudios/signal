//! Faces: the hardware EQs — the passive program equalizer, the console
//! channel EQ, the three-band proportional-Q module and the class-A
//! preamp/EQ module.
//!
//! Each is laid out from photographs of the unit it follows (the Pultec
//! EQP-1A, the SSL 4000 E channel EQ, the API 550A, the Neve 1073): panel
//! paint, knob styles, legends and positions measured as fractions of the
//! panel. Only the generic names are printed. What makes each one *that*
//! EQ is its controls — separate boost and cut on one band, swept bands
//! with fixed shapes, coarse stepped frequencies on concentric pairs — so
//! those are what the faces keep.

mod classa;
mod console;
mod program;
mod proportional;

use document_proto::NodeId;
use surface_kit::draw;
use surface_widgets::Builder;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![program::face(), console::face(), proportional::face(), classa::face()]
}

/// Repaint a placed panel: the unit's own paint, top to bottom.
fn paint(b: &mut Builder<'_>, panel: NodeId, stops: &[(f64, &str)]) {
    if let Some(n) = b.scene.node_mut(panel) {
        n.fills = vec![draw::linear(180.0, stops)];
    }
}

/// A slotted round-head panel screw of diameter `d`, the slot at `angle`.
fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, d: f64, angle: f64) {
    let head = draw::circle("screw", cx, cy, d / 2.0, draw::radial_at(0.36, 0.3, 0.85, &[(0.0, "#f1f1ed"), (0.45, "#b9bab5"), (1.0, "#5d5e5a")]));
    b.add(parent, draw::with_effects(head, vec![draw::drop_shadow("#00000080", 0.6, 1.2, 1.6)]));
    b.add(parent, draw::rotated(draw::rect("slot", cx - d * 0.38, cy - d * 0.07, d * 0.76, d * 0.14, draw::solid("#2b2b29d0")), angle));
}
