//! Motions on a running surface: driven by params and feeds, stepped by
//! `frame(t)`, kept across `swap_surface`, telling the host when to redraw.

use document_proto::motion::{Dynamics, Motion, Num, Target};
use document_proto::{Node, NodeId, NodeKind, Op, Scene, Size};
use surface_proto::{Catalog, EntryId, ParamAddr, ParamInfo, ParamValue, Range, StreamFrame, StreamInfo, StreamKind, Unit};
use surface_runtime::{Runtime, Surface};

fn catalog() -> Catalog {
    Catalog {
        id: "t".into(),
        version: 1,
        params: vec![
            ParamInfo::new("spring/decay", "Decay", Range::linear(0.2, 4.0, Unit::Seconds)),
            ParamInfo::new("tape/speed", "Speed", Range::default()),
        ],
        streams: vec![StreamInfo { id: EntryId::new("spring/in"), name: "In".into(), kind: StreamKind::Level { channels: 2 }, rate_hz: 60.0 }],
        ..Catalog::default()
    }
}

/// A coil struck by the input level, ringing for `spring/decay` seconds,
/// and a reel turning with `tape/speed`.
fn design(extra: bool, ids: &mut Vec<NodeId>) -> Surface {
    let mut scene = Scene::new("S");
    let page = scene.pages()[0];
    let mut add = |scene: &mut Scene, i: usize, name: &str, motion: Vec<Motion>| {
        let mut n = Node::new(NodeKind::Rectangle, name);
        if let Some(id) = ids.get(i) {
            n.id = *id;
        } else {
            ids.push(n.id);
        }
        n.size = Size { width: 40.0, height: 40.0 };
        n.motion = motion;
        scene.apply(Op::Insert { node: n, parent: page, index: u32::MAX }).unwrap();
    };
    let ring = Motion::new(Target::Y)
        .dynamics(Dynamics::Oscillator { freq: Num::Value(6.0), decay: Some(Num::Expr("param(spring/decay)".into())), damping: None, excite: "level(spring/in)".into(), gain: Num::Value(1.0) })
        .range(0.0, 10.0);
    add(&mut scene, 0, "coil", vec![ring]);
    add(&mut scene, 1, "reel", vec![Motion::new(Target::Rotate).input("norm(tape/speed)").dynamics(Dynamics::Spin).range(0.0, 360.0)]);
    if extra {
        add(&mut scene, 2, "lamp", vec![Motion::new(Target::Opacity).input("0.5")]);
    }
    Surface::from_scene(scene)
}

fn y(rt: &Runtime, id: NodeId, t: f64) -> f64 {
    rt.frame(t).node(id).unwrap().transform.ty
}

#[test]
fn a_level_burst_rings_the_coil_and_it_settles() {
    let mut ids = Vec::new();
    let mut rt = Runtime::new(design(false, &mut ids), catalog());
    rt.apply_values([ParamValue { addr: ParamAddr::new("spring/decay"), value: 0.0 }]); // 0.2 s
    rt.apply_frame(StreamFrame { id: EntryId::new("spring/in"), values: vec![-60.0, -60.0] }, 0.0);
    assert_eq!(y(&rt, ids[0], 0.0), 0.0);
    assert!(!rt.is_animating(), "silence and a stopped reel: nothing moves");
    rt.apply_frame(StreamFrame { id: EntryId::new("spring/in"), values: vec![-6.0, -12.0] }, 16.0);
    let mut peak: f64 = 0.0;
    for k in 1..=30 {
        peak = peak.max(y(&rt, ids[0], 16.0 * f64::from(k)).abs());
        if k == 3 {
            assert!(rt.is_animating(), "ringing: the host keeps drawing");
        }
    }
    assert!(peak > 3.0, "struck: {peak}");
    for k in 31..=90 {
        let _ = rt.frame(16.0 * f64::from(k));
    }
    assert!(y(&rt, ids[0], 16.0 * 91.0).abs() < 1e-3, "rang out in its 0.2 s decay");
    assert!(!rt.is_animating());
    // The reel turns with its param.
    rt.apply_values([ParamValue { addr: ParamAddr::new("tape/speed"), value: 0.5 }]);
    let turn = |t: f64| rt.frame(t).node(ids[1]).unwrap().transform.rotation().to_degrees();
    let a0 = turn(2000.0);
    let a = (turn(2100.0) - a0).rem_euclid(360.0);
    assert!((a - 18.0).abs() < 1e-6, "0.5 turn/s × 0.1 s = 18°: {a}");
    assert!(rt.is_animating());
}

#[test]
fn swap_surface_keeps_motion_state_for_nodes_still_there() {
    let mut ids = Vec::new();
    let mut rt = Runtime::new(design(true, &mut ids), catalog());
    rt.apply_values([ParamValue { addr: ParamAddr::new("spring/decay"), value: 1.0 }]); // 4 s
    rt.apply_frame(StreamFrame { id: EntryId::new("spring/in"), values: vec![-60.0, -60.0] }, 0.0);
    let _ = rt.frame(0.0);
    rt.apply_frame(StreamFrame { id: EntryId::new("spring/in"), values: vec![0.0, 0.0] }, 10.0);
    for k in 1..=10 {
        let _ = rt.frame(10.0 * f64::from(k));
    }
    let before = y(&rt, ids[0], 100.0);
    assert!(before.abs() > 0.5, "ringing: {before}");
    // An agent edits the document: the lamp is gone, the coil stays.
    let mut kept = ids.clone();
    kept.truncate(2);
    rt.swap_surface(design(false, &mut kept));
    assert_eq!(rt.motions(), 2);
    assert_eq!(y(&rt, ids[0], 100.0), before, "same instant, same state");
    assert!(y(&rt, ids[0], 116.0) != before && rt.is_animating(), "still ringing");
}
