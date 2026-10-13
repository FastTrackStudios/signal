//! A knob over a stepped param clicks between its positions: dragging,
//! the wheel and the arrow keys only ever land on a detent.

use std::collections::BTreeMap;

use frame::document::proto::{Node, NodeId, NodeKind, Op, Scene, Size, Transform};
use frame::surface::proto::{
    Binding, Catalog, Control, ControlKind, ParamAddr, ParamInfo, Range, SlotRole, Unit,
};
use frame::surface::runtime::{Bounds, EngineCall, Input, Key, Modifiers, Runtime, Surface};

fn knob_on(param: ParamInfo) -> (Runtime, NodeId) {
    let mut scene = Scene::new("s");
    let page = scene.pages()[0];
    let mut n = Node::new(NodeKind::Frame, "knob");
    n.size = Size {
        width: 60.0,
        height: 60.0,
    };
    n.transform = Transform::translate(0.0, 0.0);
    let id = n.id;
    scene
        .apply(Op::Insert {
            node: n,
            parent: page,
            index: u32::MAX,
        })
        .unwrap();
    let addr = param.addr.clone();
    let control = Control::new(ControlKind::knob())
        .slot(SlotRole::Needle, id.0)
        .bound(Binding::param(addr.0.clone()));
    let controls: BTreeMap<NodeId, Control> = [(id, control)].into_iter().collect();
    let catalog = Catalog {
        params: vec![param],
        ..Catalog::default()
    };
    let mut rt = Runtime::new(Surface { scene, controls }, catalog);
    rt.set_boxes([(
        id,
        Bounds {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 60.0,
        },
    )]);
    (rt, id)
}

fn sent(calls: &[EngineCall]) -> Vec<f64> {
    calls
        .iter()
        .filter_map(|c| {
            if let EngineCall::SetParam { value, .. } = c {
                Some(*value)
            } else {
                None
            }
        })
        .collect()
}

fn value(rt: &Runtime, addr: &str) -> f64 {
    rt.values()
        .iter()
        .find(|v| v.addr == ParamAddr::new(addr))
        .map_or(f64::NAN, |v| v.value)
}

fn on_detent(v: f64, n: u32) -> bool {
    let k = f64::from(n - 1);
    ((v * k).round() / k - v).abs() < 1e-9
}

const M: Modifiers = Modifiers {
    fine: false,
    alt: false,
    option: false,
};

#[test]
fn dragging_a_three_way_switch_only_lands_on_its_positions() {
    let (mut rt, id) =
        knob_on(ParamInfo::new("sw", "Switch", Range::default()).choice(&["A", "B", "C"]));
    let mut all = Vec::new();
    all.extend(sent(&rt.feed(
        id,
        Input::Down {
            x: 30.0,
            y: 100.0,
            t: 0.0,
            mods: M,
        },
        0.0,
    )));
    for i in 1..=150 {
        let y = 100.0 - f64::from(i) * 3.0;
        all.extend(sent(&rt.feed(
            id,
            Input::Move {
                x: 30.0,
                y,
                t: 0.0,
                mods: M,
            },
            0.0,
        )));
        assert!(
            on_detent(value(&rt, "sw"), 3),
            "shown value between detents: {}",
            value(&rt, "sw")
        );
    }
    all.extend(sent(&rt.feed(
        id,
        Input::Up {
            x: 30.0,
            y: -20.0,
            t: 1.0,
        },
        1.0,
    )));
    all.extend(sent(&rt.poll(2.0)));
    assert!(
        all.iter().all(|v| on_detent(*v, 3)),
        "sent values between detents: {all:?}"
    );
    assert_eq!(
        value(&rt, "sw"),
        1.0,
        "a long drag up reaches the last position"
    );
}

#[test]
fn wheel_and_arrows_move_one_detent_at_a_time() {
    let (mut rt, id) =
        knob_on(ParamInfo::new("sw", "Switch", Range::default()).choice(&["A", "B", "C"]));
    rt.feed(
        id,
        Input::Wheel {
            notches: 1.0,
            mods: M,
        },
        0.0,
    );
    assert_eq!(value(&rt, "sw"), 0.5);
    rt.feed(
        id,
        Input::Key {
            key: Key::Up,
            mods: M,
        },
        0.0,
    );
    assert_eq!(value(&rt, "sw"), 1.0);
    rt.feed(
        id,
        Input::Wheel {
            notches: 1.0,
            mods: M,
        },
        0.0,
    );
    assert_eq!(value(&rt, "sw"), 1.0, "clamped at the last position");
    rt.feed(
        id,
        Input::Key {
            key: Key::Down,
            mods: M,
        },
        0.0,
    );
    assert_eq!(value(&rt, "sw"), 0.5);
}

#[test]
fn a_stepped_range_snaps_and_a_continuous_one_does_not() {
    let (mut rt, id) = knob_on(ParamInfo::new(
        "g",
        "Gain",
        Range {
            steps: Some(5),
            ..Range::linear(-2.0, 2.0, Unit::Db)
        },
    ));
    rt.feed(
        id,
        Input::Down {
            x: 30.0,
            y: 100.0,
            t: 0.0,
            mods: M,
        },
        0.0,
    );
    rt.feed(
        id,
        Input::Move {
            x: 30.0,
            y: 90.0,
            t: 0.0,
            mods: M,
        },
        0.0,
    );
    assert!(on_detent(value(&rt, "g"), 5));
    let (mut rt, id) = knob_on(ParamInfo::new(
        "c",
        "Mix",
        Range::linear(0.0, 100.0, Unit::Percent),
    ));
    rt.feed(
        id,
        Input::Down {
            x: 30.0,
            y: 100.0,
            t: 0.0,
            mods: M,
        },
        0.0,
    );
    rt.feed(
        id,
        Input::Move {
            x: 30.0,
            y: 93.0,
            t: 0.0,
            mods: M,
        },
        0.0,
    );
    let v = value(&rt, "c");
    assert!(v > 0.0 && !on_detent(v, 5), "continuous params glide: {v}");
}
