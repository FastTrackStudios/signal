//! The visualizer family: every component places and lints clean against
//! the params and feeds `viz::catalog` publishes, draws its traces, and its
//! handles drag params through the runtime.

use std::collections::BTreeMap;

use frame::document::proto::{NodeId, Scene};
use frame::surface::kit::{Family, Place, components, place, viz};
use frame::surface::proto::{
    Binding, Catalog, ControlKind, EditPhase, EntryId, FeedFrame, ParamAddr,
};
use frame::surface::runtime::{Bounds, EngineCall, Input, Modifiers, Runtime, Surface};
use frame::surface::widgets::{Builder, Theme};

fn catalog() -> Catalog {
    let mut params = viz::catalog::eq_params("eq", 4);
    params.extend(viz::catalog::eq_view_params("eq"));
    params.extend(viz::catalog::comp_params("comp"));
    params.extend(viz::catalog::adsr_params("env"));
    let mut streams = viz::catalog::eq_streams("eq", 64);
    streams.push(viz::catalog::adsr_stream("env", 2));
    streams.extend(viz::catalog::comp_streams("comp", 120));
    streams.push(frame::surface::proto::FeedInfo::new(
        "scope",
        "Scope",
        frame::surface::proto::FeedKind::Waveform { len: 256 },
    ));
    Catalog {
        id: "viz.test".into(),
        version: 1,
        params,
        streams,
        ..Catalog::default()
    }
}

fn placement(id: &str) -> Place {
    match id {
        "viz.eq_graph" => Place::new("EQ")
            .options(&["ns=eq", "bands=4"])
            .bind(Binding::stream("eq/spectrum")),
        "viz.comp" => Place::new("Comp").options(&["ns=comp"]),
        "viz.adsr" => Place::new("Env").options(&["ns=env", "playheads=env/voices", "voices=2"]),
        "viz.spectrum" => Place::new("Spectrum").bind(Binding::stream("eq/spectrum")),
        "viz.gr_history" => Place::new("GR").bind(Binding::stream("comp/gr")),
        "viz.scope" => Place::new("Scope").bind(Binding::stream("scope")),
        other => panic!("no placement for {other}"),
    }
}

/// `id` placed on a page, as a runtime laid out and fed.
fn runtime(id: &str) -> (Runtime, NodeId) {
    let mut scene = Scene::new("viz");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let root = {
        let mut b = Builder {
            scene: &mut scene,
            controls: &mut controls,
            theme: &theme,
        };
        place(&mut b, page, id, &placement(id)).unwrap()
    };
    let mut rt = Runtime::new(Surface { scene, controls }, catalog());
    let boxes = frame::render::build_scene_layout_boxes(&rt.surface().scene, page);
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
    (rt, root)
}

fn named(rt: &Runtime, name: &str) -> NodeId {
    let s = &rt.surface().scene;
    *rt.surface()
        .controls
        .keys()
        .find(|id| s.node(**id).is_some_and(|n| n.name == name))
        .unwrap_or_else(|| panic!("no control `{name}`"))
}

fn value(rt: &Runtime, addr: &str) -> f64 {
    rt.real(&ParamAddr::new(addr)).unwrap()
}

fn centre(b: Bounds) -> (f64, f64) {
    (b.x + b.width / 2.0, b.y + b.height / 2.0)
}

fn drag(rt: &mut Runtime, from: (f64, f64), by: (f64, f64)) -> Vec<EngineCall> {
    let m = Modifiers::default();
    let mut calls = rt.pointer(
        Input::Down {
            x: from.0,
            y: from.1,
            t: 0.0,
            mods: m,
        },
        0.0,
    );
    calls.extend(rt.pointer(
        Input::Move {
            x: from.0 + by.0,
            y: from.1 + by.1,
            t: 16.0,
            mods: m,
        },
        16.0,
    ));
    calls.extend(rt.pointer(
        Input::Up {
            x: from.0 + by.0,
            y: from.1 + by.1,
            t: 32.0,
        },
        32.0,
    ));
    calls.extend(rt.poll(1000.0));
    calls
}

#[test]
fn every_viz_component_places_and_lints_clean() {
    let defs: Vec<_> = components()
        .into_iter()
        .filter(|c| c.family == Family::Viz && !viz::lit::IDS.contains(&c.id))
        .collect();
    assert_eq!(defs.len(), 6);
    for def in defs {
        assert!(def.id.starts_with("viz."));
        let (rt, _) = runtime(def.id);
        let scene = rt.surface().to_scene();
        let issues = frame::design::lint::lint(
            &scene,
            rt.catalog(),
            &frame::design::lint::Config::default(),
        );
        let errors: Vec<String> = issues
            .iter()
            .filter(|i| i.severity == frame::design::lint::Severity::Error)
            .map(ToString::to_string)
            .collect();
        assert!(errors.is_empty(), "{}:\n{}", def.id, errors.join("\n"));
        assert!(
            rt.surface()
                .controls
                .values()
                .any(|c| matches!(c.kind, ControlKind::Plot { .. } | ControlKind::Curve { .. })),
            "{} draws nothing",
            def.id
        );
    }
}

#[test]
fn dragging_an_eq_handle_moves_its_band_and_redraws_the_curve() {
    let (mut rt, _) = runtime("viz.eq_graph");
    let handle = named(&rt, "band 2 handle");
    let response = named(&rt, "response");
    let (f0, g0, q0) = (
        value(&rt, "eq/b2_freq"),
        value(&rt, "eq/b2_gain"),
        value(&rt, "eq/b2_q"),
    );
    let before = rt.curve(response).unwrap();
    let evals = rt.curve_evaluations();

    let at = centre(rt.hit_box(handle).unwrap());
    let drawn_before = rt.frame(0.0).node(handle).unwrap().transform;
    assert_eq!(
        rt.hit(at.0, at.1),
        Some(handle),
        "the handle is hit where it is drawn"
    );
    let calls = drag(&mut rt, at, (80.0, -40.0));
    let sent = |addr: &str| {
        calls
            .iter()
            .any(|c| matches!(c, EngineCall::SetParam { addr: a, .. } if a.0 == addr))
    };
    assert!(sent("eq/b2_freq") && sent("eq/b2_gain"), "{calls:?}");
    assert!(
        calls.iter().any(|c| matches!(
            c,
            EngineCall::SetParam {
                phase: EditPhase::End,
                ..
            }
        )),
        "the gesture ends"
    );
    let (f1, g1) = (value(&rt, "eq/b2_freq"), value(&rt, "eq/b2_gain"));
    assert!(f1 > f0 * 1.5, "right = higher: {f0} → {f1}");
    assert!(g1 > g0 + 2.0, "up = more gain: {g0} → {g1}");
    assert!((value(&rt, "eq/b2_q") - q0).abs() < 1e-9, "Q untouched");

    // The combined response follows, and the handle is where its values put it.
    let after = rt.curve(response).unwrap();
    assert_ne!(before, after);
    assert!(rt.curve_evaluations() > evals);
    let moved = centre(rt.hit_box(handle).unwrap());
    assert!(
        (moved.0 - at.0 - 80.0).abs() < 1.0 && (moved.1 - at.1 + 40.0).abs() < 1.0,
        "{at:?} → {moved:?}"
    );
    // …and drawn there: the frame moves it as far as its hit box moved.
    let drawn = rt.frame(0.0).node(handle).unwrap().transform;
    assert!(
        (drawn.tx - drawn_before.tx - (moved.0 - at.0)).abs() < 1e-6
            && (drawn.ty - drawn_before.ty - (moved.1 - at.1)).abs() < 1e-6
    );

    // Shift is fine; the wheel turns Q; double-click renames.
    let fine = centre(rt.hit_box(handle).unwrap());
    let m = Modifiers {
        fine: true,
        ..Modifiers::default()
    };
    rt.pointer(
        Input::Down {
            x: fine.0,
            y: fine.1,
            t: 0.0,
            mods: m,
        },
        2000.0,
    );
    rt.pointer(
        Input::Move {
            x: fine.0 + 100.0,
            y: fine.1,
            t: 0.0,
            mods: m,
        },
        2016.0,
    );
    rt.pointer(
        Input::Up {
            x: fine.0 + 100.0,
            y: fine.1,
            t: 0.0,
        },
        2032.0,
    );
    let f2 = value(&rt, "eq/b2_freq");
    assert!(f2 > f1 && f2 < f1 * 1.5, "fine: {f1} → {f2}");
    rt.feed(
        handle,
        Input::Wheel {
            notches: 3.0,
            mods: Modifiers::default(),
        },
        3000.0,
    );
    assert!(value(&rt, "eq/b2_q") > q0, "wheel raises Q");
    let at = centre(rt.hit_box(handle).unwrap());
    rt.pointer(
        Input::Down {
            x: at.0,
            y: at.1,
            t: 0.0,
            mods: Modifiers::default(),
        },
        4000.0,
    );
    rt.pointer(Input::DoubleClick, 4000.0);
    rt.pointer(
        Input::Up {
            x: at.0,
            y: at.1,
            t: 0.0,
        },
        4010.0,
    );
    assert!(rt.editing().is_some(), "double-click renames the band");
}

#[test]
fn curves_evaluate_once_per_change_and_share_across_bands() {
    let (mut rt, _) = runtime("viz.eq_graph");
    let _ = rt.frame(0.0);
    let evals = rt.curve_evaluations();
    // The combined curve once; each band's curve (shared with its dynamic
    // range) and its stem once.
    // (And the dB grid, `eq.grid`, once.)
    assert_eq!(evals, 1 + 2 * 4 + 1);
    let _ = rt.frame(16.0);
    assert_eq!(rt.curve_evaluations(), evals, "nothing changed");
    // A spectrum frame moves the plot, not the curves.
    rt.apply_frame(
        FeedFrame {
            id: EntryId::new("eq/spectrum"),
            values: vec![-40.0; 64],
        },
        20.0,
    );
    let _ = rt.frame(32.0);
    assert_eq!(rt.curve_evaluations(), evals);
    rt.apply_values([frame::surface::proto::ParamValue {
        addr: ParamAddr::new("eq/b3_gain"),
        value: 0.8,
    }]);
    let _ = rt.frame(48.0);
    assert_eq!(
        rt.curve_evaluations(),
        evals + 3,
        "the sum, band 3's curve and its stem"
    );
}

#[test]
fn the_spectrum_plot_draws_its_feed_and_asks_for_its_width() {
    let (mut rt, _) = runtime("viz.spectrum");
    let plot = named(&rt, "spectrum");
    let line = |rt: &Runtime| {
        let f = rt.frame(0.0);
        let c = &rt.surface().controls[&plot];
        let id = c
            .slots
            .iter()
            .find(|s| s.role == frame::surface::proto::SlotRole::Trace)
            .unwrap()
            .node;
        f.node(NodeId(id))
            .map(|n| (n.visible, n.fill_geometry.clone()))
            .unwrap()
    };
    let (_, empty) = line(&rt);
    assert!(empty.is_empty(), "nothing before data");
    let values: Vec<f64> = (0..64).map(|i| -30.0 - f64::from(i) * 0.5).collect();
    rt.apply_frame(
        FeedFrame {
            id: EntryId::new("eq/spectrum"),
            values,
        },
        0.0,
    );
    let (visible, d) = line(&rt);
    assert!(visible && d[0].starts_with('M'), "{d:?}");
    let reqs = rt.feed_requests();
    assert_eq!(reqs.len(), 1);
    let width = rt.surface().scene.node(plot).unwrap().size.width;
    assert_eq!(reqs[0].options.bins, Some((width / 2.0).round() as u16));
}

#[test]
fn the_compressor_rides_its_curve_and_drags_its_threshold() {
    let (mut rt, _) = runtime("viz.comp");
    let transfer = named(&rt, "transfer");
    let ball = rt.surface().controls[&transfer]
        .slots
        .iter()
        .find(|s| s.role == frame::surface::proto::SlotRole::Marker)
        .unwrap()
        .node;
    assert!(
        !rt.frame(0.0).node(NodeId(ball)).unwrap().visible,
        "no input yet: no ball"
    );
    rt.apply_frame(
        FeedFrame {
            id: EntryId::new("comp/in"),
            values: vec![-12.0],
        },
        0.0,
    );
    rt.apply_frame(
        FeedFrame {
            id: EntryId::new("comp/gr_history"),
            values: vec![-3.0; 120],
        },
        0.0,
    );
    let f = rt.frame(0.0);
    assert!(f.node(NodeId(ball)).unwrap().visible);
    let gr = named(&rt, "gain reduction");
    let readout = rt.surface().controls[&gr]
        .slots
        .iter()
        .find(|s| s.role == frame::surface::proto::SlotRole::Value)
        .unwrap()
        .node;
    assert_eq!(
        f.node(NodeId(readout)).unwrap().text().unwrap().characters,
        "-3.0 dB"
    );

    let thr = named(&rt, "threshold");
    let t0 = value(&rt, "comp/threshold");
    let at = centre(rt.hit_box(thr).unwrap());
    drag(&mut rt, at, (-30.0, 25.0));
    assert!(
        value(&rt, "comp/threshold") < t0 - 3.0,
        "left = lower threshold"
    );
    rt.feed(
        thr,
        Input::Wheel {
            notches: 5.0,
            mods: Modifiers::default(),
        },
        2000.0,
    );
    assert!(
        value(&rt, "comp/ratio") > 4.0,
        "the wheel over the threshold turns the ratio"
    );
}

#[test]
fn adsr_handles_follow_the_stages_before_them() {
    let (mut rt, _) = runtime("viz.adsr");
    let d = named(&rt, "D handle");
    let r = named(&rt, "R handle");
    let r_before = rt.hit_box(r).unwrap();
    let (d0, s0) = (value(&rt, "env/decay"), value(&rt, "env/sustain"));
    let at = centre(rt.hit_box(d).unwrap());
    drag(&mut rt, at, (40.0, 30.0));
    assert!(value(&rt, "env/decay") > d0, "right = longer decay");
    assert!(value(&rt, "env/sustain") < s0, "down = lower sustain");
    assert!(
        rt.hit_box(r).unwrap().x > r_before.x + 30.0,
        "the release point moves with the decay"
    );
    assert_eq!(
        rt.hit_box(r).unwrap().y,
        r_before.y,
        "and stays on the floor"
    );

    // Voices ride the curve.
    let heads = named(&rt, "playheads");
    rt.apply_frame(
        FeedFrame {
            id: EntryId::new("env/voices"),
            values: vec![1.0, 0.5, 0.8, -1.0, 0.0, 0.0],
        },
        0.0,
    );
    let s = rt.curve(heads).unwrap();
    assert_eq!(s.points.len(), 1, "one voice sounding: {s:?}");
    let t = value(&rt, "env/attack") + 0.5 * value(&rt, "env/decay");
    assert!((s.points[0].0 - t).abs() < 1e-9);
}
