//! Kit parts that move by themselves: a stream lamp warms and cools
//! through a spring; `flicker` makes a filament waver.

use std::collections::BTreeMap;

use document_proto::{Node, NodeId, NodeKind, Scene};
use surface_kit::{Place, place};
use surface_proto::{Binding, Catalog, EntryId, Range, SlotRole, StreamFrame, StreamInfo, StreamKind, Unit};
use surface_runtime::{Runtime, Surface};
use surface_widgets::{Builder, Theme};

fn lamp(p: &Place) -> (Runtime, NodeId) {
    let mut scene = Scene::new("Lamps");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
    let holder = b.add(page, Node::new(NodeKind::Frame, "holder"));
    let root = place(&mut b, holder, "meter.lamp", p).unwrap();
    for (id, c) in controls {
        scene.node_mut(id).unwrap().control = Some(c);
    }
    let catalog = Catalog {
        streams: vec![StreamInfo { id: EntryId::new("x/lfo"), name: "LFO".into(), kind: StreamKind::Scalar { range: Range::linear(0.0, 1.0, Unit::None) }, rate_hz: 60.0 }],
        ..Catalog::default()
    };
    let rt = Runtime::new(Surface::from_scene(scene), catalog);
    let glow = NodeId(rt.surface().controls[&root].slots_of(SlotRole::Glow).next().unwrap());
    (rt, glow)
}

fn lfo(rt: &mut Runtime, v: f64, t: f64) {
    rt.apply_frame(StreamFrame { id: EntryId::new("x/lfo"), values: vec![v] }, t);
}

#[test]
fn a_stream_lamp_warms_through_a_spring() {
    let (mut rt, glow) = lamp(&Place::new("Rate").bind(Binding::stream("x/lfo")));
    lfo(&mut rt, 0.0, 0.0);
    assert_eq!(rt.frame(0.0).node(glow).unwrap().opacity, 0.0);
    lfo(&mut rt, 1.0, 0.0);
    let _ = rt.frame(0.0); // at rest: the jump starts moving from here
    let warm = rt.frame(33.0).node(glow).unwrap().opacity;
    assert!(warm > 0.02 && warm < 0.9, "on its way up: {warm}");
    assert!(rt.is_animating());
    for k in 2..60 {
        let _ = rt.frame(33.0 * f64::from(k));
    }
    assert!((rt.frame(2000.0).node(glow).unwrap().opacity - 1.0).abs() < 0.01, "lit");
}

#[test]
fn flicker_wavers_the_light_not_the_lamp() {
    let (mut rt, glow) = lamp(&Place::new("Pilot").bind(Binding::stream("x/lfo")).options(&["flicker"]));
    lfo(&mut rt, 1.0, 0.0);
    let base = rt.surface().scene.node(glow).unwrap().emission.clone().unwrap().intensity;
    let lights: Vec<f64> = (0..20).map(|k| rt.frame(50.0 * f64::from(k)).node(glow).unwrap().emission.clone().unwrap().intensity).collect();
    assert!(lights.iter().all(|i| (i - base).abs() <= 0.12 * base + 1e-9), "within ±12 %: {lights:?}");
    assert!(lights.iter().any(|i| (i - base).abs() > 0.01), "and it moves: {lights:?}");
    assert!(rt.is_animating(), "a flickering lamp keeps the host drawing");
}
