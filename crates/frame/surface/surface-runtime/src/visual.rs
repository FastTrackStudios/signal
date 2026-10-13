//! Control state → document ops on the control's slot nodes.
//!
//! The design is authored once (in frame, or imported from Figma); a
//! control only *adjusts* the nodes its slots name: it rotates a needle,
//! sweeps an arc, grows a fill, moves a thumb, writes a readout. The ops are
//! computed from the pristine design every frame and applied to a render
//! copy, so the authored scene is never mutated and nothing drifts.
//!
//! Conventions (documented in the authoring reference):
//! - A needle is drawn at the *center* of its sweep (pointing up for a
//!   standard knob) and rotates about the center of its parent.
//! - Fill/thumb/peak-hold nodes are drawn at *full* extent / *top* position
//!   and are scaled/moved along the control's axis within their parent. A
//!   gauge has no axis of its own: a fill grows along its long side, a thumb
//!   along its lane's.
//! - A gauge's or meter's `Led`s are a ladder: lamp `i` of `n` lights at
//!   position `(i + 1) / n`. An unlit `Led` also stops giving off light.
//! - A `Glow`'s opacity (and so its light) is the position.

use std::f64::consts::PI;

use document_proto::{Node, NodeId, NodeKind, Op, Prop, Scene, Size, TextData, Transform};
use surface_proto::{Control, ControlKind, Orientation, SlotRole};

use crate::plot::{self, Trace};

/// What a control currently shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ControlState {
    /// Control position, 0..1.
    pub position: f64,
    /// Neutral at center: arcs grow from the middle.
    pub bipolar: bool,
    /// Formatted value for `Value` slots.
    pub value_text: Option<String>,
    /// Parameter name for `Label` slots.
    pub label_text: Option<String>,
    /// Peak-hold position (meters), 0..1.
    pub peak: Option<f64>,
    /// Modulation extent (positions) for `ModArc` slots.
    pub mod_range: Option<(f64, f64)>,
    /// Overrides the on/off reading of `position` for `Led` slots (radio
    /// buttons are lit when *their* option is selected).
    pub lit: Option<bool>,
    /// A plot's or curve's points for `Trace` / `Area` / `Marker` slots.
    pub trace: Option<Trace>,
    /// A handle's place in its parent, unit space (`x` right, `y` up);
    /// `None` on an axis it doesn't move along.
    pub handle: Option<(Option<f64>, Option<f64>)>,
    /// Selection, hover, tint, text editing, marquee, menu highlight.
    pub ui: crate::visual_ui::Interaction,
}

impl ControlState {
    fn on(&self) -> bool {
        self.lit.unwrap_or(self.position >= 0.5)
    }
}

/// Ops that make `scene` show `state` for the control on `control_node`.
#[must_use]
pub fn control_ops(
    scene: &Scene,
    control_node: NodeId,
    control: &Control,
    state: &ControlState,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let axis = match control.kind {
        ControlKind::Fader { orientation } | ControlKind::Meter { orientation } => {
            Some(orientation)
        }
        _ => None,
    };
    // A gauge's lamps (Led slots, in order) light one by one: lamp i of n
    // at position (i + 1) / n. Its binding's curve puts each threshold there.
    let ladder = matches!(
        control.kind,
        ControlKind::Gauge { .. } | ControlKind::Meter { .. }
    );
    let lamps = control.slots_of(SlotRole::Led).count();
    let mut lamp = 0usize;
    let sweep = match control.kind {
        ControlKind::Knob { sweep_degrees } | ControlKind::Gauge { sweep_degrees, .. } => {
            sweep_degrees
        }
        _ => 270.0,
    };
    // A handle moves itself: centred on its point in its parent.
    if let (Some(uv), Some(node)) = (state.handle, scene.node(control_node)) {
        let parent = node
            .parent
            .and_then(|p| scene.node(p))
            .map_or(node.size, |p| p.size);
        let (tx, ty) = handle_offset(node, parent, uv);
        if (tx, ty) != (node.transform.tx, node.transform.ty) {
            ops.push(set(
                control_node,
                Prop::Transform(Transform {
                    tx,
                    ty,
                    ..node.transform
                }),
            ));
        }
    }
    for slot in &control.slots {
        let id = NodeId(slot.node);
        let Some(node) = scene.node(id) else { continue };
        let parent = node
            .parent
            .and_then(|p| scene.node(p))
            .map_or(node.size, |p| p.size);
        match slot.role {
            SlotRole::Trace | SlotRole::Area => {
                let d = state.trace.as_ref().filter(|t| !t.is_empty()).map(|t| {
                    let (w, h) = (node.size.width, node.size.height);
                    if slot.role == SlotRole::Trace {
                        let dot = (node.stroke.weight * 1.2).max(1.5);
                        plot::line_path(&t.points, w, h, t.style, dot)
                    } else {
                        plot::area_path(&t.points, w, h, t.style, t.baseline)
                    }
                });
                match d.filter(|d| !d.is_empty()) {
                    Some(d) => {
                        if !matches!(node.kind, NodeKind::Vector) {
                            ops.push(set(id, Prop::Kind(NodeKind::Vector)));
                        }
                        ops.push(set(id, Prop::FillGeometry(vec![d])));
                        if !node.stroke_geometry.is_empty() {
                            ops.push(set(id, Prop::StrokeGeometry(Vec::new())));
                        }
                    }
                    None => ops.push(set(id, Prop::Visible(false))),
                }
            }
            SlotRole::Marker => match state.trace.as_ref().and_then(|t| t.marker) {
                Some((u, v)) => {
                    let (tx, ty) = handle_offset(node, parent, (Some(u), Some(v)));
                    ops.push(set(
                        id,
                        Prop::Transform(Transform {
                            tx,
                            ty,
                            ..node.transform
                        }),
                    ));
                }
                None => ops.push(set(id, Prop::Visible(false))),
            },
            SlotRole::Needle => {
                let angle = (state.position - 0.5) * sweep.to_radians();
                let pivot = (parent.width * 0.5, parent.height * 0.5);
                ops.push(set(
                    id,
                    Prop::Transform(rotate_about(node.transform, angle, pivot)),
                ));
            }
            SlotRole::ValueArc => {
                let (from, to) = if state.bipolar {
                    (0.5, state.position)
                } else {
                    (0.0, state.position)
                };
                arc_ops(&mut ops, node, sweep, from, to);
            }
            SlotRole::ModArc => match state.mod_range {
                Some((lo, hi)) => arc_ops(&mut ops, node, sweep, lo, hi),
                None => ops.push(set(id, Prop::Visible(false))),
            },
            // Without a stated orientation (a gauge's bar, an LED row), a
            // fill grows along its own long side and a thumb travels along
            // its lane's.
            SlotRole::Fill => fill_ops(
                &mut ops,
                node,
                axis.unwrap_or_else(|| long_axis(node.size)),
                state.position,
            ),
            SlotRole::Thumb => {
                ops.push(set(
                    id,
                    Prop::Transform(along(
                        node,
                        parent,
                        axis.unwrap_or_else(|| long_axis(parent)),
                        state.position,
                    )),
                ));
            }
            SlotRole::PeakHold => match state.peak {
                Some(p) => ops.push(set(
                    id,
                    Prop::Transform(along(
                        node,
                        parent,
                        axis.unwrap_or_else(|| long_axis(parent)),
                        p,
                    )),
                )),
                None => ops.push(set(id, Prop::Visible(false))),
            },
            SlotRole::Glow => {
                let opacity = node.opacity * state.position.clamp(0.0, 1.0);
                ops.push(set(id, Prop::Opacity(opacity)));
            }
            SlotRole::Value => {
                // A text being edited shows its buffer; an empty text hides.
                if let Some(e) = &state.ui.edit {
                    text_ops(&mut ops, node, &e.text);
                } else if state.ui.empty {
                    ops.push(set(id, Prop::Visible(false)));
                } else if let Some(text) = &state.value_text {
                    text_ops(&mut ops, node, text);
                }
            }
            SlotRole::Label => {
                // Authored labels win; empty or `{…}` placeholders get the name.
                let authored = node
                    .text()
                    .is_some_and(|t| !t.characters.is_empty() && !t.characters.starts_with('{'));
                if let (false, Some(text)) = (authored, &state.label_text) {
                    text_ops(&mut ops, node, text);
                }
            }
            SlotRole::Selected
            | SlotRole::Hovered
            | SlotRole::Caret
            | SlotRole::TextSelection
            | SlotRole::Placeholder
            | SlotRole::Marquee
            | SlotRole::Highlight
            | SlotRole::Tint
            | SlotRole::On
            | SlotRole::Off => {
                crate::visual_ui::slot_ops(&mut ops, node, slot.role, state.on(), &state.ui)
            }
            SlotRole::Led => {
                let on = if ladder && lamps > 0 {
                    let at = (lamp + 1) as f64 / lamps as f64;
                    lamp += 1;
                    state.position >= at - 1e-9
                } else {
                    state.on()
                };
                let opacity = if on { node.opacity } else { node.opacity * 0.2 };
                ops.push(set(id, Prop::Opacity(opacity)));
                // An unlit lamp is a dark lens, not a dim light.
                if !on && node.emission.is_some() {
                    ops.push(set(id, Prop::Emission(None)));
                }
            }
        }
    }
    ops
}

/// Where a node centred on unit point `(u, v)` of its parent (`v` up) sits:
/// its translation. An axis given as `None` keeps the authored offset.
#[must_use]
pub fn handle_offset(node: &Node, parent: Size, (u, v): (Option<f64>, Option<f64>)) -> (f64, f64) {
    let tx = u.map_or(node.transform.tx, |u| {
        u.clamp(0.0, 1.0) * parent.width - node.size.width / 2.0
    });
    let ty = v.map_or(node.transform.ty, |v| {
        (1.0 - v.clamp(0.0, 1.0)) * parent.height - node.size.height / 2.0
    });
    (tx, ty)
}

/// The axis along a box's longer side (vertical for a square).
fn long_axis(size: Size) -> Orientation {
    if size.width > size.height {
        Orientation::Horizontal
    } else {
        Orientation::Vertical
    }
}

fn set(id: NodeId, prop: Prop) -> Op {
    Op::Set { id, prop }
}

/// `R(angle)` about `pivot` (parent space), applied after `t`.
fn rotate_about(t: Transform, angle: f64, (px, py): (f64, f64)) -> Transform {
    let (s, c) = angle.sin_cos();
    // M = T(p) · R · T(-p) · t
    let r = |x: f64, y: f64| (c * x - s * y, s * x + c * y);
    let (a, cc) = r(t.a, t.c);
    let (b, d) = r(t.b, t.d);
    let (rx, ry) = r(t.tx - px, t.ty - py);
    Transform {
        a,
        b,
        tx: rx + px,
        c: cc,
        d,
        ty: ry + py,
    }
}

/// Where a thumb/marker sits for `position` along the axis of its parent.
fn along(node: &Node, parent: Size, axis: Orientation, position: f64) -> Transform {
    let p = position.clamp(0.0, 1.0);
    let mut t = node.transform;
    match axis {
        Orientation::Vertical => t.ty = (parent.height - node.size.height).max(0.0) * (1.0 - p),
        Orientation::Horizontal => t.tx = (parent.width - node.size.width).max(0.0) * p,
    }
    t
}

/// Grow a fill from the axis origin (bottom / left).
fn fill_ops(ops: &mut Vec<Op>, node: &Node, axis: Orientation, position: f64) {
    let p = position.clamp(0.0, 1.0);
    let mut size = node.size;
    let mut t = node.transform;
    match axis {
        Orientation::Vertical => {
            size.height = node.size.height * p;
            t.ty = node.transform.ty + node.size.height - size.height;
        }
        Orientation::Horizontal => size.width = node.size.width * p,
    }
    ops.push(set(node.id, Prop::Size(size)));
    ops.push(set(node.id, Prop::Transform(t)));
    ops.push(set(node.id, Prop::Visible(p > 0.0 && node.visible)));
}

/// Replace a node's outline with an annular sector for positions `from..to`.
fn arc_ops(ops: &mut Vec<Op>, node: &Node, sweep_degrees: f64, from: f64, to: f64) {
    let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
    let start = -sweep_degrees.to_radians() * 0.5;
    let a0 = start + lo.clamp(0.0, 1.0) * sweep_degrees.to_radians();
    let a1 = start + hi.clamp(0.0, 1.0) * sweep_degrees.to_radians();
    if a1 - a0 < 1e-4 {
        ops.push(set(node.id, Prop::Visible(false)));
        return;
    }
    let (w, h) = (node.size.width, node.size.height);
    let outer = w.min(h) * 0.5;
    let thickness = if node.stroke.paints.iter().any(|p| p.visible) && node.stroke.weight > 0.0 {
        node.stroke.weight
    } else {
        (outer * 0.14).max(2.0)
    };
    let inner = (outer - thickness).max(0.0);
    let d = annular_sector(w * 0.5, h * 0.5, inner, outer, a0, a1);
    if !matches!(node.kind, NodeKind::Vector) {
        ops.push(set(node.id, Prop::Kind(NodeKind::Vector)));
    }
    ops.push(set(node.id, Prop::FillGeometry(vec![d])));
    ops.push(set(node.id, Prop::StrokeGeometry(Vec::new())));
    ops.push(set(node.id, Prop::Visible(node.visible)));
}

/// SVG path of a ring sector; angles in radians, 0 = up, clockwise.
fn annular_sector(cx: f64, cy: f64, r_in: f64, r_out: f64, a0: f64, a1: f64) -> String {
    let pt = |r: f64, a: f64| (cx + r * a.sin(), cy - r * a.cos());
    let large = i32::from(a1 - a0 > PI);
    let (x0, y0) = pt(r_out, a0);
    let (x1, y1) = pt(r_out, a1);
    let (x2, y2) = pt(r_in, a1);
    let (x3, y3) = pt(r_in, a0);
    format!(
        "M {x0:.3} {y0:.3} A {r_out:.3} {r_out:.3} 0 {large} 1 {x1:.3} {y1:.3} L {x2:.3} {y2:.3} A {r_in:.3} {r_in:.3} 0 {large} 0 {x3:.3} {y3:.3} Z"
    )
}

/// Swap a text node's characters (keeping its style; dropping exported
/// glyph outlines so the new text is shaped live).
fn text_ops(ops: &mut Vec<Op>, node: &Node, text: &str) {
    let mut data: TextData = node.text().cloned().unwrap_or_default();
    if data.characters == text && node.fill_geometry.is_empty() {
        return;
    }
    data.characters = text.to_string();
    if let Some(first) = data.runs.first().cloned() {
        data.runs = vec![document_proto::TextRun {
            text: text.to_string(),
            ..first
        }];
    }
    ops.push(set(node.id, Prop::Kind(NodeKind::Text(data))));
    if !node.fill_geometry.is_empty() {
        ops.push(set(node.id, Prop::FillGeometry(Vec::new())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_proto::{Color, Paint};

    fn knob_scene() -> (Scene, NodeId, Control) {
        let mut scene = Scene::new("p");
        let page = scene.pages()[0];
        let mut body = Node::new(NodeKind::Frame, "Knob");
        body.size = Size {
            width: 64.0,
            height: 64.0,
        };
        let body_id = body.id;
        scene
            .apply(Op::Insert {
                node: body,
                parent: page,
                index: 0,
            })
            .unwrap();
        let mut needle = Node::new(NodeKind::Rectangle, "needle");
        needle.size = Size {
            width: 4.0,
            height: 24.0,
        };
        needle.transform = Transform::translate(30.0, 8.0);
        let needle_id = needle.id;
        scene
            .apply(Op::Insert {
                node: needle,
                parent: body_id,
                index: 0,
            })
            .unwrap();
        let mut arc = Node::new(NodeKind::Ellipse, "arc");
        arc.size = Size {
            width: 64.0,
            height: 64.0,
        };
        arc.fills = vec![Paint::solid(Color::rgba(1.0, 0.4, 0.2, 1.0))];
        let arc_id = arc.id;
        scene
            .apply(Op::Insert {
                node: arc,
                parent: body_id,
                index: 0,
            })
            .unwrap();
        let mut value = Node::new(
            NodeKind::Text(TextData {
                characters: "0".into(),
                ..TextData::default()
            }),
            "value",
        );
        value.fill_geometry = vec!["M0 0Z".into()];
        let value_id = value.id;
        scene
            .apply(Op::Insert {
                node: value,
                parent: body_id,
                index: 0,
            })
            .unwrap();
        let control = Control::new(ControlKind::knob())
            .slot(SlotRole::Needle, needle_id.0)
            .slot(SlotRole::ValueArc, arc_id.0)
            .slot(SlotRole::Value, value_id.0);
        (scene, body_id, control)
    }

    #[test]
    fn needle_rotates_about_parent_center() {
        let (scene, body, control) = knob_scene();
        let mut s = scene.clone();
        let ops = control_ops(
            &scene,
            body,
            &control,
            &ControlState {
                position: 1.0,
                ..Default::default()
            },
        );
        s.transact(ops).unwrap();
        let needle = s.nodes.values().find(|n| n.name == "needle").unwrap();
        assert!(
            (needle.transform.rotation() - 135f64.to_radians()).abs() < 1e-9,
            "full = +135° from center"
        );
        // Needle top-left (30, 8) rotated 135° cw about (32, 32).
        let (s135, c135) = 135f64.to_radians().sin_cos();
        let (dx, dy) = (30.0 - 32.0, 8.0 - 32.0);
        assert!((needle.transform.tx - (c135 * dx - s135 * dy + 32.0)).abs() < 1e-9);
        assert!((needle.transform.ty - (s135 * dx + c135 * dy + 32.0)).abs() < 1e-9);
    }

    #[test]
    fn center_position_leaves_needle_as_authored() {
        let (scene, body, control) = knob_scene();
        let ops = control_ops(
            &scene,
            body,
            &control,
            &ControlState {
                position: 0.5,
                ..Default::default()
            },
        );
        let needle = scene.nodes.values().find(|n| n.name == "needle").unwrap();
        let t = ops.iter().find_map(|o| match o {
            Op::Set {
                id,
                prop: Prop::Transform(t),
            } if *id == needle.id => Some(*t),
            _ => None,
        });
        let t = t.unwrap();
        assert!(
            (t.tx - 30.0).abs() < 1e-9 && (t.ty - 8.0).abs() < 1e-9 && (t.a - 1.0).abs() < 1e-12
        );
    }

    #[test]
    fn arc_becomes_a_sector_and_hides_at_zero() {
        let (scene, body, control) = knob_scene();
        let arc = scene.nodes.values().find(|n| n.name == "arc").unwrap().id;
        let ops = control_ops(
            &scene,
            body,
            &control,
            &ControlState {
                position: 0.6,
                ..Default::default()
            },
        );
        assert!(ops.iter().any(|o| matches!(o, Op::Set { id, prop: Prop::FillGeometry(g) } if *id == arc && g[0].starts_with('M'))));
        assert!(ops.iter().any(
            |o| matches!(o, Op::Set { id, prop: Prop::Kind(NodeKind::Vector) } if *id == arc)
        ));
        let ops = control_ops(
            &scene,
            body,
            &control,
            &ControlState {
                position: 0.0,
                ..Default::default()
            },
        );
        assert!(ops.contains(&Op::Set {
            id: arc,
            prop: Prop::Visible(false)
        }));
        // Bipolar at center: nothing to draw.
        let ops = control_ops(
            &scene,
            body,
            &control,
            &ControlState {
                position: 0.5,
                bipolar: true,
                ..Default::default()
            },
        );
        assert!(ops.contains(&Op::Set {
            id: arc,
            prop: Prop::Visible(false)
        }));
    }

    #[test]
    fn value_text_replaces_outlines() {
        let (scene, body, control) = knob_scene();
        let mut s = scene.clone();
        let ops = control_ops(
            &scene,
            body,
            &control,
            &ControlState {
                position: 0.3,
                value_text: Some("-6.0 dB".into()),
                ..Default::default()
            },
        );
        s.transact(ops).unwrap();
        let v = s.nodes.values().find(|n| n.name == "value").unwrap();
        assert_eq!(v.text().unwrap().characters, "-6.0 dB");
        assert!(v.fill_geometry.is_empty());
    }

    #[test]
    fn fader_fill_and_thumb() {
        let mut scene = Scene::new("p");
        let page = scene.pages()[0];
        let mut track = Node::new(NodeKind::Frame, "fader");
        track.size = Size {
            width: 20.0,
            height: 200.0,
        };
        let track_id = track.id;
        scene
            .apply(Op::Insert {
                node: track,
                parent: page,
                index: 0,
            })
            .unwrap();
        let mut fill = Node::new(NodeKind::Rectangle, "fill");
        fill.size = Size {
            width: 20.0,
            height: 200.0,
        };
        let fill_id = fill.id;
        scene
            .apply(Op::Insert {
                node: fill,
                parent: track_id,
                index: 0,
            })
            .unwrap();
        let mut thumb = Node::new(NodeKind::Rectangle, "thumb");
        thumb.size = Size {
            width: 20.0,
            height: 10.0,
        };
        let thumb_id = thumb.id;
        scene
            .apply(Op::Insert {
                node: thumb,
                parent: track_id,
                index: 1,
            })
            .unwrap();
        let control = Control::new(ControlKind::fader())
            .slot(SlotRole::Fill, fill_id.0)
            .slot(SlotRole::Thumb, thumb_id.0);
        let mut s = scene.clone();
        s.transact(control_ops(
            &scene,
            track_id,
            &control,
            &ControlState {
                position: 0.25,
                ..Default::default()
            },
        ))
        .unwrap();
        let fill = s.node(fill_id).unwrap();
        assert_eq!(fill.size.height, 50.0);
        assert_eq!(fill.transform.ty, 150.0);
        assert_eq!(
            s.node(thumb_id).unwrap().transform.ty,
            142.5,
            "(200-10)·(1-0.25)"
        );
    }
}
