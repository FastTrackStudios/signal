//! Meters and readouts through the runtime: gain reduction rests at 0 and
//! attacks downward, bars honour their binding's curve, a meter switch
//! picks its source from a choice param, lamps light at their thresholds,
//! and readouts print what their format says.

use std::collections::BTreeMap;

use document_proto::{Node, NodeId, NodeKind, Op, Scene, Size, Transform};
use surface_proto::{
    Binding, Catalog, Control, ControlKind, Curve, EntryId, MeterTap, Orientation, ParamAddr, ParamInfo, ParamValue, Range,
    SlotRole, SourceSelect, StreamFrame, StreamInfo, StreamKind, Unit, ValueFormat,
};
use surface_runtime::{Runtime, Surface};

fn insert(scene: &mut Scene, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    let mut n = Node::new(NodeKind::Frame, name);
    n.size = Size { width: w, height: h };
    n.transform = Transform::translate(x, y);
    let id = n.id;
    scene.apply(Op::Insert { node: n, parent, index: u32::MAX }).unwrap();
    id
}

fn catalog() -> Catalog {
    Catalog {
        params: vec![
            ParamInfo::new("comp/meter", "Meter", Range::default()).choice(&["GR", "+4", "+8", "Off"]),
            ParamInfo::new("comp/ratio", "Ratio", Range { steps: Some(21), ..Range::linear(1.0, 3.0, Unit::Ratio) }),
            ParamInfo::new("comp/gain", "Gain", Range { steps: Some(49), ..Range::linear(-12.0, 12.0, Unit::Db) }),
        ],
        streams: vec![
            StreamInfo { id: EntryId::new("comp/gr"), name: "GR".into(), kind: StreamKind::GainReduction, rate_hz: 30.0 },
            StreamInfo { id: EntryId::new("comp/out"), name: "Out".into(), kind: StreamKind::Level { channels: 2 }, rate_hz: 30.0 },
            StreamInfo {
                id: EntryId::new("lfo"),
                name: "LFO".into(),
                kind: StreamKind::Scalar { range: Range::linear(0.0, 1.0, Unit::None) },
                rate_hz: 60.0,
            },
        ],
        ..Catalog::default()
    }
}

/// One control on a 100×100 frame with a 10×100 child `part`.
struct One {
    rt: Runtime,
    root: NodeId,
    part: NodeId,
}

fn one(control: impl FnOnce(NodeId) -> Control, part: (f64, f64, f64, f64)) -> One {
    let mut scene = Scene::new("s");
    let page = scene.pages()[0];
    let root = insert(&mut scene, page, "meter", (0.0, 0.0, 100.0, 100.0));
    let part = insert(&mut scene, root, "part", part);
    let controls: BTreeMap<NodeId, Control> = [(root, control(part))].into_iter().collect();
    One { rt: Runtime::new(Surface { scene, controls }, catalog()), root, part }
}

fn position(o: &One, t: f64) -> f64 {
    let c = o.rt.surface().controls[&o.root].clone();
    o.rt.state(o.root, &c, t).position
}

fn gauge(stream: &str, floor: f64, ceiling: f64) -> impl FnOnce(NodeId) -> Control {
    let stream = stream.to_string();
    move |part| {
        Control::new(ControlKind::Gauge { sweep_degrees: 68.0, floor_db: floor, ceiling_db: ceiling })
            .slot(SlotRole::Needle, part.0)
            .bound(Binding::stream(stream))
    }
}

#[test]
fn a_gain_reduction_needle_rests_at_zero_and_swings_down_with_reduction() {
    let mut o = one(gauge("comp/gr", -20.0, 0.0), (45.0, 0.0, 10.0, 100.0));
    assert_eq!(position(&o, 0.0), 1.0, "no data: 0 dB of reduction, not the floor");
    o.rt.apply_frame(StreamFrame { id: EntryId::new("comp/gr"), values: vec![-10.0] }, 0.0);
    assert!((position(&o, 0.0) - 0.5).abs() < 1e-9, "attack to -10 at once");
    // It recovers toward 0 at the release rate, not instantly.
    o.rt.apply_frame(StreamFrame { id: EntryId::new("comp/gr"), values: vec![0.0] }, 100.0);
    let p = position(&o, 100.0);
    assert!(p > 0.5 && p < 1.0, "recovering: {p}");
    assert_eq!(position(&o, 10_000.0), 1.0);
    // A level stream still rests at the floor.
    let o = one(gauge("comp/out", -20.0, 3.0), (45.0, 0.0, 10.0, 100.0));
    assert_eq!(position(&o, 0.0), 0.0);
}

#[test]
fn a_gr_needle_on_a_vu_card_rests_on_zero_vu() {
    // The card's law over −20…+3, as the kit binds it.
    let o = one(
        |part| {
            let mut c = gauge("comp/gr", -20.0, 3.0)(part);
            c.binding.as_mut().unwrap().curve = Curve::Power { exponent: 1.9 };
            c
        },
        (45.0, 0.0, 10.0, 100.0),
    );
    let want = (20.0_f64 / 23.0).powf(1.9);
    assert!((position(&o, 0.0) - want).abs() < 1e-9);
}

#[test]
fn a_bar_meter_honours_its_curve_and_reads_nothing_without_data() {
    let bar = |curve: Curve| {
        move |part: NodeId| {
            Control::new(ControlKind::Meter { orientation: Orientation::Vertical })
                .slot(SlotRole::Fill, part.0)
                .bound(Binding { curve, ..Binding::stream("comp/out") })
        }
    };
    let mut lin = one(bar(Curve::Linear), (0.0, 0.0, 10.0, 100.0));
    let mut pow = one(bar(Curve::Power { exponent: 2.0 }), (0.0, 0.0, 10.0, 100.0));
    for o in [&lin, &pow] {
        assert_eq!(position(o, 0.0), 0.0, "silence reads empty, not full scale");
        let frame = o.rt.frame(0.0);
        assert!(!frame.node(o.part).unwrap().visible, "an empty bar draws nothing");
    }
    for o in [&mut lin, &mut pow] {
        o.rt.apply_frame(StreamFrame { id: EntryId::new("comp/out"), values: vec![-27.0, -27.0] }, 0.0);
    }
    assert!((position(&lin, 0.0) - 0.5).abs() < 1e-9, "-27 is halfway up −60…+6");
    assert!((position(&pow, 0.0) - 0.25).abs() < 1e-9, "…and a quarter through a square law");
}

#[test]
fn a_switched_meter_reads_the_tap_its_choice_selects() {
    let taps = vec![
        Some(MeterTap::new("comp/gr")),
        Some(MeterTap::new("comp/out").offset(-4.0)),
        Some(MeterTap::new("comp/out").offset(-8.0)),
        None,
    ];
    let mut o = one(
        |part| {
            let mut c = gauge("comp/gr", -20.0, 3.0)(part);
            c.select = Some(SourceSelect::new("comp/meter", taps));
            c
        },
        (45.0, 0.0, 10.0, 100.0),
    );
    let at_db = |db: f64| (db + 20.0) / 23.0;
    o.rt.apply_frame(StreamFrame { id: EntryId::new("comp/gr"), values: vec![-5.0] }, 0.0);
    o.rt.apply_frame(StreamFrame { id: EntryId::new("comp/out"), values: vec![2.0, 2.0] }, 0.0);
    let pick = |o: &mut One, i: f64| o.rt.apply_values([ParamValue { addr: ParamAddr::new("comp/meter"), value: i / 3.0 }]);
    pick(&mut o, 0.0);
    assert!((position(&o, 0.0) - at_db(-5.0)).abs() < 1e-9, "GR");
    pick(&mut o, 1.0);
    assert!((position(&o, 0.0) - at_db(-2.0)).abs() < 1e-9, "+4: +2 dB out reads −2");
    pick(&mut o, 2.0);
    assert!((position(&o, 0.0) - at_db(-6.0)).abs() < 1e-9, "+8");
    pick(&mut o, 3.0);
    assert_eq!(position(&o, 0.0), 0.0, "off: the needle drops to its stop");
    let issues = o.rt.surface().controls[&o.root].validate(o.rt.catalog());
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn a_gauge_thumb_travels_along_a_wide_lane() {
    // A horizontal LED row's cover: a 200×10 lane, the cover its left half.
    let mut o = one(
        |part| {
            Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: -60.0, ceiling_db: 0.0 })
                .slot(SlotRole::Thumb, part.0)
                .bound(Binding::stream("comp/out"))
        },
        (0.0, 0.0, 100.0, 10.0),
    );
    // Re-parent the cover into a wide lane.
    let mut s = o.rt.surface().scene.clone();
    let lane = insert(&mut s, o.root, "lane", (0.0, 0.0, 200.0, 10.0));
    s.apply(Op::Move { id: o.part, parent: lane, index: 0 }).unwrap();
    let controls = o.rt.surface().controls.clone();
    o.rt = Runtime::new(Surface { scene: s, controls }, catalog());
    o.rt.apply_frame(StreamFrame { id: EntryId::new("comp/out"), values: vec![-15.0, -15.0] }, 0.0);
    let t = o.rt.frame(0.0).node(o.part).unwrap().transform;
    assert!((t.tx - 75.0).abs() < 1e-9 && t.ty.abs() < 1e-9, "slid right, not down: {t:?}");
}

#[test]
fn a_ladder_lights_its_lamps_at_their_thresholds() {
    let mut scene = Scene::new("s");
    let page = scene.pages()[0];
    let root = insert(&mut scene, page, "ladder", (0.0, 0.0, 100.0, 10.0));
    let lamps: Vec<NodeId> = (0..4).map(|i| insert(&mut scene, root, "lamp", (f64::from(i) * 25.0, 0.0, 10.0, 10.0))).collect();
    let mut c = Control::new(ControlKind::Gauge { sweep_degrees: 0.0, floor_db: -40.0, ceiling_db: 0.0 }).bound(Binding::stream("comp/out"));
    for l in &lamps {
        c = c.slot(SlotRole::Led, l.0);
    }
    let mut rt = Runtime::new(Surface { scene, controls: [(root, c)].into_iter().collect() }, catalog());
    let lit = |rt: &Runtime| -> Vec<bool> {
        let f = rt.frame(0.0);
        lamps.iter().map(|l| f.node(*l).unwrap().opacity > 0.5).collect()
    };
    assert_eq!(lit(&rt), [false; 4], "nothing lit without signal");
    rt.apply_frame(StreamFrame { id: EntryId::new("comp/out"), values: vec![-15.0, -15.0] }, 0.0);
    assert_eq!(lit(&rt), [true, true, false, false], "-15 over −40…0 lights two of four");
}

#[test]
fn a_glow_follows_a_scalar_stream() {
    let mut o = one(
        |part| Control::new(ControlKind::Readout).slot(SlotRole::Glow, part.0).bound(Binding::stream("lfo")),
        (0.0, 0.0, 10.0, 10.0),
    );
    assert_eq!(o.rt.frame(0.0).node(o.part).unwrap().opacity, 0.0);
    o.rt.apply_frame(StreamFrame { id: EntryId::new("lfo"), values: vec![0.25] }, 0.0);
    assert!((o.rt.frame(0.0).node(o.part).unwrap().opacity - 0.25).abs() < 1e-9);
}

#[test]
fn readouts_print_steps_decimals_and_formats() {
    let text = |addr: &str, value: f64, format: Option<ValueFormat>| {
        let mut o = one(
            |part| {
                let mut c = Control::new(ControlKind::Readout).slot(SlotRole::Value, part.0).bound(Binding::param(addr));
                c.format = format;
                c
            },
            (0.0, 0.0, 10.0, 10.0),
        );
        o.rt.apply_values([ParamValue { addr: ParamAddr::new(addr), value }]);
        let c = o.rt.surface().controls[&o.root].clone();
        o.rt.state(o.root, &c, 0.0).value_text.unwrap()
    };
    assert_eq!(text("comp/ratio", 0.25, None), "1.5 :1", "a 0.1 step prints tenths");
    assert_eq!(text("comp/gain", 0.5 + 1.0 / 48.0, None), "0.5 dB");
    assert_eq!(text("comp/gain", 0.75, Some(ValueFormat::decimals(0))), "6 dB");
    assert_eq!(text("comp/ratio", 0.5, Some(ValueFormat::labels(&["Soft", "Med", "Hard"]))), "Med");
    assert_eq!(text("comp/meter", 1.0 / 3.0, None), "+4");
    // A meter's readout takes the format too.
    let mut o = one(
        |part| {
            let mut c = gauge("comp/gr", -20.0, 0.0)(part).slot(SlotRole::Value, part.0);
            c.format = Some(ValueFormat::decimals(0).unit("GR"));
            c
        },
        (0.0, 0.0, 10.0, 10.0),
    );
    o.rt.apply_frame(StreamFrame { id: EntryId::new("comp/gr"), values: vec![-6.4] }, 0.0);
    let c = o.rt.surface().controls[&o.root].clone();
    assert_eq!(o.rt.state(o.root, &c, 0.0).value_text.as_deref(), Some("-6 GR"));
}
