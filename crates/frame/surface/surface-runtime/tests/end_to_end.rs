//! A surface driving the mock engine over vox: drag a knob, tap a
//! footswitch, watch a meter — the whole loop, no GUI.

use architect::{LocalServer, Scope, Services as _};
use document_proto::{Color, Node, NodeId, NodeKind, Op, Paint, Scene, Size, TextData, Transform};
use surface_memory::Memory;
use surface_proto::*;
use surface_runtime::{Bounds, Input, Modifiers, Runtime, Surface, sync};

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

fn surface() -> (Surface, NodeId, NodeId, NodeId, NodeId) {
    let mut scene = Scene::new("Stage");
    let page = scene.pages()[0];

    let mut knob = Node::new(NodeKind::Frame, "Gain");
    knob.size = Size {
        width: 64.0,
        height: 64.0,
    };
    knob.transform = Transform::translate(0.0, 0.0);
    let knob_id = insert(&mut scene, page, knob);
    let mut needle = Node::new(NodeKind::Rectangle, "needle");
    needle.size = Size {
        width: 4.0,
        height: 24.0,
    };
    needle.transform = Transform::translate(30.0, 8.0);
    let needle_id = insert(&mut scene, knob_id, needle);
    let mut value = Node::new(NodeKind::Text(TextData::default()), "value");
    value.fills = vec![Paint::solid(Color::rgba(1.0, 1.0, 1.0, 1.0))];
    let value_id = insert(&mut scene, knob_id, value);

    let mut sw = Node::new(NodeKind::Frame, "Drive");
    sw.size = Size {
        width: 80.0,
        height: 80.0,
    };
    sw.transform = Transform::translate(100.0, 0.0);
    let sw_id = insert(&mut scene, page, sw);

    let mut meter = Node::new(NodeKind::Frame, "Out L");
    meter.size = Size {
        width: 10.0,
        height: 100.0,
    };
    meter.transform = Transform::translate(200.0, 0.0);
    let meter_id = insert(&mut scene, page, meter);
    let mut bar = Node::new(NodeKind::Rectangle, "bar");
    bar.size = Size {
        width: 10.0,
        height: 100.0,
    };
    let bar_id = insert(&mut scene, meter_id, bar);

    let controls = [
        (
            knob_id,
            Control::new(ControlKind::knob())
                .bound(Binding::param("amp/gain"))
                .slot(SlotRole::Needle, needle_id.0)
                .slot(SlotRole::Value, value_id.0),
        ),
        (
            sw_id,
            Control::new(ControlKind::footswitch()).bound(Binding::param("drive/on")),
        ),
        (
            meter_id,
            Control::new(ControlKind::Meter {
                orientation: Orientation::Vertical,
            })
            .bound(Binding::stream("out_level"))
            .slot(SlotRole::Fill, bar_id.0),
        ),
    ]
    .into_iter()
    .collect();
    (
        Surface { scene, controls },
        knob_id,
        sw_id,
        meter_id,
        bar_id,
    )
}

fn boxes(rt: &mut Runtime, knob: NodeId, sw: NodeId, meter: NodeId) {
    rt.set_boxes([
        (
            knob,
            Bounds {
                x: 0.0,
                y: 0.0,
                width: 64.0,
                height: 64.0,
            },
        ),
        (
            sw,
            Bounds {
                x: 100.0,
                y: 0.0,
                width: 80.0,
                height: 80.0,
            },
        ),
        (
            meter,
            Bounds {
                x: 200.0,
                y: 0.0,
                width: 10.0,
                height: 100.0,
            },
        ),
    ]);
}

#[tokio::test]
async fn drag_a_knob_over_vox() {
    let engine = Memory::demo();
    let scope = Scope::new();
    let local = LocalServer::serve(engine.clone().into_router(), scope.clone());
    let client: SurfaceEngineClient = local.establish().await.unwrap();

    let (surface, knob, sw, meter, _) = surface();
    let mut rt = sync::connect(&client, surface).await.unwrap();
    boxes(&mut rt, knob, sw, meter);

    // Gain defaults to 5/10. Drag 40 px up (+0.2), far outside the knob.
    let m = Modifiers::default();
    let mut calls = rt.pointer(
        Input::Down {
            x: 32.0,
            y: 32.0,
            t: 0.0,
            mods: m,
        },
        0.0,
    );
    for i in 1..=40 {
        let t = f64::from(i) * 4.0;
        calls.extend(rt.pointer(
            Input::Move {
                x: 400.0,
                y: 32.0 - f64::from(i),
                t,
                mods: m,
            },
            t,
        ));
        calls.extend(rt.poll(t));
    }
    calls.extend(rt.pointer(
        Input::Up {
            x: 400.0,
            y: -8.0,
            t: 170.0,
        },
        170.0,
    ));
    assert!(calls.len() < 10, "coalesced: {} calls", calls.len());
    sync::send(&client, calls).await.unwrap();

    let gain = engine.value(&ParamAddr::new("amp/gain")).unwrap();
    assert!(
        (gain - 0.7).abs() < 1e-9,
        "engine holds the final value, got {gain}"
    );

    // The drawn frame shows the value and a rotated needle.
    let frame = rt.frame(200.0);
    let texts: Vec<_> = frame
        .nodes
        .values()
        .filter_map(|n| n.text().map(|t| t.characters.clone()))
        .collect();
    assert!(
        texts.contains(&"7.00".to_string()),
        "readout, got {texts:?}"
    );
    let needle = frame.nodes.values().find(|n| n.name == "needle").unwrap();
    assert!((needle.transform.rotation() - (0.2 * 270f64).to_radians()).abs() < 1e-9);
    scope.close().await;
}

#[tokio::test]
async fn footswitch_tap_toggles_a_param() {
    let engine = Memory::demo();
    let (surface, knob, sw, meter, _) = surface();
    let mut rt = sync::connect(&sync::Local(&engine), surface).await.unwrap();
    boxes(&mut rt, knob, sw, meter);
    let m = Modifiers::default();
    let mut calls = rt.pointer(
        Input::Down {
            x: 140.0,
            y: 40.0,
            t: 0.0,
            mods: m,
        },
        0.0,
    );
    calls.extend(rt.pointer(
        Input::Up {
            x: 140.0,
            y: 40.0,
            t: 120.0,
        },
        120.0,
    ));
    sync::send(&sync::Local(&engine), calls).await.unwrap();
    assert_eq!(engine.value(&ParamAddr::new("drive/on")), Some(1.0));
}

#[tokio::test]
async fn meter_follows_telemetry_with_ballistics() {
    let engine = Memory::demo();
    let (surface, _, _, meter, bar) = surface();
    let mut rt = sync::connect(&sync::Local(&engine), surface).await.unwrap();
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("out_level"),
            values: vec![-27.0, -40.0],
        },
        0.0,
    );
    let bar_h = |rt: &Runtime, t| rt.frame(t).node(bar).unwrap().size.height;
    assert!(
        (bar_h(&rt, 0.0) - 50.0).abs() < 1e-9,
        "-27 dB is mid-scale on -60..+6"
    );
    assert!(bar_h(&rt, 500.0) < 50.0, "falls after the level stops");
    let state = rt.state(meter, &rt.surface().controls[&meter], 0.0);
    assert!((state.peak.unwrap() - 0.5).abs() < 1e-9);
}

#[test]
fn hit_testing_prefers_topmost_input_controls() {
    let (surface, knob, sw, meter, _) = surface();
    let mut rt = Runtime::new(surface, surface_memory::demo_catalog());
    boxes(&mut rt, knob, sw, meter);
    assert_eq!(rt.hit(10.0, 10.0), Some(knob));
    assert_eq!(rt.hit(150.0, 10.0), Some(sw));
    assert_eq!(rt.hit(205.0, 50.0), None, "meters aren't input controls");
    assert_eq!(rt.hit(90.0, 10.0), None);
}
