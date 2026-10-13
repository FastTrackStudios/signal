//! The `suite-poly-clean` pages lint clean against the suite's merged
//! catalog, every control is bound to the page's own entries, and every
//! page is its unit's form factor.

use frame::design::lint::{Config, Severity, lint};
use frame::surface::proto::{ControlKind, Source};

const PAGES: &[(&str, f64, f64)] = &[
    ("01-pre-gold-boost", 156.0, 286.0),
    ("02-pre-bronze-compressor", 156.0, 286.0),
    ("03-pre-rose-overdrive", 156.0, 286.0),
    ("04-amp-flame-lead-head", 1200.0, 500.0),
    ("05-amp-cherub-rhythm-head", 1200.0, 500.0),
    ("06-amp-bloom-acoustic-head", 1200.0, 500.0),
    ("07-cab-two-twelve", 1500.0, 860.0),
    ("08-eq-nine-band-tabletop", 1100.0, 316.0),
    ("09-post-mint-chorus", 156.0, 286.0),
    ("10-post-onyx-delay", 404.0, 286.0),
    ("11-post-lilac-reverb", 156.0, 286.0),
    ("12-post-voices-harmonizer", 480.0, 286.0),
];

/// Names that must never appear on a page or in an address.
const BANNED: &[&str] = &[
    "neural",
    "archetype",
    "henson",
    "polyphia",
    "fortin",
    "mesa",
    "marshall",
    "shure",
    "sennheiser",
    "royer",
];

#[test]
fn every_page_is_here_at_its_form() {
    let faces = plugin_faces::suite_poly_clean::faces();
    let files: Vec<&str> = faces.iter().map(|f| f.file).collect();
    assert_eq!(files, PAGES.iter().map(|p| p.0).collect::<Vec<_>>());
    for (f, (_, w, h)) in faces.iter().zip(PAGES) {
        assert_eq!(f.plugin, "suite-poly-clean");
        let page = f.scene.pages()[0];
        let root = f
            .scene
            .node(page)
            .and_then(|p| p.children.first().copied())
            .and_then(|r| f.scene.node(r))
            .expect("root");
        assert!(
            (root.size.width - w).abs() < 0.5 && (root.size.height - h).abs() < 0.5,
            "{}: {}×{}",
            f.file,
            root.size.width,
            root.size.height
        );
    }
}

#[test]
fn every_page_lints_without_errors() {
    let all = plugin_faces::all();
    let catalogs = plugin_faces::catalogs(&all);
    for f in plugin_faces::suite_poly_clean::faces() {
        let issues = lint(&f.scene, &catalogs[f.plugin], &Config::default());
        let errors: Vec<String> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(ToString::to_string)
            .collect();
        assert!(
            errors.is_empty(),
            "{}: {} lint errors:\n{}",
            f.file,
            errors.len(),
            errors.join("\n")
        );
    }
}

#[test]
fn every_control_is_bound_to_the_pages_own_entries() {
    for f in plugin_faces::suite_poly_clean::faces() {
        let mut bound = 0;
        for id in f.scene.walk(f.scene.root) {
            let Some(c) = f.scene.node(id).and_then(|n| n.control.as_ref()) else {
                continue;
            };
            let binding = match &c.kind {
                ControlKind::Handle { x, .. } => x.as_ref(),
                _ => c.binding.as_ref(),
            };
            let Some(b) = binding else {
                panic!("{}: control {id} is unbound", f.file)
            };
            bound += 1;
            match &b.source {
                Source::Param { addr } => assert!(
                    f.params.iter().any(|p| &p.addr == addr),
                    "{}: unknown param {}",
                    f.file,
                    addr.0
                ),
                Source::Stream { id, .. } => assert!(
                    f.streams.iter().any(|s| &s.id == id),
                    "{}: unknown stream {}",
                    f.file,
                    id.0
                ),
                other => panic!("{}: unexpected binding {other:?}", f.file),
            }
        }
        assert!(bound > 0, "{}: nothing bound", f.file);
        assert!(
            f.params.iter().any(|p| p.featured),
            "{}: no featured params",
            f.file
        );
        // Every param the page declares is on it.
        for p in &f.params {
            let used = f.scene.walk(f.scene.root).into_iter().any(|id| {
                f.scene.node(id).and_then(|n| n.control.as_ref()).is_some_and(|c| {
                    let b = match &c.kind {
                        ControlKind::Handle { x, .. } => x.as_ref(),
                        _ => c.binding.as_ref(),
                    };
                    matches!(b.map(|b| &b.source), Some(Source::Param { addr }) if addr == &p.addr)
                })
            });
            assert!(used, "{}: {} has no control", f.file, p.addr.0);
        }
    }
}

#[test]
fn no_brand_names_anywhere() {
    for f in plugin_faces::suite_poly_clean::faces() {
        let text = frame::document::text::print(&f.scene).to_lowercase();
        for word in BANNED {
            assert!(!text.contains(word), "{}: prints `{word}`", f.file);
            assert!(
                !f.file.contains(word.trim()),
                "{}: file name has `{word}`",
                f.file
            );
            for p in &f.params {
                assert!(
                    !p.addr.0.to_lowercase().contains(word.trim()),
                    "{}: {}",
                    f.file,
                    p.addr.0
                );
                assert!(
                    !p.name.to_lowercase().contains(word.trim()),
                    "{}: {}",
                    f.file,
                    p.name
                );
            }
        }
    }
}
