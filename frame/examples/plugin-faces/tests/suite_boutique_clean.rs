//! The boutique-clean suite: every unit lints clean against the suite's
//! catalog (as built and as written out), binds only what exists, binds
//! every param it declares, sits in its form factor's size, and prints no
//! maker, artist or product name.

use document_proto::Scene;
use plugin_faces::{Face, catalogs, suite_boutique_clean};
use surface_proto::{Catalog, Source};

const PLUGIN: &str = "suite-boutique-clean";

fn catalog() -> Catalog {
    catalogs(&plugin_faces::all()).remove(PLUGIN).expect("suite catalog")
}

fn controls(scene: &Scene) -> Vec<(&document_proto::Node, &surface_proto::Control)> {
    scene.walk(scene.root).into_iter().filter_map(|id| scene.node(id)).filter_map(|n| n.control.as_ref().map(|c| (n, c))).collect()
}

fn lint_errors(face: &Face, scene: &Scene, catalog: &Catalog) {
    let issues = design_lint::lint(scene, catalog, &design_lint::Config::default());
    let errors: Vec<String> = issues.iter().filter(|i| i.severity == design_lint::Severity::Error).map(ToString::to_string).collect();
    assert!(errors.is_empty(), "{}: {} lint errors:\n{}", face.file, errors.len(), errors.join("\n"));
}

#[test]
fn eleven_units_in_signal_order() {
    let files: Vec<&str> = suite_boutique_clean::faces().iter().map(|f| f.file).collect();
    assert_eq!(files.len(), 11);
    let mut sorted = files.clone();
    sorted.sort_unstable();
    assert_eq!(files, sorted, "file names carry the signal order");
    assert!(suite_boutique_clean::faces().iter().all(|f| f.plugin == PLUGIN));
}

#[test]
fn every_unit_lints_without_errors() {
    let catalog = catalog();
    for face in suite_boutique_clean::faces() {
        lint_errors(&face, &face.scene, &catalog);
        let text = document_text::print(&face.scene);
        let parsed = document_text::parse(&text, Default::default()).unwrap_or_else(|e| panic!("{}: {e}", face.file));
        lint_errors(&face, &parsed, &catalog);
    }
}

#[test]
fn every_binding_exists_and_every_param_is_bound() {
    let catalog = catalog();
    for face in suite_boutique_clean::faces() {
        let f = face.file;
        let mut bound = std::collections::BTreeSet::new();
        for (node, c) in controls(&face.scene) {
            let issues = c.validate(&catalog);
            assert!(issues.is_empty(), "{f} / {}: {issues:?}", node.name);
            // A handle binds its two axes; everything else its binding.
            let bindings = match &c.kind {
                surface_proto::ControlKind::Handle { x, y, .. } => vec![x.as_ref(), y.as_ref()],
                _ => vec![c.binding.as_ref()],
            };
            for binding in bindings {
                match binding.map(|b| &b.source) {
                    Some(Source::Param { addr }) => {
                        assert!(face.params.iter().any(|p| &p.addr == addr), "{f}: {addr} is not this unit's");
                        bound.insert(addr.0.clone());
                    }
                    Some(Source::Stream { id, .. }) => assert!(face.streams.iter().any(|s| &s.id == id), "{f}: {} is not this unit's", id.0),
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
    for face in suite_boutique_clean::faces() {
        let root = face.scene.node(face.scene.pages()[0]).and_then(|p| p.children.first().copied()).and_then(|r| face.scene.node(r)).expect("root");
        let (w, h) = (root.size.width, root.size.height);
        let size = face.file.rsplit('-').next().expect("size");
        let (fw, fh) = size.split_once('x').expect("WxH");
        assert_eq!((w, h), (fw.parse().expect("w"), fh.parse().expect("h")), "{}", face.file);
        if face.file.contains("standard") {
            assert!((w - 156.0).abs() < 1e-9 && (h - 286.0).abs() < 1e-9, "{}: a standard pedal is 156 × 286", face.file);
        }
    }
}

#[test]
fn no_maker_artist_or_product_is_printed() {
    let banned = ["asato", "mateus", "neural", "archetype", "dc30", "dc-30", "lonestar", "shiva", "matchless", "mesa", "bogner", "あさと"];
    for face in suite_boutique_clean::faces() {
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
