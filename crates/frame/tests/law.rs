//! THE LAW: `parse(print(s), s.assets.clone()) == s`, for every scene.

use std::collections::BTreeMap;
use std::path::PathBuf;

use frame::document::proto::*;
use frame::document::text::{parse, parse_fragment, print, print_subtree};

#[track_caller]
fn assert_law(scene: &Scene) -> String {
    let text = print(scene);
    let back = match parse(&text, scene.assets.clone()) {
        Ok(s) => s,
        Err(e) => panic!("printed markup doesn't parse: {e}\n---\n{text}"),
    };
    if &back != scene {
        for (id, n) in &scene.nodes {
            let b = back.nodes.get(id);
            if b != Some(n) {
                panic!("node {id} differs\nexpected: {n:#?}\n     got: {b:#?}\n---\n{text}");
            }
        }
        assert_eq!(back.root, scene.root);
        panic!("scenes differ (node set)");
    }
    // Printing is deterministic and a fixed point.
    assert_eq!(print(&back), text);
    text
}

fn add(scene: &mut Scene, parent: NodeId, mut node: Node) -> NodeId {
    let id = node.id;
    node.parent = Some(parent);
    scene.node_mut(parent).unwrap().children.push(id);
    scene.nodes.insert(id, node);
    id
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgba(
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        1.0,
    )
}

/// A scene that exercises every NodeKind and every `Prop`, in both compact
/// and fallback forms.
fn kitchen_sink() -> Scene {
    let mut s = Scene::new("Surface");
    let page = s.pages()[0];
    let asset = s.add_asset("image/png", vec![1, 2, 3, 4]);
    let svg = s.add_asset("image/svg+xml", b"<svg/>".to_vec());

    // Frame with auto layout + every compact prop.
    let mut panel = Node::new(NodeKind::Frame, "Panel");
    panel.transform = Transform::translate(10.0, -20.5);
    panel.size = Size {
        width: 320.0,
        height: 200.0,
    };
    panel.opacity = 0.75;
    panel.blend = BlendMode::PassThrough;
    panel.corner_radii = CornerRadii::uniform(8.0);
    panel.clips_content = true;
    panel.fills = vec![Paint::solid(rgb(0x1e, 0x1e, 0x1e))];
    panel.stroke = Stroke {
        paints: vec![Paint::solid(Color::rgba(0.0, 0.0, 0.0, 128.0 / 255.0))],
        weight: 2.0,
        cap: StrokeCap::Round,
        join: StrokeJoin::Bevel,
        align: StrokeAlign::Inside,
        miter_limit: 10.0,
        dash_pattern: vec![4.0, 2.5],
        dash_offset: 1.0,
    };
    panel.effects = vec![
        Effect {
            kind: EffectKind::DropShadow,
            visible: true,
            color: Color::rgba(0.0, 0.0, 0.0, 0.25),
            offset_x: 0.0,
            offset_y: 2.0,
            radius: 4.0,
            spread: 0.0,
        },
        Effect {
            kind: EffectKind::LayerBlur,
            visible: false,
            color: rgb(1, 2, 3),
            offset_x: 1.0,
            offset_y: 0.0,
            radius: 3.0,
            spread: -1.0,
        },
    ];
    panel.layout = Some(AutoLayout {
        direction: LayoutDirection::Horizontal,
        gap: 8.0,
        padding_left: 12.0,
        padding_right: 12.0,
        padding_top: 12.0,
        padding_bottom: 12.0,
        wrap: true,
        main_align: Some(MainAxisAlign::SpaceBetween),
        cross_align: Some(CrossAxisAlign::Center),
        hug_main: false,
        hug_cross: false,
    });
    panel.source = Some(SourceRef {
        tool: "figma".into(),
        id: "1:2".into(),
        node_type: "FRAME".into(),
        bounds: None,
    });
    let panel = add(&mut s, page, panel);

    // Layout child props, odd paddings, rotation, masks.
    let mut col = Node::new(NodeKind::Frame, "");
    col.layout = Some(AutoLayout {
        direction: LayoutDirection::Vertical,
        gap: 0.0,
        padding_left: 1.0,
        padding_right: 2.0,
        padding_top: 3.0,
        padding_bottom: 4.0,
        wrap: false,
        main_align: None,
        cross_align: Some(CrossAxisAlign::Baseline),
        hug_main: false,
        hug_cross: false,
    });
    col.layout_child = LayoutChild {
        grow: Some(1.0),
        align_self: Some(AlignSelf::Stretch),
        absolute: true,
        min_width: Some(10.0),
        min_height: Some(0.0),
        max_width: Some(100.5),
        max_height: None,
        resize_x: Some(ResizeX::Stretch),
        resize_y: Some(ResizeY::Bottom),
    };
    let (a, b, c, d) = (
        30f64.to_radians().cos(),
        -(30f64.to_radians().sin()),
        30f64.to_radians().sin(),
        30f64.to_radians().cos(),
    );
    col.transform = Transform {
        a,
        b,
        tx: 5.0,
        c,
        d,
        ty: 6.0,
    };
    col.corner_radii = CornerRadii {
        top_left: 1.0,
        top_right: 2.0,
        bottom_right: 3.0,
        bottom_left: 4.0,
    };
    col.mask = Some(MaskKind::Luminance);
    col.visible = false;
    col.locked = true;
    let col = add(&mut s, panel, col);

    // Every leaf kind.
    let mut knob = Node::new(NodeKind::Ellipse, "Knob");
    knob.size = Size {
        width: 64.0,
        height: 64.0,
    };
    knob.fills = vec![Paint::solid(rgb(0xff, 0x5a, 0x36))];
    knob.transform = Transform {
        a: 0.0,
        b: -1.0,
        tx: 0.0,
        c: 1.0,
        d: 0.0,
        ty: 0.0,
    }; // 90°
    add(&mut s, col, knob);

    let mut rect = Node::new(NodeKind::Rectangle, "Image");
    rect.fills = vec![
        Paint {
            kind: PaintKind::Image {
                asset: asset.clone(),
                source_ref: Some("abc123".into()),
                scale_mode: ImageScaleMode::Crop,
                transform: Some(Transform::translate(0.1, 0.2)),
            },
            opacity: 0.5,
            blend: BlendMode::Multiply,
            visible: false,
        },
        Paint {
            kind: PaintKind::Image {
                asset,
                source_ref: None,
                scale_mode: ImageScaleMode::Fill,
                transform: None,
            },
            opacity: 1.0,
            blend: BlendMode::Normal,
            visible: true,
        },
        Paint {
            kind: PaintKind::LinearGradient {
                stops: vec![
                    GradientStop {
                        position: 0.0,
                        color: rgb(255, 0, 0),
                    },
                    GradientStop {
                        position: 1.0,
                        color: Color::rgba(0.1, 0.2, 0.3, 0.4),
                    },
                ],
                transform: Some(Transform {
                    a: 0.5,
                    b: 0.1,
                    tx: 0.2,
                    c: -0.3,
                    d: 2.0,
                    ty: 1e-9,
                }),
            },
            opacity: 1.0,
            blend: BlendMode::Normal,
            visible: true,
        },
        Paint {
            kind: PaintKind::RadialGradient {
                stops: vec![GradientStop {
                    position: 0.5,
                    color: rgb(0, 0, 0),
                }],
                transform: None,
            },
            opacity: 0.3,
            blend: BlendMode::Normal,
            visible: true,
        },
        Paint {
            kind: PaintKind::AngularGradient {
                stops: vec![],
                transform: None,
            },
            opacity: 1.0,
            blend: BlendMode::Normal,
            visible: true,
        },
    ];
    rect.transform = Transform {
        a: 1.5,
        b: 0.25,
        tx: 1.0 / 3.0,
        c: 0.0,
        d: 0.5,
        ty: -0.0,
    };
    add(&mut s, col, rect);

    let mut line = Node::new(NodeKind::Line, "Line");
    line.stroke.paints = vec![Paint::solid(Color::rgba(0.3, 0.3, 0.3, 1.0))]; // not hex-exact
    line.stroke.weight = 0.5;
    line.stroke_geometry = vec!["M0 0 L10 0".into()];
    add(&mut s, col, line);

    let mut poly = Node::new(NodeKind::Polygon, "Polygon");
    poly.fill_geometry = vec!["M0 0 L10 0 L5 8 Z".into(), "M1 1 Z".into()];
    poly.svg_snapshot = Some(svg);
    add(&mut s, col, poly);

    let mut star = Node::new(NodeKind::Star, "Star \"quoted\" <tag> & {brace}\nnewline");
    star.source = Some(SourceRef {
        tool: "we/ird".into(),
        id: "I1:2;3:4".into(),
        node_type: "STAR".into(),
        bounds: None,
    });
    add(&mut s, col, star);

    let mut vector = Node::new(NodeKind::Vector, "Vector");
    vector.fill_geometry = vec![String::new()];
    add(&mut s, col, vector);

    let mut boolean = Node::new(
        NodeKind::BooleanOperation {
            op: Some(BooleanOp::Subtract),
        },
        "Bool",
    );
    let bid = boolean.id;
    boolean.children.clear();
    let bid = {
        add(&mut s, col, boolean);
        bid
    };
    add(&mut s, bid, Node::new(NodeKind::Rectangle, "A"));
    add(&mut s, bid, Node::new(NodeKind::Ellipse, "B"));
    add(
        &mut s,
        col,
        Node::new(NodeKind::BooleanOperation { op: None }, "BooleanOperation"),
    );

    // Components.
    let set = add(&mut s, page, Node::new(NodeKind::ComponentSet, "Buttons"));
    let comp = add(&mut s, set, Node::new(NodeKind::Component, "Button"));
    add(
        &mut s,
        page,
        Node::new(
            NodeKind::Instance {
                component: Some(comp),
            },
            "Button 1",
        ),
    );
    add(
        &mut s,
        page,
        Node::new(NodeKind::Instance { component: None }, "Detached"),
    );
    let group = add(&mut s, page, Node::new(NodeKind::Group, "Group"));
    add(&mut s, group, Node::new(NodeKind::Utility, "Slice"));
    add(
        &mut s,
        group,
        Node::new(
            NodeKind::Other {
                tag: "WIDGET".into(),
            },
            "WIDGET",
        ),
    );
    add(
        &mut s,
        group,
        Node::new(NodeKind::Other { tag: String::new() }, "odd"),
    );

    // Text: body, specials, newline, runs, every text prop.
    let text = |chars: &str| TextData {
        characters: chars.into(),
        ..TextData::default()
    };
    let mut label = Node::new(NodeKind::Text(text("Gain")), "Label");
    label.fills = vec![Paint::solid(rgb(255, 255, 255))];
    add(&mut s, panel, label);
    add(
        &mut s,
        panel,
        Node::new(NodeKind::Text(text("a < b && {c} > d &amp;")), "Specials"),
    );
    add(
        &mut s,
        panel,
        Node::new(NodeKind::Text(text("  padded  ")), "Padded"),
    );
    add(
        &mut s,
        panel,
        Node::new(NodeKind::Text(text("line one\nline two\n")), "Multi"),
    );
    add(&mut s, panel, Node::new(NodeKind::Text(text("")), "Empty"));
    add(
        &mut s,
        panel,
        Node::new(NodeKind::Text(text("\t\r")), "Tabby"),
    );
    let full = TextData {
        characters: "Mixed".into(),
        font_family: Some("Inter".into()),
        font_size: 12.5,
        font_weight: Some(600.0),
        font_style: Some("italic".into()),
        font_width: Some(75.0),
        line_height: Some(16.0),
        letter_spacing: -0.2,
        text_case: TextCase::SmallCapsForced,
        align: TextAlign::Justified,
        runs: vec![
            TextRun {
                text: "Mi".into(),
                ..TextRun::default()
            },
            TextRun {
                text: "xed".into(),
                fills: vec![
                    Paint::solid(rgb(1, 2, 3)),
                    Paint::solid(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                ],
                font_family: Some("Roboto".into()),
                font_size: Some(20.0),
                font_weight: Some(300.0),
                font_style: Some("Italic".into()),
                font_width: Some(112.5),
                letter_spacing: Some(1.0),
            },
        ],
    };
    add(&mut s, panel, Node::new(NodeKind::Text(full), "Full"));
    let parent_text = add(
        &mut s,
        panel,
        Node::new(NodeKind::Text(text("has kids")), "Parent text"),
    );
    add(&mut s, parent_text, Node::new(NodeKind::Rectangle, "Kid"));
    s
}

#[test]
fn kitchen_sink_round_trips() {
    let text = assert_law(&kitchen_sink());
    // Spot-check compact forms.
    assert!(text.contains("fill=\"#1e1e1e\""), "{text}");
    assert!(
        text.contains(
            "layout=\"row\" gap=8 padding=12 wrap=true justify=\"space-between\" align=\"center\""
        ),
        "{text}"
    );
    assert!(text.contains("padding=\"3 2 4 1\""), "{text}");
    assert!(text.contains("rotation=30"), "{text}");
    assert!(text.contains("rotation=90"), "{text}");
    assert!(text.contains(">Gain</Text>"), "{text}");
    assert!(text.contains("source=\"figma/FRAME/1:2\""), "{text}");
    assert!(text.contains("stroke-dash=\"4 2.5\""), "{text}");
    assert!(text.contains("radius=\"1 2 3 4\""), "{text}");
    assert!(text.contains("transform={"), "{text}");
}

/// Visualizer controls — a plot, a curve with inputs of every source, a
/// handle with offset axes — are markup like any control.
#[test]
fn plots_curves_and_handles_round_trip() {
    use frame::surface::proto::{
        Axis, Binding, Control, ControlKind, Curve, CurveInput, HandleAxis, InputSource, PlotRange,
        PlotStyle, SlotRole,
    };
    let mut s = Scene::new("Viz");
    let page = s.pages()[0];
    let mut area = Node::new(NodeKind::Frame, "area");
    area.size = Size {
        width: 400.0,
        height: 200.0,
    };
    let area = add(&mut s, page, area);
    let line = add(&mut s, area, Node::new(NodeKind::Vector, "line"));
    let ball = add(&mut s, area, Node::new(NodeKind::Ellipse, "ball"));
    let plot = Control::new(ControlKind::Plot {
        style: PlotStyle::Area { baseline: -0.5 },
        x: Axis::Time { len: 240 },
        y: PlotRange {
            min: -24.0,
            max: 0.0,
            curve: Curve::Power { exponent: 0.5 },
        },
    })
    .bound(Binding::stream("comp/gr"))
    .slot(SlotRole::Area, line.0);
    let curve = Control::new(ControlKind::Curve {
        provider: "comp.transfer".into(),
        inputs: vec![
            CurveInput::param("threshold", "comp/threshold"),
            CurveInput::constant("ratio", 4.0),
            CurveInput {
                name: "marker".into(),
                source: InputSource::Stream {
                    id: frame::surface::proto::EntryId::new("comp/in"),
                    channel: Some(1),
                },
            },
        ],
        series: Some("transfer".into()),
        style: PlotStyle::Dots,
        x: Axis::Log {
            f_min: 20.0,
            f_max: 20_000.0,
        },
        y: PlotRange::linear(-60.0, 6.0),
    })
    .slot(SlotRole::Trace, line.0)
    .slot(SlotRole::Marker, ball.0);
    let handle = Control::new(ControlKind::Handle {
        x: Some(Binding::param("env/decay")),
        y: None,
        wheel: Some(Binding::param("env/curve")),
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 4.0 }).after(&["env/attack"], 0.25),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    });
    for (name, c) in [("plot", plot), ("curve", curve), ("handle", handle)] {
        let mut n = Node::new(NodeKind::Frame, name);
        n.control = Some(c);
        add(&mut s, area, n);
    }
    let text = assert_law(&s);
    assert!(
        text.contains("\"Plot\"") && text.contains("\"Curve\"") && text.contains("\"Handle\""),
        "{text}"
    );
}

#[test]
fn every_prop_variant_is_covered() {
    // Setting each Prop to a non-default value on a node round-trips.
    let s = kitchen_sink();
    let all = |f: &dyn Fn(&Node) -> bool| s.nodes.values().any(f);
    let d = Node::new(NodeKind::Frame, "x");
    assert!(all(&|n| n.name.is_empty()));
    assert!(all(&|n| !n.visible));
    assert!(all(&|n| n.locked));
    assert!(all(&|n| n.transform != d.transform));
    assert!(all(&|n| n.size != d.size));
    assert!(all(&|n| n.opacity != d.opacity));
    assert!(all(&|n| n.blend != d.blend));
    assert!(all(&|n| n.corner_radii != d.corner_radii));
    assert!(all(&|n| n.clips_content));
    assert!(all(&|n| n.mask.is_some()));
    assert!(all(&|n| !n.fills.is_empty()));
    assert!(all(&|n| n.stroke != d.stroke));
    assert!(all(&|n| !n.effects.is_empty()));
    assert!(all(&|n| !n.fill_geometry.is_empty()));
    assert!(all(&|n| !n.stroke_geometry.is_empty()));
    assert!(all(&|n| n.svg_snapshot.is_some()));
    assert!(all(&|n| n.layout.is_some()));
    assert!(all(&|n| n.layout_child != d.layout_child));
    assert!(all(&|n| n.source.is_some()));
}

#[test]
fn subtree_and_fragment_round_trip() {
    let s = kitchen_sink();
    let panel = s.node(s.pages()[0]).unwrap().children[0];
    let text = print_subtree(&s, panel).unwrap();
    let nodes = parse_fragment(&text).unwrap();
    let expected: Vec<NodeId> = s.walk(panel);
    assert_eq!(nodes.iter().map(|n| n.id).collect::<Vec<_>>(), expected);
    for n in &nodes {
        let mut want = s.node(n.id).unwrap().clone();
        if n.id == panel {
            want.parent = None;
        }
        assert_eq!(n, &want);
    }
    assert!(print_subtree(&s, NodeId::new()).is_none());
}

// ── Figma parity scenarios ───────────────────────────────────────────────

fn scenario_exports() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("render/tests/parity/scenarios");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let p = entry.unwrap().path();
        for candidate in [
            p.join("latest-bridge-export.json"),
            p.join("export/latest-bridge-export.json"),
        ] {
            if candidate.is_file() {
                out.push(candidate);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn figma_parity_scenarios_round_trip() {
    let exports = scenario_exports();
    assert!(!exports.is_empty(), "no parity scenario exports found");
    let mut passed = 0;
    for path in &exports {
        let bytes = std::fs::read(path).unwrap();
        let scene = frame::figma::import::import_bytes(&bytes)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let text = print(&scene);
        let back = parse(&text, scene.assets.clone())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(back == scene, "{} doesn't round-trip", path.display());
        assert_law(&scene);
        passed += 1;
    }
    eprintln!("{passed}/{} parity scenarios round-trip", exports.len());
}

// ── property test ────────────────────────────────────────────────────────

mod random {
    use super::*;
    use proptest::prelude::*;

    fn f() -> BoxedStrategy<f64> {
        prop_oneof![
            Just(0.0),
            Just(1.0),
            Just(-0.0),
            (-500i32..500).prop_map(f64::from),
            -1e4..1e4f64,
            any::<f64>().prop_filter("finite", |v| v.is_finite()),
        ]
        .boxed()
    }

    fn unit() -> BoxedStrategy<f64> {
        prop_oneof![(0u8..=255).prop_map(|n| f64::from(n) / 255.0), 0.0..=1.0f64].boxed()
    }

    fn color() -> BoxedStrategy<Color> {
        (unit(), unit(), unit(), unit())
            .prop_map(|(r, g, b, a)| Color::rgba(r, g, b, a))
            .boxed()
    }

    fn transform() -> BoxedStrategy<Transform> {
        prop_oneof![
            (f(), f()).prop_map(|(x, y)| Transform::translate(x, y)),
            (-720.0..720.0f64, f(), f()).prop_map(|(deg, tx, ty)| {
                let (s, c) = deg.to_radians().sin_cos();
                Transform {
                    a: c,
                    b: -s,
                    tx,
                    c: s,
                    d: c,
                    ty,
                }
            }),
            (f(), f(), f(), f(), f(), f()).prop_map(|(a, b, tx, c, d, ty)| Transform {
                a,
                b,
                tx,
                c,
                d,
                ty
            }),
        ]
        .boxed()
    }

    fn blend() -> BoxedStrategy<BlendMode> {
        prop_oneof![
            Just(BlendMode::Normal),
            Just(BlendMode::Multiply),
            Just(BlendMode::PassThrough)
        ]
        .boxed()
    }

    fn paint() -> BoxedStrategy<Paint> {
        let kind = prop_oneof![
            color().prop_map(|color| PaintKind::Solid { color }),
            (
                prop::collection::vec((unit(), color()), 0..3),
                prop::option::of(transform())
            )
                .prop_map(|(st, transform)| {
                    PaintKind::LinearGradient {
                        stops: st
                            .into_iter()
                            .map(|(position, color)| GradientStop { position, color })
                            .collect(),
                        transform,
                    }
                }),
            (
                "[0-9a-f]{8}",
                prop::option::of(".*"),
                prop::option::of(transform())
            )
                .prop_map(|(a, source_ref, transform)| {
                    PaintKind::Image {
                        asset: AssetId(a),
                        source_ref,
                        scale_mode: ImageScaleMode::Tile,
                        transform,
                    }
                }),
        ];
        (kind, prop_oneof![Just(1.0), unit()], blend(), any::<bool>())
            .prop_map(|(kind, opacity, blend, visible)| Paint {
                kind,
                opacity,
                blend,
                visible,
            })
            .boxed()
    }

    fn paints() -> BoxedStrategy<Vec<Paint>> {
        prop_oneof![
            Just(vec![]),
            color().prop_map(|c| vec![Paint::solid(c)]),
            prop::collection::vec(paint(), 0..3),
        ]
        .boxed()
    }

    fn opt_f() -> BoxedStrategy<Option<f64>> {
        prop::option::of(f()).boxed()
    }

    fn text_data() -> BoxedStrategy<TextData> {
        (
            ".*",
            prop::option::of("[A-Za-z ]{0,8}"),
            f(),
            opt_f(),
            opt_f(),
            f(),
            prop::collection::vec(
                (".*", paints(), prop::option::of(".*"), opt_f(), opt_f()).prop_map(
                    |(text, fills, font_style, font_size, font_width)| TextRun {
                        text,
                        fills,
                        font_style,
                        font_size,
                        font_width,
                        ..TextRun::default()
                    },
                ),
                0..2,
            ),
            (prop::option::of("[A-Za-z ]{0,12}"), opt_f()),
        )
            .prop_map(
                |(
                    characters,
                    font_family,
                    font_size,
                    font_weight,
                    line_height,
                    letter_spacing,
                    runs,
                    (font_style, font_width),
                )| TextData {
                    characters,
                    font_family,
                    font_size,
                    font_weight,
                    font_style,
                    font_width,
                    line_height,
                    letter_spacing,
                    text_case: TextCase::Upper,
                    align: TextAlign::Center,
                    runs,
                },
            )
            .boxed()
    }

    fn kind() -> BoxedStrategy<NodeKind> {
        prop_oneof![
            Just(NodeKind::Frame),
            Just(NodeKind::Group),
            Just(NodeKind::Rectangle),
            Just(NodeKind::Ellipse),
            Just(NodeKind::Vector),
            Just(NodeKind::Instance { component: None }),
            Just(NodeKind::BooleanOperation {
                op: Some(BooleanOp::Exclude)
            }),
            ".*".prop_map(|tag| NodeKind::Other { tag }),
            text_data().prop_map(NodeKind::Text),
        ]
        .boxed()
    }

    fn layout() -> BoxedStrategy<Option<AutoLayout>> {
        prop::option::of(
            (any::<bool>(), f(), f(), f(), f(), f(), any::<bool>()).prop_map(
                |(h, gap, l, r, t, b, wrap)| AutoLayout {
                    direction: if h {
                        LayoutDirection::Horizontal
                    } else {
                        LayoutDirection::Vertical
                    },
                    gap,
                    padding_left: l,
                    padding_right: r,
                    padding_top: t,
                    padding_bottom: b,
                    wrap,
                    main_align: if wrap { Some(MainAxisAlign::End) } else { None },
                    cross_align: None,
                    hug_main: false,
                    hug_cross: false,
                },
            ),
        )
        .boxed()
    }

    fn node() -> BoxedStrategy<Node> {
        (
            (
                kind(),
                ".*",
                any::<bool>(),
                any::<bool>(),
                transform(),
                f(),
                f(),
                prop_oneof![Just(1.0), unit()],
            ),
            (f(), f(), f(), f(), any::<bool>(), paints(), paints(), f()),
            (
                prop::collection::vec(".*", 0..3),
                prop::option::of(("[a-z/]{0,5}", ".*", "[A-Z/]{0,6}")),
                layout(),
                opt_f(),
                prop::collection::vec(f(), 0..3),
                prop::collection::vec(
                    (color(), f(), f(), any::<bool>()).prop_map(|(color, x, r, visible)| Effect {
                        kind: EffectKind::InnerShadow,
                        visible,
                        color,
                        offset_x: x,
                        offset_y: 0.0,
                        radius: r,
                        spread: 0.0,
                    }),
                    0..2,
                ),
            ),
        )
            .prop_map(
                |(
                    (kind, name, visible, locked, transform, w, h, opacity),
                    (r1, r2, r3, r4, clip, fills, strokes, weight),
                    (geometry, source, layout, grow, dash, effects),
                )| {
                    let mut n = Node::new(kind, name);
                    n.visible = visible;
                    n.locked = locked;
                    n.transform = transform;
                    n.size = Size {
                        width: w,
                        height: h,
                    };
                    n.opacity = opacity;
                    n.corner_radii = CornerRadii {
                        top_left: r1,
                        top_right: r2,
                        bottom_right: r3,
                        bottom_left: r4,
                    };
                    n.clips_content = clip;
                    n.fills = fills;
                    n.stroke.paints = strokes;
                    n.stroke.weight = weight;
                    n.stroke.dash_pattern = dash;
                    n.effects = effects;
                    n.fill_geometry = geometry;
                    n.source = source.map(|(tool, id, node_type)| SourceRef {
                        tool,
                        id,
                        node_type,
                        bounds: None,
                    });
                    n.layout = layout;
                    n.layout_child.grow = grow;
                    n
                },
            )
            .boxed()
    }

    fn scene() -> BoxedStrategy<Scene> {
        prop::collection::vec((node(), any::<prop::sample::Index>()), 0..8)
            .prop_map(|nodes| {
                let mut s = Scene::new("Page");
                let mut ids = vec![s.pages()[0]];
                for (n, parent) in nodes {
                    let p = *parent.get(&ids);
                    ids.push(add(&mut s, p, n));
                }
                s
            })
            .boxed()
    }

    proptest! {
        // 256 cases by default; `PROPTEST_CASES=5000` for a deeper run.
        #[test]
        fn random_scenes_round_trip(s in scene()) {
            let text = print(&s);
            let back = parse(&text, s.assets.clone());
            prop_assert!(back.is_ok(), "{:?}\n{}", back.err(), text);
            prop_assert_eq!(back.unwrap(), s, "{}", text);
        }
    }
}

#[test]
fn assets_pass_through() {
    let s = kitchen_sink();
    assert_eq!(s.assets.len(), 2);
    let back = parse(&print(&s), BTreeMap::new()).unwrap();
    assert!(back.assets.is_empty());
}

/// `cargo test -p document-text --test law dump -- --ignored --nocapture`
#[test]
#[ignore = "debug aid: prints the kitchen sink and a Figma scenario as markup"]
fn dump() {
    println!("{}", print(&kitchen_sink()));
    let p = scenario_exports()
        .into_iter()
        .find(|p| p.to_string_lossy().contains("regression.kitchen-sink"))
        .unwrap();
    let s = frame::figma::import::import_bytes(&std::fs::read(p).unwrap()).unwrap();
    println!("{}", print(&s));
}
