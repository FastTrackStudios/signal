//! The funk-rhythm suite: every unit lints clean against the suite's
//! catalog (as built and as written out), binds only what exists, binds
//! every param it declares, sits in its form factor's size, and prints no
//! maker, artist or product name.

use document_proto::Scene;
use plugin_faces::{Face, catalogs, suite_funk_rhythm};
use surface_proto::{Catalog, Source};

const PLUGIN: &str = "suite-funk-rhythm";

fn catalog() -> Catalog {
    catalogs(&plugin_faces::all()).remove(PLUGIN).expect("suite catalog")
}

fn controls(scene: &Scene) -> Vec<(&document_proto::Node, &surface_proto::Control)> {
    scene.walk(scene.root).into_iter().filter_map(|id| scene.node(id)).filter_map(|n| n.control.as_ref().map(|c| (n, c))).collect()
}

/// What a control binds: a handle's two axes, else its binding.
fn bindings(c: &surface_proto::Control) -> Vec<Option<&surface_proto::Binding>> {
    match &c.kind {
        surface_proto::ControlKind::Handle { x, y, .. } => vec![x.as_ref(), y.as_ref()],
        _ => vec![c.binding.as_ref()],
    }
}

fn lint_errors(face: &Face, scene: &Scene, catalog: &Catalog) {
    let issues = design_lint::lint(scene, catalog, &design_lint::Config::default());
    let errors: Vec<String> = issues.iter().filter(|i| i.severity == design_lint::Severity::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{}: {} lint errors:\n{}", face.file, errors.len(), errors.join("\n"));
}

#[test]
fn thirteen_units_in_signal_order() {
    let files: Vec<&str> = suite_funk_rhythm::faces().iter().map(|f| f.file).collect();
    assert_eq!(files.len(), 13);
    let mut sorted = files.clone();
    sorted.sort_unstable();
    assert_eq!(files, sorted, "file names carry the signal order");
    assert!(suite_funk_rhythm::faces().iter().all(|f| f.plugin == PLUGIN));
}

#[test]
fn every_unit_lints_without_errors() {
    let catalog = catalog();
    for face in suite_funk_rhythm::faces() {
        lint_errors(&face, &face.scene, &catalog);
        let text = document_text::print(&face.scene);
        let parsed = document_text::parse(&text, Default::default()).unwrap_or_else(|e| panic!("{}: {e}", face.file));
        lint_errors(&face, &parsed, &catalog);
    }
}

#[test]
fn every_binding_exists_and_every_param_is_bound() {
    let catalog = catalog();
    for face in suite_funk_rhythm::faces() {
        let f = face.file;
        let mut bound = std::collections::BTreeSet::new();
        for (node, c) in controls(&face.scene) {
            let issues = c.validate(&catalog);
            assert!(issues.is_empty(), "{f} / {}: {issues:?}", node.name);
            for binding in bindings(c) {
                match binding.map(|b| &b.source) {
                    Some(Source::Param { addr }) => {
                        assert!(face.params.iter().any(|p| &p.addr == addr), "{f}: {addr} is not this unit's");
                        bound.insert(addr.0.clone());
                    }
                    other => panic!("{f}: \"{}\" has binding {other:?}", node.name),
                }
            }
        }
        for p in &face.params {
            assert!(p.featured, "{f}: {} not featured", p.addr);
            assert!(bound.contains(&p.addr.0), "{f}: {} has no control", p.addr);
        }
    }
}

#[test]
fn every_unit_is_its_form_factors_size() {
    for face in suite_funk_rhythm::faces() {
        let root = face.scene.node(face.scene.pages()[0]).and_then(|p| p.children.first().copied()).and_then(|r| face.scene.node(r)).expect("root");
        let (w, h) = (root.size.width, root.size.height);
        // The file name says `<form>-<w>x<h>`.
        let size = face.file.rsplit('-').next().expect("size");
        let (fw, fh) = size.split_once('x').expect("WxH");
        assert_eq!((w, h), (fw.parse().expect("w"), fh.parse().expect("h")), "{}", face.file);
        if face.file.contains("standard") || face.file.contains("bigbox") {
            assert!((h - 286.0).abs() < 1e-9, "{}: every pedal is 286 tall", face.file);
        }
    }
}

#[test]
fn no_maker_artist_or_product_is_printed() {
    let banned = ["cory", "wong", "neural", "archetype", "postal", "4th position", "tuber", "big rig", "amp snob", "clean machine", "d.i. funk", "funk console", "delay-y-y", "the 80s", "the wash"];
    for face in suite_funk_rhythm::faces() {
        let scene = &face.scene;
        let text: String = scene
            .walk(scene.root)
            .into_iter()
            .filter_map(|id| scene.node(id))
            .map(|n| format!("{} {}\n", n.name, n.text().map(|t| t.characters.as_str()).unwrap_or_default()))
            .collect::<String>()
            .to_lowercase();
        for word in banned {
            assert!(!text.contains(word), "{} prints {word}", face.file);
            assert!(!face.file.contains(word), "{} is named {word}", face.file);
        }
        for p in &face.params {
            assert!(!banned.iter().any(|w| p.name.to_lowercase().contains(w) || p.addr.0.contains(w)), "{}: {}", face.file, p.addr);
        }
    }
}

#[test]
fn every_hardware_knob_is_a_gpu_knob() {
    // The cab's mic knobs are the plugin's flat UI knobs, not hardware.
    for face in suite_funk_rhythm::faces().iter().filter(|f| !f.file.contains("cab")) {
        let s = &face.scene;
        for (n, c) in controls(s) {
            if !matches!(c.kind, surface_proto::ControlKind::Knob { .. }) {
                continue;
            }
            let gpu = s.walk(n.id).into_iter().filter_map(|id| s.node(id)).any(|k| k.shader.as_ref().is_some_and(|f| f.shader == "knob.3d"));
            assert!(gpu, "{}: {} isn't a knob.3d", face.file, n.name);
        }
    }
}

#[test]
fn the_eq_reads_off_at_its_stops_and_signs_its_gains() {
    let cat = catalog();
    let faces = suite_funk_rhythm::faces();
    let eq = faces.iter().find(|f| f.file.starts_with("10-eq")).expect("eq");
    let read = |addr: &str, n: f64| {
        let (_, c) = controls(&eq.scene)
            .into_iter()
            .find(|(_, c)| matches!(c.kind, surface_proto::ControlKind::Readout) && matches!(c.binding.as_ref().map(|b| &b.source), Some(Source::Param { addr: a }) if a.0 == addr))
            .unwrap_or_else(|| panic!("{addr} readout"));
        cat.params.iter().find(|p| p.addr.0 == addr).expect("param").display_as(n, c.format.as_ref())
    };
    assert_eq!(read("eq/hpf", 0.0), "OFF");
    assert_eq!(read("eq/lpf", 1.0), "OFF");
    assert_ne!(read("eq/hpf", 0.5), "OFF");
    assert_eq!(read("eq/b65", 1.0), "+12.0 dB");
    assert_eq!(read("eq/b65", 0.0), "-12.0 dB");
    assert_eq!(read("eq/b65", 0.5), "0 dB");
}
