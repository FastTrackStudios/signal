//! The lit visualizers (`viz.comp_scope`, `viz.delay_taps`,
//! `viz.reverb_space`, `viz.mod_lfo`, `viz.eq_glow`): each places and lints
//! clean against `viz::lit_catalog`, its shader's inputs resolve against a
//! runtime, and its vector fallback draws.

use std::collections::BTreeMap;

use frame::document::proto::{NodeId, Scene};
use frame::surface::kit::{Family, Place, components, place, viz};
use frame::surface::proto::{Catalog, ControlKind, FeedFrame, ParamAddr, ParamValue};
use frame::surface::runtime::{Bounds, Input, Modifiers, Runtime, Surface};
use frame::surface::widgets::{Builder, Theme};

fn catalog() -> Catalog {
    let (mut params, mut streams) = viz::lit_catalog::comp_scope("comp", 120);
    params.extend(viz::lit_catalog::delay_params("delay"));
    params.extend(viz::lit_catalog::reverb_params("reverb"));
    params.extend(viz::lit_catalog::mod_params("mod"));
    let (eq, eq_feeds) = viz::lit_catalog::eq_glow("eq", 4, 128);
    params.extend(eq);
    streams.extend(eq_feeds);
    Catalog {
        id: "viz.lit".into(),
        version: 1,
        params,
        streams,
        ..Catalog::default()
    }
}

fn placement(id: &str) -> Place {
    match id {
        "viz.comp_scope" => Place::new("Comp").options(&["ns=comp", "len=120", "on"]),
        "viz.delay_taps" => Place::new("Delay").options(&["ns=delay"]),
        "viz.reverb_space" => Place::new("Reverb").options(&["ns=reverb"]),
        "viz.mod_lfo" => Place::new("Mod").options(&["ns=mod", "color=#f472b6"]),
        "viz.eq_glow" => Place::new("Glow").options(&["ns=eq", "bands=4", "ground"]),
        other => panic!("no placement for {other}"),
    }
}

fn runtime(id: &str) -> (Runtime, NodeId) {
    let mut scene = Scene::new("lit");
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
    let catalog = catalog();
    let mut rt = Runtime::new(Surface { scene, controls }, catalog.clone());
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
    for f in surface_memory_frames(&catalog) {
        rt.apply_frame(f, 0.0);
    }
    rt.set_clock(1.25);
    (rt, root)
}

fn surface_memory_frames(catalog: &Catalog) -> Vec<FeedFrame> {
    frame::surface::memory::Memory::new(catalog.clone()).frames(1.0)
}

/// The light: the node carrying the component's shader.
fn light(rt: &Runtime) -> (NodeId, frame::document::proto::ShaderFx) {
    rt.surface()
        .scene
        .nodes
        .values()
        .find_map(|n| n.shader.clone().map(|s| (n.id, s)))
        .expect("a lit node")
}

#[test]
fn the_lit_components_are_listed() {
    let ids: Vec<&str> = components()
        .into_iter()
        .filter(|c| c.family == Family::Viz)
        .map(|c| c.id)
        .collect();
    for id in viz::lit::IDS {
        assert!(ids.contains(&id), "{id} is not in the kit");
    }
}

#[test]
fn every_lit_component_places_lints_clean_and_feeds_its_shader() {
    for (id, shader) in viz::lit::IDS.into_iter().zip([
        "viz.comp",
        "viz.delay",
        "viz.reverb",
        "viz.modulation",
        "eq.glow",
    ]) {
        let (rt, _) = runtime(id);
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
        assert!(errors.is_empty(), "{id}:\n{}", errors.join("\n"));
        for (nid, c) in &rt.surface().controls {
            let issues = c.validate(rt.catalog());
            assert!(
                issues.is_empty(),
                "{id} {}: {issues:?}",
                rt.surface()
                    .scene
                    .node(*nid)
                    .map_or("?", |n| n.name.as_str())
            );
        }

        let (node, fx) = light(&rt);
        assert_eq!(fx.shader, shader);
        let inputs = rt.shader_inputs();
        let data = &inputs[&node];
        for i in &fx.inputs {
            assert!(
                data.get(&i.name).is_some_and(|v| !v.is_empty()),
                "{id}: `{}` has no data",
                i.name
            );
        }
        // Every input the shader reads is bound (the rest default in WGSL).
        assert!(fx.inputs.len() <= frame::document::proto::shader_input::MAX_INPUTS);

        // The fallback draws: at least one curve or plot has points.
        let frame = rt.frame(0.0);
        let drawn = rt
            .surface()
            .controls
            .iter()
            .filter(|(_, c)| matches!(c.kind, ControlKind::Plot { .. } | ControlKind::Curve { .. }))
            .any(|(cid, c)| {
                c.slots.iter().any(|s| {
                    frame.node(NodeId(s.node)).is_some_and(|n| {
                        n.fill_geometry
                            .iter()
                            .chain(&n.stroke_geometry)
                            .any(|g| !g.is_empty())
                    })
                }) && rt.surface().scene.node(*cid).is_some()
            });
        assert!(drawn, "{id}: the fallback draws nothing");
    }
}

#[test]
fn the_compressor_feeds_its_traces_mapped_to_the_display_range() {
    let (rt, _) = runtime("viz.comp_scope");
    let (node, _) = light(&rt);
    let data = &rt.shader_inputs()[&node];
    assert_eq!(data["input"].len(), 128);
    assert_eq!(data["gr"].len(), 128);
    assert!(
        data["input"]
            .iter()
            .chain(&data["gr"])
            .all(|v| (0.0..=1.0).contains(v))
    );
    assert!(data["gr_db"][0] >= 0.0, "gain reduction arrives positive");
    assert!((data["threshold"][0] - -24.0).abs() < 1e-3);
}

#[test]
fn dragging_the_threshold_rule_sets_the_threshold() {
    let (mut rt, _) = runtime("viz.comp_scope");
    let handle = *rt
        .surface()
        .controls
        .iter()
        .find(|(_, c)| matches!(c.kind, ControlKind::Handle { .. }))
        .map(|(id, _)| id)
        .expect("threshold handle");
    let b = rt.hit_box(handle).expect("laid out");
    let (x, y) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
    let before = rt.real(&ParamAddr::new("comp/threshold")).unwrap();
    let m = Modifiers::default();
    rt.pointer(
        Input::Down {
            x,
            y,
            t: 0.0,
            mods: m,
        },
        0.0,
    );
    rt.pointer(
        Input::Move {
            x,
            y: y + 60.0,
            t: 16.0,
            mods: m,
        },
        16.0,
    );
    rt.pointer(
        Input::Up {
            x,
            y: y + 60.0,
            t: 32.0,
        },
        32.0,
    );
    let after = rt.real(&ParamAddr::new("comp/threshold")).unwrap();
    assert!(
        after < before - 3.0,
        "dragging down lowers the threshold: {before} → {after}"
    );
}

#[test]
fn the_delay_taps_follow_its_params() {
    let (mut rt, _) = runtime("viz.delay_taps");
    let (node, _) = light(&rt);
    let lefts = |rt: &Runtime| rt.shader_inputs()[&node]["left"].len() / 2;
    let short = lefts(&rt);
    // More feedback, a longer tail.
    let fb = rt
        .catalog()
        .param(&ParamAddr::new("delay/feedback"))
        .unwrap()
        .range
        .to_normalized(0.9);
    rt.apply_values([ParamValue {
        addr: ParamAddr::new("delay/feedback"),
        value: fb,
    }]);
    assert!(lefts(&rt) > short, "{short} → {}", lefts(&rt));
}

#[test]
fn the_name_lists_match_the_runtime() {
    use frame::surface::runtime::viz_curves as rv;
    assert_eq!(
        viz::lit_catalog::DIVISIONS.to_vec(),
        rv::DIVISIONS.iter().map(|d| d.0).collect::<Vec<_>>()
    );
    assert_eq!(viz::lit_catalog::DELAY_FAMILIES, rv::DELAY_FAMILIES);
    assert_eq!(viz::lit_catalog::REVERB_FAMILIES, rv::REVERB_FAMILIES);
    assert_eq!(viz::lit_catalog::MOD_ENGINES, rv::MOD_ENGINES);
}
