//! The materials family and the rendered parts: every material places as
//! a swatch within a node and byte budget, its texture assets are in the
//! document, and the parts give callers the frames they fill.

use std::collections::BTreeMap;

use document_proto::{NodeId, PaintKind, Scene};
use surface_kit::materials::cabinet::{CabinetSpec, Kind, cabinet};
use surface_kit::{Family, Place, components};
use surface_widgets::{Builder, Theme};

fn build(id: &str, p: &Place) -> (Scene, NodeId) {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let root = {
        let mut b = Builder {
            scene: &mut scene,
            controls: &mut controls,
            theme: &theme,
        };
        surface_kit::place(&mut b, page, id, p).unwrap_or_else(|e| panic!("{id}: {e}"))
    };
    (scene, root)
}

/// Nodes under `root`, image-filled nodes, and the bytes of assets used.
fn cost(scene: &Scene, root: NodeId) -> (usize, usize, usize) {
    let nodes = scene.walk(root);
    let mut images = 0;
    let mut assets = std::collections::BTreeSet::new();
    for id in &nodes {
        for p in &scene.node(*id).unwrap().fills {
            if let PaintKind::Image { asset, .. } = &p.kind {
                images += 1;
                assert!(
                    scene.asset(asset).is_some(),
                    "an image fill's asset is in the document"
                );
                assets.insert(asset.clone());
            }
        }
    }
    let bytes = assets
        .iter()
        .map(|a| scene.asset(a).unwrap().bytes.len())
        .sum();
    (nodes.len(), images, bytes)
}

#[test]
fn every_material_places_in_every_finish_within_budget() {
    let mats: Vec<_> = components()
        .into_iter()
        .filter(|c| c.family == Family::Material)
        .collect();
    assert!(mats.len() >= 16, "{} materials", mats.len());
    for def in mats {
        assert!(def.id.starts_with("material."), "{}", def.id);
        let variants: Vec<Option<&str>> = if def.variants.is_empty() {
            vec![None]
        } else {
            def.variants.iter().map(|v| Some(*v)).collect()
        };
        for v in variants {
            let mut p = Place::new("Swatch").options(&["w=480", "h=320"]);
            if let Some(v) = v {
                p = p.variant(v);
            }
            let (scene, root) = build(def.id, &p);
            let (nodes, images, bytes) = cost(&scene, root);
            println!(
                "{:<26} {:<16} nodes {nodes:>3}  image rects {images:>2}  asset bytes {bytes:>6}",
                def.id,
                v.unwrap_or("-")
            );
            assert!(nodes <= 60, "{} {v:?}: {nodes} nodes", def.id);
            assert!(
                images <= surface_kit::materials::texture::MAX_TILES,
                "{} {v:?}: {images} tiles",
                def.id
            );
            assert!(
                bytes <= 160_000,
                "{} {v:?}: {bytes} bytes of texture",
                def.id
            );
            let size = scene.node(root).unwrap().size;
            assert_eq!((size.width, size.height), (480.0, 320.0));
        }
    }
}

#[test]
fn a_texture_is_stored_once_however_many_surfaces_use_it() {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder {
        scene: &mut scene,
        controls: &mut controls,
        theme: &theme,
    };
    for _ in 0..3 {
        surface_kit::place(
            &mut b,
            page,
            "material.tolex",
            &Place::new("T").variant("bronco"),
        )
        .unwrap();
    }
    assert_eq!(b.scene.assets.len(), 1);
}

#[test]
fn parts_report_their_cost() {
    for (id, p) in [
        ("knob.render", Place::new("Drive").variant("fluted")),
        (
            "knob.render",
            Place::new("Drive")
                .variant("knurled")
                .options(&["material=metal"]),
        ),
        ("knob.render", Place::new("Drive").variant("pointer")),
        ("panel.pedal_3d", Place::new("Pedal").options(&["screws"])),
        ("panel.amp_head", Place::new("Amp")),
        (
            "panel.amp_combo",
            Place::new("Amp").options(&["grille=diamond"]),
        ),
    ] {
        let (scene, root) = build(id, &p);
        let (nodes, images, bytes) = cost(&scene, root);
        println!(
            "{id:<18} {:<10} nodes {nodes:>3}  image rects {images:>2}  asset bytes {bytes:>6}",
            p.variant.as_deref().unwrap_or("-")
        );
        assert!(nodes < 200, "{id}: {nodes}");
    }
}

#[test]
fn a_cabinet_gives_its_faceplate_and_grille_to_fill() {
    let mut scene = Scene::new("Kit");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder {
        scene: &mut scene,
        controls: &mut controls,
        theme: &theme,
    };
    let cab = cabinet(
        &mut b,
        page,
        "Head",
        &CabinetSpec::new(Kind::Head, 900.0, 400.0),
    );
    for (id, name) in [(cab.faceplate, "faceplate"), (cab.grille, "grille")] {
        let n = b.scene.node(id).unwrap();
        assert_eq!(n.name, name);
        assert_eq!(n.parent, Some(cab.root));
    }
    let face = b.scene.node(cab.faceplate).unwrap().size;
    let knob =
        surface_kit::place(&mut b, cab.faceplate, "knob.render", &Place::new("Gain")).unwrap();
    assert_eq!(b.scene.node(knob).unwrap().parent, Some(cab.faceplate));
    assert!(face.width > 700.0 && face.height > 80.0, "{face:?}");
}

#[test]
fn a_3d_knob_is_a_rendered_knob_whose_standing_parts_carry_the_shader() {
    use surface_kit::materials::knob3d::{PRESETS, preset};
    use surface_proto::SlotRole;
    for name in PRESETS {
        let mut scene = Scene::new("Kit");
        let page = scene.pages()[0];
        let mut controls = BTreeMap::new();
        let theme = Theme::stage_dark();
        let root = {
            let mut b = Builder {
                scene: &mut scene,
                controls: &mut controls,
                theme: &theme,
            };
            surface_kit::place(
                &mut b,
                page,
                "knob.3d",
                &Place::new("Drive").variant(name).size(64.0).legend("Drive"),
            )
            .unwrap()
        };
        let shaded: Vec<_> = scene
            .walk(root)
            .into_iter()
            .filter(|id| scene.node(*id).unwrap().shader.is_some())
            .collect();
        assert_eq!(shaded.len(), 1, "{name}: one shader node");
        let cap = scene.node(shaded[0]).unwrap();
        let fx = cap.shader.as_ref().unwrap();
        assert_eq!(
            (cap.name.as_str(), fx.shader.as_str()),
            ("cap", "knob.3d"),
            "{name}"
        );
        assert_eq!(fx.params["angle"], 300.0, "{name}: the knob's travel");
        assert_eq!(
            fx.params["p0"],
            f64::from(preset(name).profile as u8),
            "{name}"
        );
        assert!(
            fx.inputs
                .iter()
                .all(|i| matches!(i.source, document_proto::ShaderInputSource::Const { .. })),
            "{name}: its look needs no host"
        );
        // The turning body (the fallback's Needle) is under the shader node;
        // the printed ring is not.
        let needle = NodeId(controls[&root].slots_of(SlotRole::Needle).next().unwrap());
        assert!(
            scene.walk(shaded[0]).contains(&needle),
            "{name}: the Needle draws under the shader"
        );
        let ring = scene
            .walk(root)
            .into_iter()
            .find(|id| scene.node(*id).unwrap().name == "scale dots")
            .unwrap();
        assert!(
            !scene.walk(shaded[0]).contains(&ring),
            "{name}: the printed ring stays outside the shader"
        );
        // The knob sits in the middle of the shader's box, `radius` of it.
        let side = cap.size.width;
        assert!(
            (fx.params["radius"] - 32.0 / (side / 2.0)).abs() < 1e-9,
            "{name}"
        );
    }
}

#[test]
fn a_3d_knob_without_the_stage_is_the_rendered_knob() {
    // The same nodes as `knob.render` in the matching look, plus the `cap`.
    let (a, ra) = build("knob.3d", &Place::new("Drive").variant("fluted"));
    let (b, rb) = build("knob.render", &Place::new("Drive").variant("fluted"));
    let names = |s: &Scene, r| {
        s.walk(r)
            .into_iter()
            .map(|id| s.node(id).unwrap().name.clone())
            .filter(|n| n != "cap")
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&a, ra), names(&b, rb));
}

#[test]
fn a_3d_knobs_options_reach_its_shader() {
    let p = Place::new("Drive")
        .variant("gold_knurl")
        .options(&["color=#ff0000", "rough=0.7", "ribs=30", "tilt=10"])
        .sweep(270.0);
    let (scene, root) = build("knob.3d", &p);
    let fx = scene
        .walk(root)
        .into_iter()
        .find_map(|id| scene.node(id).unwrap().shader.clone())
        .unwrap();
    assert_eq!((fx.params["tint_r"], fx.params["tint_g"]), (1.0, 0.0));
    assert_eq!(
        (
            fx.params["softness"],
            fx.params["p1"],
            fx.params["elevation"],
            fx.params["angle"]
        ),
        (0.7, 30.0, 10.0, 270.0)
    );
}

#[test]
fn a_3d_knobs_shape_options_reshape_the_preset() {
    use surface_kit::materials::knob::{Cap, Skirt};
    use surface_kit::materials::knob3d::{Look3d, Pointer, Profile, SkirtKind};
    let p = Place::new("Drive").variant("scalloped").options(&[
        "profile=stepped",
        "skirt=knurled",
        "index=dot",
        "cap_r=0.7",
        "step_r=0.4",
        "fallback=domed",
    ]);
    let l = Look3d::from_place(&p);
    assert_eq!(
        (l.profile, l.skirt, l.pointer),
        (Profile::Stepped, SkirtKind::Knurled, Pointer::Dot)
    );
    assert_eq!((l.cap_r, l.step.0), (0.7, 0.4));
    assert_eq!(
        (l.fallback.cap, l.fallback.skirt),
        (Cap::Domed, Skirt::Ridged)
    );
}

#[test]
fn a_pedal_without_a_front_face_is_all_lid() {
    let names = |p: &Place| {
        let (s, r) = build("panel.pedal_3d", p);
        s.walk(r)
            .into_iter()
            .map(|id| s.node(id).unwrap().name.clone())
            .collect::<Vec<_>>()
    };
    let with = names(&Place::new("Box").variant("standard"));
    let without = names(&Place::new("Box").variant("standard").options(&["front=0"]));
    assert!(with.iter().any(|n| n == "front face") && with.iter().any(|n| n == "base plate"));
    assert!(
        !without
            .iter()
            .any(|n| n == "front face" || n == "base plate")
    );
    let lid_w = |p: &Place| {
        let (s, r) = build("panel.pedal_3d", p);
        s.walk(r)
            .into_iter()
            .map(|id| s.node(id).unwrap())
            .find(|n| n.name == "lid")
            .unwrap()
            .size
            .width
    };
    let std_w = lid_w(&Place::new("Box").variant("standard"));
    let narrow = lid_w(
        &Place::new("Box")
            .variant("standard")
            .options(&["jack_inset=18"]),
    );
    assert!(
        (narrow - (156.0 - 36.0)).abs() < 1e-9 && narrow < std_w,
        "{narrow} vs {std_w}"
    );
}

#[test]
fn a_rendered_knob_keeps_its_light_out_of_the_turning_body() {
    use surface_proto::SlotRole;
    for cap in [
        "flat",
        "domed",
        "fluted",
        "knurled",
        "chicken_head",
        "pointer",
    ] {
        for m in ["plastic", "metal", "chrome", "rubber"] {
            let mut scene = Scene::new("Kit");
            let page = scene.pages()[0];
            let mut controls = BTreeMap::new();
            let theme = Theme::stage_dark();
            let root = {
                let mut b = Builder {
                    scene: &mut scene,
                    controls: &mut controls,
                    theme: &theme,
                };
                let mopt = format!("material={m}");
                surface_kit::place(
                    &mut b,
                    page,
                    "knob.render",
                    &Place::new("x").variant(cap).options(&[mopt.as_str()]),
                )
                .unwrap()
            };
            let needle = NodeId(controls[&root].slots_of(SlotRole::Needle).next().unwrap());
            for id in scene.walk(needle) {
                let n = scene.node(id).unwrap();
                assert!(
                    n.effects
                        .iter()
                        .all(|e| e.offset_x == 0.0 && e.offset_y == 0.0),
                    "{cap}/{m}: {} has an offset shadow and turns",
                    n.name
                );
                for p in &n.fills {
                    let ok = matches!(p.kind, PaintKind::Solid { .. })
                        || matches!(
                            &p.kind,
                            PaintKind::RadialGradient {
                                transform: None,
                                ..
                            }
                        );
                    assert!(
                        ok,
                        "{cap}/{m}: {} turns with a lit (off-centre) paint",
                        n.name
                    );
                }
            }
        }
    }
}
