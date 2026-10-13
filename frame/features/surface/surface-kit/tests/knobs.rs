//! The knob family: every style places, wires one needle, scales with
//! `Place::size`, rejects unknown variants, and lints clean.

use std::collections::BTreeMap;

use document_proto::{NodeId, NodeKind, Paint, PaintKind, Scene, TextCase};
use surface_kit::{DARK_INK, Family, Ink, Place, TOUCH_MIN, components, draw, place};
use surface_proto::{Binding, Control, ControlKind, ParamAddr, ParamValue, SlotRole};
use surface_runtime::{Bounds, Runtime, Surface};
use surface_widgets::{Builder, Theme};

fn knob_ids() -> Vec<&'static str> {
    components().into_iter().filter(|c| c.family == Family::Knob).map(|c| c.id).collect()
}

/// Place `id` into a fresh scene; returns the scene (controls attached to
/// their nodes) and the root.
fn placed(id: &str, p: &Place) -> Result<(Scene, NodeId, Control), String> {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let root = {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        place(&mut b, page, id, p)?
    };
    for (n, c) in &controls {
        scene.node_mut(*n).expect("control node").control = Some(c.clone());
    }
    let control = controls.get(&root).cloned().ok_or("no control on the root")?;
    Ok((scene, root, control))
}

#[test]
fn the_kit_has_every_source_style() {
    let ids = knob_ids();
    for id in [
        "knob.bakelite",
        "knob.metal",
        "knob.skirted",
        "knob.daka",
        "knob.marconi",
        "knob.collet",
        "knob.silver_top",
        "knob.metal_fluted",
        "knob.pointer",
        "knob.neve",
        "knob.soft_pointer",
        "knob.chicken_head",
        "knob.dial",
    ] {
        assert!(ids.contains(&id), "missing {id}");
    }
}

#[test]
fn every_knob_places_with_one_needle_inside_it() {
    for id in knob_ids() {
        let (scene, root, control) = placed(id, &Place::new("Gain")).unwrap();
        assert!(matches!(control.kind, ControlKind::Knob { sweep_degrees } if sweep_degrees > 0.0), "{id}");
        let needles: Vec<_> = control.slots_of(SlotRole::Needle).collect();
        assert_eq!(needles.len(), 1, "{id} has {} needles", needles.len());
        let needle = NodeId(needles[0]);
        let subtree = scene.walk(root);
        assert!(subtree.contains(&needle) && needle != root, "{id}'s needle is not a descendant");
        // It turns about its parent's centre, which must be the knob's
        // shaft: the needle is the knob's body — a square frame filling the
        // square knob frame, with the turning parts inside it.
        let n = scene.node(needle).unwrap();
        assert!(matches!(n.kind, NodeKind::Frame), "{id}: the needle is a frame");
        assert!(!n.children.is_empty(), "{id}: the body carries the turning parts");
        let parent = scene.node(n.parent.unwrap()).unwrap();
        assert!((parent.size.width - parent.size.height).abs() < 1e-9, "{id}: pivot frame not square");
        assert_eq!(n.size, parent.size, "{id}: the body fills the pivot frame");
        assert_eq!(n.transform, document_proto::Transform::IDENTITY, "{id}: drawn at the centre of travel");
        assert!(subtree.len() < 200, "{id} is {} nodes", subtree.len());
        assert_eq!(control.slots_of(SlotRole::Label).count(), 1, "{id}");
    }
}

/// Text inside `id`'s needle frame.
fn turning_text(id: &str, p: &Place) -> Vec<String> {
    let (scene, _, control) = placed(id, p).unwrap();
    let needle = NodeId(control.slots_of(SlotRole::Needle).next().unwrap());
    scene.walk(needle).into_iter().filter_map(|n| scene.node(n)?.text().map(|t| t.characters.clone())).collect()
}

#[test]
fn a_dials_numerals_are_text_that_turns_and_a_panel_scale_does_not() {
    let turning = turning_text("knob.dial", &Place::new("x").scale(&["0", "5", "10"]));
    assert_eq!(turning, vec!["0", "5", "10"]);
    assert!(turning_text("knob.daka", &Place::new("x")).is_empty(), "the Pultec's scale is printed on the panel");
}

#[test]
fn the_rooms_light_does_not_turn() {
    for id in knob_ids() {
        let (scene, _, control) = placed(id, &Place::new("x")).unwrap();
        let needle = NodeId(control.slots_of(SlotRole::Needle).next().unwrap());
        for n in scene.walk(needle) {
            let name = &scene.node(n).unwrap().name;
            assert!(!name.contains("light") && name != "hub", "{id}: {name} turns");
        }
    }
}

#[test]
fn sizes_scale_with_place_size() {
    for id in knob_ids() {
        // Above the size where the printed scale's type floor holds it.
        let (a, ra, _) = placed(id, &Place::new("A").size(80.0)).unwrap();
        let (b, rb, _) = placed(id, &Place::new("B").size(160.0)).unwrap();
        let (wa, wb) = (a.node(ra).unwrap().size.width, b.node(rb).unwrap().size.width);
        assert!((wb / wa - 2.0).abs() < 0.02, "{id}: {wa} → {wb}");
    }
}

#[test]
fn unknown_variants_are_rejected_and_known_ones_place() {
    for def in components().into_iter().filter(|c| c.family == Family::Knob) {
        let err = placed(def.id, &Place::new("x").variant("plaid")).unwrap_err();
        assert!(err.contains("plaid"), "{err}");
        for v in def.variants {
            placed(def.id, &Place::new("x").variant(*v)).unwrap_or_else(|e| panic!("{} {v}: {e}", def.id));
        }
    }
}

#[test]
fn a_legend_and_a_scale_are_honoured() {
    let (scene, root, _) = placed("knob.bakelite", &Place::new("x").legend("Input").scale(&["-48", "0", "+12"])).unwrap();
    let texts: Vec<String> =
        scene.walk(root).into_iter().filter_map(|id| scene.node(id)?.text().map(|t| t.characters.clone())).collect();
    for s in ["Input", "-48", "0", "+12"] {
        assert!(texts.iter().any(|t| t == s), "missing {s} in {texts:?}");
    }
}

#[test]
fn every_knob_lints_clean_bound_to_a_param() {
    let catalog = surface_memory::demo_catalog();
    for id in knob_ids() {
        // (The cap of a concentric pair binds its own param; others ignore it.)
        let p = Place::new("Gain").bind(Binding::param("amp/gain")).inner(Place::new("Bass").bind(Binding::param("amp/bass")));
        let (scene, _, _) = placed(id, &p).unwrap();
        let issues = design_lint::lint(&scene, &catalog, &design_lint::Config::default());
        assert!(issues.is_empty(), "{id}: {}", issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"));
    }
}

fn solid(p: &Paint) -> Option<String> {
    match &p.kind {
        PaintKind::Solid { color } => Some(format!("{:.3},{:.3},{:.3}", color.r, color.g, color.b)),
        _ => None,
    }
}

fn hex(c: &str) -> String {
    let c = draw::hex(c);
    format!("{:.3},{:.3},{:.3}", c.r, c.g, c.b)
}

/// The inks of the printed parts (scale, numerals, legend) of a placed knob.
fn printed_inks(id: &str, p: &Place) -> Vec<(String, String)> {
    let (scene, root, _) = placed(id, p).unwrap();
    scene
        .walk(root)
        .into_iter()
        .filter_map(|n| scene.node(n))
        .filter(|n| n.name.starts_with("scale ") || n.name == "numeral" || n.name == "legend")
        .map(|n| (n.name.clone(), n.fills.first().and_then(solid).unwrap_or_default()))
        .collect()
}

#[test]
fn the_print_takes_the_place_s_ink() {
    for id in ["knob.bakelite", "knob.neve", "knob.daka", "knob.concentric"] {
        let p = Place::new("x").legend("Drive");
        let light = printed_inks(id, &p);
        assert!(light.len() > 3, "{id}: {light:?}");
        assert!(light.iter().all(|(_, c)| *c != hex(DARK_INK)), "{id}: light by default");
        let dark = printed_inks(id, &p.clone().ink(Ink::Dark));
        assert!(dark.iter().all(|(_, c)| *c == hex(DARK_INK)), "{id}: {dark:?}");
        let red = printed_inks(id, &p.clone().ink(Ink::Color("#b3281f".into())));
        assert!(red.iter().all(|(_, c)| *c == hex("#b3281f")), "{id}: {red:?}");
    }
}

#[test]
fn legends_are_capitals_unless_the_case_is_kept() {
    let case = |p: &Place| {
        let (scene, _, control) = placed("knob.bakelite", p).unwrap();
        let legend = NodeId(control.slots_of(SlotRole::Label).next().unwrap());
        scene.node(legend).unwrap().text().unwrap().text_case
    };
    assert_eq!(case(&Place::new("x").legend("Drive")), TextCase::Upper);
    assert_eq!(case(&Place::new("x").legend("kHz").keep_case()), TextCase::Original);
}

fn sweep_of(c: &Control) -> f64 {
    match c.kind {
        ControlKind::Knob { sweep_degrees } => sweep_degrees,
        ref k => panic!("not a knob: {}", k.name()),
    }
}

#[test]
fn a_stepped_knob_turns_through_its_detents() {
    let (scene, root, c) = placed("knob.chicken_head", &Place::new("Mode").options(&["Hall", "Room", "Plate", "Spring"])).unwrap();
    assert!((sweep_of(&c) - 180.0).abs() < 1e-9, "four detents, 60° apart");
    let printed: Vec<String> = scene.walk(root).into_iter().filter_map(|n| scene.node(n).filter(|n| n.name == "numeral")?.text().map(|t| t.characters.clone())).collect();
    assert_eq!(printed, ["Hall", "Room", "Plate", "Spring"]);
    // Many positions share 300°; an explicit sweep wins over both.
    let (_, _, c) = placed("knob.pointer", &Place::new("x").options(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"])).unwrap();
    assert!((sweep_of(&c) - 300.0).abs() < 1e-9);
    let (_, _, c) = placed("knob.bakelite", &Place::new("x").sweep(270.0)).unwrap();
    assert!((sweep_of(&c) - 270.0).abs() < 1e-9);
    let (_, _, c) = placed("knob.bakelite", &Place::new("x")).unwrap();
    assert!((sweep_of(&c) - 300.0).abs() < 1e-9, "a continuous knob keeps its style's travel");
}

/// The shaft of a placed knob, in its root's space.
fn shaft(scene: &Scene, root: NodeId) -> (f64, f64) {
    let knob = scene.node(root).unwrap().children.iter().find_map(|c| scene.node(*c).filter(|n| n.name == "knob")).unwrap();
    (knob.transform.tx + knob.size.width / 2.0, knob.transform.ty + knob.size.height / 2.0)
}

#[test]
fn a_touch_box_is_the_knob_s_body_padded_to_a_fingertip() {
    for id in knob_ids() {
        let loose = placed(id, &Place::new("x").size(64.0)).unwrap();
        let tight = placed(id, &Place::new("x").size(64.0).touch(TOUCH_MIN)).unwrap();
        let (lw, tw) = (loose.0.node(loose.1).unwrap().size, tight.0.node(tight.1).unwrap().size);
        assert!(tw.width == 64.0 && tw.height == 64.0, "{id}: {tw:?}");
        if id != "knob.concentric" {
            assert!(lw.width > 64.0 * 1.2, "{id}: the default box still holds the scale ({lw:?})");
            assert_eq!(shaft(&tight.0, tight.1), (32.0, 32.0), "{id}: the box is centred on the shaft");
        }
        // Small knobs are padded out to the touch minimum.
        let small = placed(id, &Place::new("x").size(30.0).touch(TOUCH_MIN)).unwrap();
        let sw = small.0.node(small.1).unwrap().size;
        assert_eq!((sw.width, sw.height), (44.0, 44.0), "{id}");
    }
    // The print hangs outside the box, where it was.
    let (scene, root, c) = placed("knob.bakelite", &Place::new("x").size(64.0).legend("Gain").touch(TOUCH_MIN)).unwrap();
    let legend = scene.node(NodeId(c.slots_of(SlotRole::Label).next().unwrap())).unwrap();
    assert!(legend.transform.ty > 64.0, "legend under the box: {}", legend.transform.ty);
    let (x, y, w, h) = surface_kit::drawn_bounds(&scene, root);
    assert!(x < 0.0 && y < 0.0 && w > 64.0 && h > 64.0, "{x} {y} {w} {h}");
}

#[test]
fn a_cell_holds_what_a_tight_knob_draws() {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
    let root = surface_kit::place_in_cell(&mut b, page, "knob.bakelite", &Place::new("x").size(64.0).legend("Gain").touch(TOUCH_MIN)).unwrap();
    let holder = b.scene.node(root).unwrap().parent.unwrap();
    let (x, y, w, h) = surface_kit::drawn_bounds(b.scene, root);
    let hs = b.scene.node(holder).unwrap().size;
    assert_eq!((hs.width, hs.height), (w, h));
    let t = b.scene.node(root).unwrap().transform;
    assert_eq!((t.tx, t.ty), (-x, -y), "nothing hangs out of the cell");
}

fn concentric() -> (Runtime, NodeId, NodeId) {
    let p = Place::new("Freq").size(80.0).bind(Binding::param("amp/gain")).inner(Place::new("Boost").bind(Binding::param("amp/bass")).legend("Boost"));
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let root = {
        let mut b = Builder { scene: &mut scene, controls: &mut controls, theme: &theme };
        let root = place(&mut b, page, "knob.concentric", &p).unwrap();
        b.scene.node_mut(root).unwrap().transform = document_proto::Transform::translate(100.0, 100.0);
        root
    };
    let cap = *controls.keys().find(|k| **k != root).expect("two controls");
    for (n, c) in &controls {
        scene.node_mut(*n).unwrap().control = Some(c.clone());
    }
    let mut rt = Runtime::new(Surface::from_scene(scene), surface_memory::demo_catalog());
    let boxes = render::build_scene_layout_boxes(&rt.surface().scene, page)
        .into_iter()
        .map(|(id, b)| (id, Bounds { x: b.x, y: b.y, width: b.width, height: b.height }))
        .collect::<Vec<_>>();
    rt.set_boxes(boxes);
    (rt, root, cap)
}

#[test]
fn a_concentric_pair_is_two_controls_on_one_shaft() {
    let (rt, collar, cap) = concentric();
    let scene = rt.surface().to_scene();
    assert_eq!(rt.surface().controls.len(), 2);
    assert_eq!(scene.node(cap).unwrap().parent, Some(collar), "the cap's control is nested in the collar's");
    let issues = design_lint::lint(&scene, rt.catalog(), &design_lint::Config::default());
    let (errors, warnings, _) = design_lint::counts(&issues);
    assert_eq!((errors, warnings), (0, 0), "{}", issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"));
    // The cap takes its disc; the collar the ring round it.
    assert_eq!(rt.hit(140.0, 140.0), Some(cap), "centre");
    assert_eq!(rt.hit(140.0, 104.0), Some(collar), "top of the collar ring");
    assert_eq!(rt.hit(40.0, 40.0), None);
}

#[test]
fn turning_the_collar_leaves_the_cap_alone() {
    let (mut rt, collar, cap) = concentric();
    let needle = |id: NodeId| NodeId(rt.surface().controls[&id].slots_of(SlotRole::Needle).next().unwrap());
    let (outer, inner) = (needle(collar), needle(cap));
    assert!(!rt.surface().scene.walk(outer).contains(&inner), "the cap doesn't ride on the collar's body");
    rt.apply_values([ParamValue { addr: ParamAddr::new("amp/gain"), value: 1.0 }, ParamValue { addr: ParamAddr::new("amp/bass"), value: 0.5 }]);
    let f = rt.frame(0.0);
    assert!((f.node(outer).unwrap().transform.rotation().to_degrees() - 150.0).abs() < 1e-6);
    assert!(f.node(inner).unwrap().transform.rotation().abs() < 1e-9);
}

/// The printed labels round a placed knob: `(text, font size, how near the
/// set words come to the shaft)` — the glyphs' estimated box, anchored as
/// the label is aligned, measured from the centre of the `knob` frame.
fn label_reach(scene: &Scene, root: NodeId) -> Vec<(String, f64, f64)> {
    let knob = scene.node(root).unwrap().children.iter().copied().find(|c| scene.node(*c).unwrap().name == "knob").unwrap();
    let side = scene.node(knob).unwrap().size.width;
    let (cx, cy) = (side / 2.0, side / 2.0);
    scene
        .walk(knob)
        .into_iter()
        .filter_map(|id| {
            let n = scene.node(id)?;
            let t = n.text().filter(|_| n.name == "numeral")?;
            let shown = if t.text_case == TextCase::Upper { t.characters.to_uppercase() } else { t.characters.clone() };
            let w = draw::text_width(&shown, t.font_size);
            let (x, y, bw) = (n.transform.tx, n.transform.ty + t.font_size * 0.68, n.size.width);
            let x0 = match t.align {
                document_proto::TextAlign::Left => x,
                document_proto::TextAlign::Right => x + bw - w,
                _ => x + (bw - w) / 2.0,
            };
            let h = t.font_size * 0.9;
            // The nearest point of the glyph box to the shaft.
            let nx = cx.clamp(x0, x0 + w) - cx;
            let ny = cy.clamp(y - h / 2.0, y + h / 2.0) - cy;
            Some((t.characters.clone(), t.font_size, nx.hypot(ny)))
        })
        .collect()
}

#[test]
fn stepped_words_clear_their_ticks_at_a_readable_size() {
    // "Triode" and "Standard" used to be centred on one radius — across
    // their ticks — and set at 7·d/60 px, 5 px on a 44 px knob.
    let spec = surface_kit::knobs::spec_of("knob.chicken_head").unwrap();
    for d in [44.0, 64.0, 96.0] {
        let p = Place::new("Stage").size(d).options(&["Triode", "Pentode", "Standard"]);
        let (scene, root, _) = placed("knob.chicken_head", &p).unwrap();
        let tick_end = (spec.ring.geometry().0 + spec.ring_offset + 5.0) * d / 60.0;
        let labels = label_reach(&scene, root);
        assert_eq!(labels.len(), 3, "{labels:?}");
        for (s, size, reach) in labels {
            assert!(size >= draw::SCALE_TYPE_MIN, "{d} px: {s} at {size} px");
            assert!(reach > tick_end + 1.0, "{d} px: {s} comes to {reach:.1} px of the shaft, its tick ends at {tick_end:.1}");
        }
    }
}

#[test]
fn scale_numerals_are_readable_and_clear_on_a_small_knob() {
    for id in ["knob.bakelite", "knob.pointer", "knob.skirted", "knob.metal"] {
        let spec = surface_kit::knobs::spec_of(id).unwrap();
        let (scene, root, _) = placed(id, &Place::new("x").size(44.0)).unwrap();
        let tick_end = (spec.ring.geometry().0 + spec.ring_offset + 5.0) * 44.0 / 60.0;
        for (s, size, reach) in label_reach(&scene, root) {
            assert!(size >= draw::SCALE_TYPE_MIN, "{id}: {s} at {size} px");
            assert!(reach > tick_end + 1.0, "{id}: {s} at {reach:.1}, ticks end at {tick_end:.1}");
        }
    }
}

#[test]
fn an_empty_scale_prints_no_ring() {
    for id in knob_ids() {
        let (scene, root, _) = placed(id, &Place::new("x").scale(&[])).unwrap();
        let printed: Vec<String> = scene.walk(root).into_iter().filter_map(|n| scene.node(n)).filter(|n| n.name.starts_with("scale") || n.name == "numeral").map(|n| n.name.clone()).collect();
        // The dial prints its numerals on its own skirt, not as a ring.
        if id != "knob.dial" {
            assert!(printed.is_empty(), "{id}: {printed:?}");
        }
    }
}

#[test]
fn a_concentric_pair_can_be_all_grey_and_stepped() {
    let p = Place::new("Freq").variant("grey").options(&["Off", "35", "60", "110", "220"]).inner(Place::new("Gain"));
    let (_, _, c) = placed("knob.concentric", &p).unwrap();
    assert_eq!(c.kind, ControlKind::Knob { sweep_degrees: surface_kit::detent_sweep(5) });
}
