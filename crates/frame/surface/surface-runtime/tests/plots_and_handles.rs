//! Plots, curves and handles through the runtime: a reading scrolls
//! through a history ring (a `History` feed is taken as sent), gaps lift
//! the pen, spectrum bins land where their band puts them, curves come
//! from the registry (an engine's override wins), and a handle drags its
//! params through a rotated container.

use std::collections::BTreeMap;

use document_proto::{Node, NodeId, NodeKind, Op, Scene, Size, Transform};
use surface_proto::{
    Axis, Binding, Catalog, Control, ControlKind, CurveInput, EntryId, FeedInfo, FeedKind,
    HandleAxis, ParamAddr, ParamInfo, PlotRange, PlotStyle, Range, SlotRole, StreamFrame, Unit,
};
use surface_runtime::{Bounds, CurveRegistry, Input, Modifiers, Runtime, Series, Surface};

fn insert(
    scene: &mut Scene,
    parent: NodeId,
    kind: NodeKind,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
) -> NodeId {
    let mut n = Node::new(kind, name);
    n.size = Size {
        width: w,
        height: h,
    };
    n.transform = Transform::translate(x, y);
    let id = n.id;
    scene
        .apply(Op::Insert {
            node: n,
            parent,
            index: u32::MAX,
        })
        .unwrap();
    id
}

fn catalog() -> Catalog {
    Catalog {
        params: vec![
            ParamInfo::new("eq/freq", "Freq", Range::log(20.0, 20_000.0, Unit::Hz))
                .with_default_real(1000.0),
            ParamInfo::new("eq/gain", "Gain", Range::linear(-12.0, 12.0, Unit::Db))
                .with_default_real(0.0),
        ],
        streams: vec![
            FeedInfo::new("gr", "GR", FeedKind::GainReduction),
            FeedInfo::new("gr_history", "GR History", FeedKind::gr_history(8)),
            FeedInfo::new(
                "spec",
                "Spectrum",
                FeedKind::Spectrum {
                    bins: 4,
                    f_min: 100.0,
                    f_max: 1600.0,
                    floor_db: -90.0,
                },
            ),
        ],
        ..Catalog::default()
    }
}

/// A 200×100 area on a page, holding a plot/curve frame (with a `Trace`
/// vector) of `kind` bound to `binding`.
fn plotted(kind: ControlKind, binding: Option<Binding>) -> (Runtime, NodeId, NodeId) {
    let mut scene = Scene::new("s");
    let page = scene.pages()[0];
    let area = insert(
        &mut scene,
        page,
        NodeKind::Frame,
        "area",
        (0.0, 0.0, 200.0, 100.0),
    );
    let plot = insert(
        &mut scene,
        area,
        NodeKind::Frame,
        "plot",
        (0.0, 0.0, 200.0, 100.0),
    );
    let line = insert(
        &mut scene,
        plot,
        NodeKind::Vector,
        "line",
        (0.0, 0.0, 200.0, 100.0),
    );
    let mut c = Control::new(kind).slot(SlotRole::Trace, line.0);
    c.binding = binding;
    let controls = BTreeMap::from([(plot, c)]);
    (
        Runtime::new(Surface { scene, controls }, catalog()),
        plot,
        line,
    )
}

fn path(rt: &Runtime, line: NodeId) -> Option<String> {
    let f = rt.frame(0.0);
    let n = f.node(line)?;
    n.visible
        .then(|| n.fill_geometry.first().cloned())
        .flatten()
}

fn history(len: u32) -> ControlKind {
    ControlKind::Plot {
        style: PlotStyle::Area { baseline: 0.0 },
        x: Axis::Time { len },
        y: PlotRange::linear(-10.0, 0.0),
    }
}

#[test]
fn a_reading_scrolls_through_a_ring_of_its_length() {
    let (mut rt, _, line) = plotted(history(4), Some(Binding::stream("gr")));
    assert_eq!(path(&rt, line), None, "nothing before data");
    for (i, v) in [-1.0, -2.0, -3.0, -4.0, -5.0, -6.0].into_iter().enumerate() {
        rt.apply_frame(
            StreamFrame {
                id: EntryId::new("gr"),
                values: vec![v],
            },
            i as f64,
        );
    }
    // The last four, newest at the right edge: x 0, 66.67, 133.33, 200.
    assert_eq!(
        path(&rt, line).unwrap(),
        "M 0.00 30.00 L 66.67 40.00 L 133.33 50.00 L 200.00 60.00"
    );
    assert!(rt.animating());
}

#[test]
fn a_history_feed_is_drawn_as_sent_and_gaps_lift_the_pen() {
    let (mut rt, _, line) = plotted(history(8), Some(Binding::stream("gr_history")));
    let values = vec![-1.0, -1.0, f64::NAN, -2.0, -2.0, -2.0, -2.0, -2.0];
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("gr_history"),
            values,
        },
        0.0,
    );
    let d = path(&rt, line).unwrap();
    assert_eq!(d.matches('M').count(), 2, "{d}");
    assert!(d.ends_with("L 200.00 20.00"), "{d}");
}

#[test]
fn spectrum_bins_land_where_their_band_puts_them() {
    // Four bins over 100–1600 Hz (an octave each), on an axis over the same band.
    let kind = ControlKind::Plot {
        style: PlotStyle::Line,
        x: Axis::Log {
            f_min: 100.0,
            f_max: 1600.0,
        },
        y: PlotRange::linear(-90.0, 0.0),
    };
    let (mut rt, _, line) = plotted(kind, Some(Binding::stream("spec")));
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("spec"),
            values: vec![0.0, -45.0, -45.0, -90.0],
        },
        0.0,
    );
    // Bin centres at 1/8, 3/8, 5/8, 7/8 of the width.
    assert_eq!(
        path(&rt, line).unwrap(),
        "M 25.00 0.00 L 75.00 50.00 L 125.00 50.00 L 175.00 100.00"
    );
}

#[test]
fn curves_come_from_the_registry_and_an_engine_overrides_a_builtin() {
    let kind = ControlKind::Curve {
        provider: "filter.response".into(),
        inputs: vec![CurveInput::param("freq", "eq/freq")],
        series: None,
        style: PlotStyle::Line,
        x: Axis::Log {
            f_min: 20.0,
            f_max: 20_000.0,
        },
        y: PlotRange::linear(-24.0, 24.0),
    };
    let (rt, plot, line) = plotted(kind, None);
    let s = rt.curve(plot).unwrap();
    assert_eq!(s.name, "response");
    assert!(
        path(&rt, line).unwrap().starts_with("M 0.00 50.00"),
        "flat at 20 Hz for a 1 kHz low-pass"
    );
    let mut own = CurveRegistry::builtin();
    own.register("filter.response", |i: &surface_runtime::CurveInputs| {
        vec![Series::new(
            "exact",
            vec![(20.0, 24.0), (i.or("freq", 0.0), 24.0)],
        )]
    });
    let rt = rt.with_curves(own);
    let p = rt.curve(plot).unwrap().points;
    assert!(p.len() == 2 && (p[1].0 - 1000.0).abs() < 1e-9, "{p:?}");
    let unknown = CurveRegistry::empty();
    let rt = rt.with_curves(unknown);
    assert_eq!(path(&rt, line), None, "no provider: nothing drawn");
}

/// A 200×100 panel turned 90° clockwise about its corner at (200, 0) —
/// covering x ∈ [100, 200], y ∈ [0, 200] — with a 20×20 handle inside
/// bound to freq across (log) and gain up.
fn turned_handle() -> (Runtime, NodeId) {
    let mut scene = Scene::new("s");
    let page = scene.pages()[0];
    let mut panel = Node::new(NodeKind::Frame, "panel");
    panel.size = Size {
        width: 200.0,
        height: 100.0,
    };
    panel.transform = Transform {
        a: 0.0,
        b: -1.0,
        tx: 200.0,
        c: 1.0,
        d: 0.0,
        ty: 0.0,
    };
    let panel_id = panel.id;
    scene
        .apply(Op::Insert {
            node: panel,
            parent: page,
            index: 0,
        })
        .unwrap();
    let handle = insert(
        &mut scene,
        panel_id,
        NodeKind::Frame,
        "handle",
        (90.0, 40.0, 20.0, 20.0),
    );
    let kind = ControlKind::Handle {
        x: Some(Binding::param("eq/freq")),
        y: Some(Binding::param("eq/gain")),
        wheel: None,
        x_map: HandleAxis::new(Axis::Log {
            f_min: 20.0,
            f_max: 20_000.0,
        }),
        y_map: HandleAxis::new(Axis::Linear {
            min: -12.0,
            max: 12.0,
        }),
    };
    let controls = BTreeMap::from([(handle, Control::new(kind))]);
    let mut rt = Runtime::new(Surface { scene, controls }, catalog());
    let boxes = render::build_scene_layout_boxes(&rt.surface().scene, page);
    rt.set_boxes(
        boxes
            .iter()
            .map(|(id, b)| {
                (
                    *id,
                    Bounds {
                        x: b.x,
                        y: b.y,
                        width: b.width,
                        height: b.height,
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    (rt, handle)
}

#[test]
fn a_handle_drags_through_a_rotated_container() {
    let (mut rt, handle) = turned_handle();
    // 1 kHz is 0.566 across the log axis, 0 dB half-way up: the handle's
    // centre sits at (113.2, 50) of the panel — drawn turned, at
    // screen (200 − 50, 113.2) = (150, 113.2).
    let u = (1000.0f64 / 20.0).ln() / 1000.0f64.ln();
    let (sx, sy) = (150.0, 200.0 * u);
    assert_eq!(rt.hit(sx, sy), Some(handle), "hit where it is drawn");
    assert_eq!(
        rt.hit(200.0 * u, 50.0),
        None,
        "not where its unturned box is"
    );
    // Dragging down the screen runs along the panel's width: more freq.
    // Dragging right runs up the panel: more gain.
    let m = Modifiers::default();
    rt.pointer(
        Input::Down {
            x: sx,
            y: sy,
            t: 0.0,
            mods: m,
        },
        0.0,
    );
    rt.pointer(
        Input::Move {
            x: sx + 25.0,
            y: sy + 20.0,
            t: 0.0,
            mods: m,
        },
        16.0,
    );
    rt.pointer(
        Input::Up {
            x: sx + 25.0,
            y: sy + 20.0,
            t: 0.0,
        },
        32.0,
    );
    let freq = rt.real(&ParamAddr::new("eq/freq")).unwrap();
    let gain = rt.real(&ParamAddr::new("eq/gain")).unwrap();
    assert!(
        (freq - 20.0 * 1000f64.powf(u + 0.1)).abs() < 1e-6 * freq,
        "{freq}"
    );
    assert!((gain - 6.0).abs() < 1e-9, "{gain}");
}
