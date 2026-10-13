use frame::design::lint::{Config, Severity, contrast_ratio, counts, lint};
use frame::document::proto::{Color, NodeId, Op, Prop, Size};
use frame::surface::memory::demo_catalog;
use frame::surface::proto::{Binding, Curve, MidiMapping, MidiMessage, ParamAddr, SlotRole};
use frame::surface::widgets::{Options, surface_for_catalog};

fn demo() -> frame::document::proto::Scene {
    surface_for_catalog(&demo_catalog(), &Options::default()).to_scene()
}

fn control_named<'a>(
    scene: &'a frame::document::proto::Scene,
    name: &str,
) -> &'a frame::document::proto::Node {
    scene
        .nodes
        .values()
        .find(|n| n.name == name && n.control.is_some())
        .unwrap()
}

#[test]
fn generated_surface_has_no_errors() {
    let issues = lint(&demo(), &demo_catalog(), &Config::default());
    let (errors, warnings, _) = counts(&issues);
    assert_eq!(
        errors,
        0,
        "{}",
        issues
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Small pan knob is below touch size — the only expected warning class.
    assert!(
        issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .all(|i| i.rule == "touch-target" || i.rule == "size-inconsistent"),
        "{warnings}"
    );
}

#[test]
fn unknown_param_gets_a_suggestion() {
    let mut s = demo();
    let gain = control_named(&s, "Gain").clone();
    let mut c = gain.control.clone().unwrap();
    c.binding = Some(Binding::param("amp/gian"));
    s.apply(Op::Set {
        id: gain.id,
        prop: Prop::Control(Some(c)),
    })
    .unwrap();
    let issues = lint(&s, &demo_catalog(), &Config::default());
    let i = issues.iter().find(|i| i.rule == "binding").unwrap();
    assert!(i.fix.contains("did you mean `amp/gain`"), "{i}");
}

#[test]
fn missing_slots_and_dangling_slots() {
    let mut s = demo();
    let gain = control_named(&s, "Gain").clone();
    let mut c = gain.control.clone().unwrap();
    c.slots
        .retain(|sl| !matches!(sl.role, SlotRole::Needle | SlotRole::ValueArc));
    c.slots.push(frame::surface::proto::Slot {
        role: SlotRole::Value,
        node: NodeId::new().0,
    });
    s.apply(Op::Set {
        id: gain.id,
        prop: Prop::Control(Some(c)),
    })
    .unwrap();
    let rules: Vec<_> = lint(&s, &demo_catalog(), &Config::default())
        .into_iter()
        .map(|i| i.rule)
        .collect();
    assert!(rules.contains(&"slot-missing".to_string()));
    assert!(rules.contains(&"slot-dangling".to_string()));
}

#[test]
fn duplicate_midi_is_an_error() {
    let mut s = demo();
    for name in ["Gain", "Bass"] {
        let n = control_named(&s, name).clone();
        let mut c = n.control.clone().unwrap();
        c.midi = Some(MidiMapping {
            channel: Some(1),
            message: MidiMessage::Cc { number: 21 },
            curve: Curve::Linear,
        });
        s.apply(Op::Set {
            id: n.id,
            prop: Prop::Control(Some(c)),
        })
        .unwrap();
    }
    assert!(
        lint(&s, &demo_catalog(), &Config::default())
            .iter()
            .any(|i| i.rule == "midi-duplicate" && i.severity == Severity::Error)
    );
}

#[test]
fn overlap_and_inconsistent_size() {
    let mut s = demo();
    let bass = control_named(&s, "Bass").clone();
    // Pull Bass out of flow onto Gain, and make it bigger.
    let mut lc = bass.layout_child.clone();
    lc.absolute = true;
    s.transact([
        Op::Set {
            id: bass.id,
            prop: Prop::LayoutChild(lc),
        },
        Op::Set {
            id: bass.id,
            prop: Prop::Transform(frame::document::proto::Transform::translate(0.0, 0.0)),
        },
        Op::Set {
            id: bass.id,
            prop: Prop::Size(Size {
                width: 80.0,
                height: 120.0,
            }),
        },
    ])
    .unwrap();
    let rules: Vec<_> = lint(&s, &demo_catalog(), &Config::default())
        .into_iter()
        .map(|i| i.rule)
        .collect();
    assert!(rules.contains(&"overlap".to_string()), "{rules:?}");
    assert!(rules.contains(&"size-inconsistent".to_string()));
}

#[test]
fn low_contrast_text() {
    let mut s = demo();
    let label = s.nodes.values().find(|n| n.name == "caption").unwrap().id;
    s.apply(Op::Set {
        id: label,
        prop: Prop::Fills(vec![frame::document::proto::Paint::solid(Color::rgba(
            0.1, 0.1, 0.12, 1.0,
        ))]),
    })
    .unwrap();
    assert!(
        lint(&s, &demo_catalog(), &Config::default())
            .iter()
            .any(|i| i.rule == "contrast" && i.node == label)
    );
    assert!(
        (contrast_ratio(
            &Color::rgba(0.0, 0.0, 0.0, 1.0),
            &Color::rgba(1.0, 1.0, 1.0, 1.0)
        ) - 21.0)
            .abs()
            < 1e-9
    );
}

#[test]
fn unbound_controls_are_flagged() {
    let mut s = demo();
    let gain = control_named(&s, "Gain").clone();
    let mut c = gain.control.clone().unwrap();
    c.binding = None;
    s.apply(Op::Set {
        id: gain.id,
        prop: Prop::Control(Some(c)),
    })
    .unwrap();
    let issues = lint(&s, &demo_catalog(), &Config::default());
    assert!(
        issues
            .iter()
            .any(|i| i.rule == "binding" && i.message.contains("not bound"))
    );
    let _ = ParamAddr::new("x");
}

/// A 100×100 knob with a 50×50 knob inside it: nested in its node (a
/// concentric pair) or a sibling laid over it.
fn pair(nested: bool, inner_at: f64) -> frame::document::proto::Scene {
    use frame::document::proto::{Node, NodeKind, Transform};
    use frame::surface::proto::{Control, ControlKind};
    let add =
        |s: &mut frame::document::proto::Scene, parent, name: &str, at: f64, w: f64, bind: &str| {
            let mut n = Node::new(NodeKind::Frame, name);
            n.size = Size {
                width: w,
                height: w,
            };
            n.transform = Transform::translate(at, at);
            let pointer = Node::new(NodeKind::Rectangle, "pointer");
            n.control = Some(
                Control::new(ControlKind::knob())
                    .bound(Binding::param(bind))
                    .slot(SlotRole::Needle, pointer.id.0),
            );
            let id = n.id;
            s.apply(Op::Insert {
                node: n,
                parent,
                index: u32::MAX,
            })
            .unwrap();
            s.apply(Op::Insert {
                node: pointer,
                parent: id,
                index: 0,
            })
            .unwrap();
            id
        };
    let mut s = frame::document::proto::Scene::new("s");
    let page = s.pages()[0];
    let outer = add(&mut s, page, "Collar", 0.0, 100.0, "amp/gain");
    add(
        &mut s,
        if nested { outer } else { page },
        "Cap",
        inner_at,
        50.0,
        "amp/bass",
    );
    s
}

#[test]
fn a_concentric_pair_is_not_an_overlap() {
    let overlaps = |s: &frame::document::proto::Scene| {
        lint(s, &demo_catalog(), &Config::default())
            .iter()
            .filter(|i| i.rule == "overlap")
            .count()
    };
    assert_eq!(overlaps(&pair(true, 25.0)), 0, "a cap nested in its collar");
    assert_eq!(
        overlaps(&pair(false, 25.0)),
        1,
        "a sibling laid over another is still ambiguous"
    );
    assert_eq!(
        overlaps(&pair(true, 70.0)),
        1,
        "nested but hanging out of the collar's box"
    );
}

/// A 200×100 plot area holding one control of `kind` (a trace vector in
/// its `Trace` slot), the control node the area's size.
fn in_area(
    kind: frame::surface::proto::ControlKind,
    binding: Option<Binding>,
    area: Size,
) -> frame::document::proto::Scene {
    use frame::document::proto::{Node, NodeKind};
    use frame::surface::proto::Control;
    let mut s = frame::document::proto::Scene::new("s");
    let page = s.pages()[0];
    let mut a = Node::new(NodeKind::Frame, "area");
    a.size = area;
    let a_id = a.id;
    s.apply(Op::Insert {
        node: a,
        parent: page,
        index: 0,
    })
    .unwrap();
    let mut n = Node::new(NodeKind::Frame, "viz");
    n.size = Size {
        width: 44.0,
        height: 44.0,
    };
    let line = Node::new(NodeKind::Vector, "line");
    let mut c = Control::new(kind).slot(SlotRole::Trace, line.id.0);
    c.binding = binding;
    n.control = Some(c);
    let id = n.id;
    s.apply(Op::Insert {
        node: n,
        parent: a_id,
        index: 0,
    })
    .unwrap();
    s.apply(Op::Insert {
        node: line,
        parent: id,
        index: 0,
    })
    .unwrap();
    s
}

fn rules(s: &frame::document::proto::Scene) -> Vec<(String, Severity)> {
    lint(s, &demo_catalog(), &Config::default())
        .into_iter()
        .map(|i| (i.rule, i.severity))
        .collect()
}

#[test]
fn a_plot_bound_to_a_missing_feed_or_a_param_is_a_binding_error() {
    use frame::surface::proto::{Axis, ControlKind, PlotRange, PlotStyle};
    let plot = || ControlKind::Plot {
        style: PlotStyle::Line,
        x: Axis::Log {
            f_min: 20.0,
            f_max: 20_000.0,
        },
        y: PlotRange::linear(-90.0, 0.0),
    };
    let area = Size {
        width: 200.0,
        height: 100.0,
    };
    let missing = rules(&in_area(plot(), Some(Binding::stream("eq/spectrum")), area));
    assert!(
        missing.contains(&("binding".into(), Severity::Error)),
        "{missing:?}"
    );
    let param = rules(&in_area(plot(), Some(Binding::param("amp/gain")), area));
    assert!(
        param.contains(&("binding".into(), Severity::Error)),
        "{param:?}"
    );
    let ok = rules(&in_area(plot(), Some(Binding::stream("out_level")), area));
    assert!(!ok.iter().any(|(_, s)| *s == Severity::Error), "{ok:?}");
}

#[test]
fn a_curve_missing_an_input_or_its_provider_is_flagged() {
    use frame::surface::proto::{Axis, ControlKind, CurveInput, PlotRange, PlotStyle};
    let curve = |provider: &str, inputs: Vec<CurveInput>| ControlKind::Curve {
        provider: provider.into(),
        inputs,
        series: None,
        style: PlotStyle::Line,
        x: Axis::Linear {
            min: -60.0,
            max: 0.0,
        },
        y: PlotRange::linear(-60.0, 0.0),
    };
    let area = Size {
        width: 200.0,
        height: 100.0,
    };
    let no_ratio = rules(&in_area(
        curve(
            "comp.transfer",
            vec![CurveInput::param("threshold", "amp/master")],
        ),
        None,
        area,
    ));
    assert!(
        no_ratio.contains(&("curve-input".into(), Severity::Error)),
        "{no_ratio:?}"
    );
    let full = vec![
        CurveInput::param("threshold", "amp/master"),
        CurveInput::constant("ratio", 4.0),
    ];
    assert!(
        rules(&in_area(curve("comp.transfer", full.clone()), None, area)).is_empty(),
        "a complete curve is clean"
    );
    let unknown_param = vec![
        CurveInput::param("threshold", "comp/threshold"),
        CurveInput::constant("ratio", 4.0),
    ];
    assert!(
        rules(&in_area(curve("comp.transfer", unknown_param), None, area))
            .contains(&("binding".into(), Severity::Error))
    );
    let typo = rules(&in_area(curve("comp.transfr", full), None, area));
    assert!(
        typo.contains(&("curve-provider".into(), Severity::Warning)),
        "{typo:?}"
    );
    let no_bands = rules(&in_area(curve("eq.response", Vec::new()), None, area));
    assert!(
        no_bands.contains(&("curve-input".into(), Severity::Error)),
        "{no_bands:?}"
    );
}

#[test]
fn a_handle_needs_params_and_an_area() {
    use frame::surface::proto::{Axis, ControlKind, HandleAxis};
    let handle = |x: &str| ControlKind::Handle {
        x: Some(Binding::param(x)),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear {
            min: 0.0,
            max: 10.0,
        }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    let ok = rules(&in_area(
        handle("amp/gain"),
        None,
        Size {
            width: 200.0,
            height: 100.0,
        },
    ));
    assert!(!ok.iter().any(|(_, s)| *s == Severity::Error), "{ok:?}");
    let flat = rules(&in_area(
        handle("amp/gain"),
        None,
        Size {
            width: 0.0,
            height: 0.0,
        },
    ));
    assert!(
        flat.contains(&("handle-area".into(), Severity::Error)),
        "{flat:?}"
    );
    let unknown = rules(&in_area(
        handle("amp/gian"),
        None,
        Size {
            width: 200.0,
            height: 100.0,
        },
    ));
    assert!(
        unknown.contains(&("binding".into(), Severity::Error)),
        "{unknown:?}"
    );
}

#[test]
fn controls_on_exclusive_variant_layers_do_not_overlap() {
    // Bass laid over Gain, as in `overlap_and_inconsistent_size`: then shown
    // only for different values of one choice (colourway layers), which are
    // never on screen together — not an overlap. The same value is.
    use frame::surface::proto::{Condition, ParamAddr as Addr};
    let mut s = demo();
    let (bass, gain) = (
        control_named(&s, "Bass").clone(),
        control_named(&s, "Gain").clone(),
    );
    let mut lc = bass.layout_child.clone();
    lc.absolute = true;
    let at = gain.transform;
    let shown = |node: &frame::document::proto::Node, v: f64| {
        let mut c = node.control.clone().unwrap();
        c.show_if = Some(Condition::any_of(Addr::new("amp/channel").0, &[v]));
        Prop::Control(Some(c))
    };
    s.transact([
        Op::Set {
            id: bass.id,
            prop: Prop::LayoutChild(lc),
        },
        Op::Set {
            id: bass.id,
            prop: Prop::Transform(at),
        },
        Op::Set {
            id: bass.id,
            prop: Prop::Size(gain.size),
        },
        Op::Set {
            id: bass.id,
            prop: shown(&bass, 0.0),
        },
        Op::Set {
            id: gain.id,
            prop: shown(&gain, 1.0),
        },
    ])
    .unwrap();
    let overlaps = |s: &frame::document::proto::Scene| {
        lint(s, &demo_catalog(), &Config::default())
            .iter()
            .filter(|i| i.rule == "overlap")
            .count()
    };
    assert_eq!(overlaps(&s), 0, "different colourways never show together");
    let gain2 = control_named(&s, "Gain").clone();
    s.apply(Op::Set {
        id: gain2.id,
        prop: shown(&gain2, 0.0),
    })
    .unwrap();
    assert!(overlaps(&s) > 0, "the same colourway still overlaps");
}
