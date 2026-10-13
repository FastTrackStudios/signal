//! A Needle slot on a *frame* turns the whole subtree: the renderer inherits
//! the frame's rotation into its children, so a knob body (knurl, notch,
//! numerals…) can live in one rotating frame instead of one flattened vector.

use frame::document::proto::{Color, Node, NodeId, NodeKind, Op, Paint, Scene, Size, Transform};
use frame::surface::proto::{Control, ControlKind, SlotRole};
use frame::surface::runtime::{ControlState, visual::control_ops};

fn insert(scene: &mut Scene, parent: NodeId, node: Node) -> NodeId {
    let id = node.id;
    scene
        .apply(Op::Insert {
            node,
            parent,
            index: u32::MAX,
        })
        .unwrap();
    id
}

/// Knob (64×64) ⊃ pivot frame (the Needle slot, 64×64) ⊃ grey body + red notch
/// at 12 o'clock. Returns the scene, the knob and its control.
fn knob() -> (Scene, NodeId, NodeId, Control) {
    let mut scene = Scene::new("Stage");
    let page = scene.pages()[0];
    let mut knob = Node::new(NodeKind::Frame, "Knob");
    knob.size = Size {
        width: 64.0,
        height: 64.0,
    };
    let knob_id = insert(&mut scene, page, knob);
    let mut pivot = Node::new(NodeKind::Frame, "pivot");
    pivot.size = Size {
        width: 64.0,
        height: 64.0,
    };
    let pivot_id = insert(&mut scene, knob_id, pivot);
    let mut body = Node::new(NodeKind::Ellipse, "body");
    body.size = Size {
        width: 64.0,
        height: 64.0,
    };
    body.fills = vec![Paint::solid(Color::rgba(0.3, 0.3, 0.3, 1.0))];
    insert(&mut scene, pivot_id, body);
    let mut notch = Node::new(NodeKind::Rectangle, "notch");
    notch.size = Size {
        width: 4.0,
        height: 12.0,
    };
    notch.transform = Transform::translate(30.0, 4.0);
    notch.fills = vec![Paint::solid(Color::rgba(1.0, 0.0, 0.0, 1.0))];
    let notch_id = insert(&mut scene, pivot_id, notch);
    let control = Control::new(ControlKind::knob()).slot(SlotRole::Needle, pivot_id.0);
    (scene, knob_id, notch_id, control)
}

/// The scene showing `position` (0..1 over a 270° sweep).
fn at(position: f64) -> (Scene, NodeId) {
    let (scene, knob_id, notch_id, control) = knob();
    let mut frame = scene.clone();
    frame
        .transact(control_ops(
            &scene,
            knob_id,
            &control,
            &ControlState {
                position,
                ..ControlState::default()
            },
        ))
        .unwrap();
    // The slot op rotates the pivot frame only; the notch keeps its authored transform.
    assert_eq!(
        frame.node(notch_id).unwrap().transform,
        Transform::translate(30.0, 4.0)
    );
    (frame, notch_id)
}

/// Which of `points` are red in a 64×64 render; `None` without a GPU.
fn red_at(scene: &Scene, points: &[(u32, u32)]) -> Option<Vec<bool>> {
    let mut vs = vello::Scene::new();
    frame::render::paint_scene_into(
        &mut vs,
        scene,
        scene.pages()[0],
        kurbo::Affine::IDENTITY,
        None,
    );
    let Ok(mut offscreen) = frame::render::offscreen::Offscreen::new() else {
        eprintln!("skipping pixel check: no GPU adapter");
        return None;
    };
    let px = offscreen
        .render(&vs, 64, 64, peniko::Color::from_rgba8(0, 0, 0, 255))
        .expect("render");
    Some(
        points
            .iter()
            .map(|&(x, y)| {
                let i = ((y * px.width + x) * 4) as usize;
                px.rgba[i] > 200 && px.rgba[i + 1] < 50 && px.rgba[i + 2] < 50
            })
            .collect(),
    )
}

#[test]
fn needle_slot_on_a_frame_rotates_the_notch_inside_it() {
    // Centered: notch at 12 o'clock, x ∈ [30, 34], y ∈ [4, 16].
    let (rest, _) = at(0.5);
    // +90° (clockwise on screen) about (32, 32): notch at 3 o'clock, x ∈ [48, 60], y ∈ [30, 34].
    let (turned, _) = at(0.5 + 90.0 / 270.0);

    // The primitives carry the rotation as a group around the pivot's subtree.
    let groups = |s: &Scene| {
        frame::render::build_scene_primitives(s, s.pages()[0])
            .into_iter()
            .filter(|p| matches!(p, frame::render::PaintPrimitive::GroupStart { .. }))
            .count()
    };
    assert_eq!(groups(&rest), 0, "the centered needle is untransformed");
    assert_eq!(groups(&turned), 1);

    if let (Some(before), Some(after)) = (
        red_at(&rest, &[(32, 10), (54, 32)]),
        red_at(&turned, &[(32, 10), (54, 32)]),
    ) {
        assert_eq!(before, vec![true, false], "notch at 12 o'clock");
        assert_eq!(
            after,
            vec![false, true],
            "notch turned to 3 o'clock with its frame"
        );
    }
}
