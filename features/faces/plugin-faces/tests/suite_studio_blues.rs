//! The studio-blues suite: every unit lints clean against the suite's
//! merged catalog, every control is bound to the unit's own entries, every
//! pedal is a pedal class's size with its footswitches in the bottom 30 %,
//! and nothing names a real product.

use design_lint::{Config, Severity, lint};
use document_proto::{NodeId, Scene};
use plugin_faces::suite_studio_blues::{self, PLUGIN};
use surface_proto::{ControlKind, Source};

#[test]
fn every_unit_lints_without_errors() {
    let all = plugin_faces::all();
    let catalogs = plugin_faces::catalogs(&all);
    let mut errors = Vec::new();
    for f in suite_studio_blues::faces() {
        assert_eq!(f.plugin, PLUGIN);
        let issues = lint(&f.scene, &catalogs[PLUGIN], &Config::default());
        errors.extend(
            issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .map(|i| format!("{}: {i}", f.file)),
        );
    }
    assert!(
        errors.is_empty(),
        "{} lint errors:\n{}",
        errors.len(),
        errors.join("\n")
    );
}

#[test]
fn every_control_is_bound_to_its_units_entries() {
    for f in suite_studio_blues::faces() {
        let ns = f.params[0]
            .addr
            .0
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string();
        for p in &f.params {
            assert!(
                p.addr.0.starts_with(&format!("{ns}/")),
                "{}: {} is outside `{ns}/`",
                f.file,
                p.addr.0
            );
        }
        let mut knobs = 0;
        for id in f.scene.walk(f.scene.root) {
            let Some(c) = f.scene.node(id).and_then(|n| n.control.as_ref()) else {
                continue;
            };
            let Some(b) = &c.binding else {
                panic!("{}: control {id} is unbound", f.file)
            };
            if matches!(c.kind, ControlKind::Knob { .. }) {
                knobs += 1;
            }
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
        assert!(knobs > 0, "{}: no knobs", f.file);
        assert!(
            f.params.iter().any(|p| p.featured),
            "{}: no featured params",
            f.file
        );
    }
}

/// Every control's top edge in page space: `(node, y)`.
fn tops(scene: &Scene, id: NodeId, y: f64, out: &mut Vec<(NodeId, f64)>) {
    let Some(n) = scene.node(id) else { return };
    let y = y + n.transform.ty;
    if n.control.is_some() {
        out.push((id, y));
    }
    for c in &n.children {
        tops(scene, *c, y, out);
    }
}

#[test]
fn pedals_are_a_class_size_and_stomp_low() {
    let widths = [92.0, 144.0, 156.0, 172.0, 222.0, 241.0, 286.0, 404.0];
    for f in suite_studio_blues::faces()
        .into_iter()
        .filter(|f| f.file.contains("-pedal-"))
    {
        let root = f
            .scene
            .node(f.scene.pages()[0])
            .and_then(|p| p.children.first().copied())
            .expect("root");
        let size = f.scene.node(root).expect("root node").size;
        assert!(
            (size.height - 286.0).abs() < 0.5,
            "{}: height {}",
            f.file,
            size.height
        );
        assert!(
            widths.iter().any(|w| (size.width - w).abs() < 0.5),
            "{}: width {} is no pedal class",
            f.file,
            size.width
        );
        let mut all = Vec::new();
        tops(&f.scene, root, 0.0, &mut all);
        let mut stomps = 0;
        for (id, y) in all {
            let c = f
                .scene
                .node(id)
                .and_then(|n| n.control.as_ref())
                .expect("control");
            let momentary = matches!(
                c.kind,
                ControlKind::Button {
                    mode: surface_proto::ButtonMode::Momentary
                }
            );
            if matches!(c.kind, ControlKind::Footswitch { .. }) || momentary {
                stomps += 1;
                assert!(
                    y >= 286.0 * 0.7 - 1.0,
                    "{}: footswitch top at y {y}",
                    f.file
                );
            }
        }
        assert!(stomps > 0, "{}: no footswitch", f.file);
    }
}

#[test]
fn nothing_names_a_real_product() {
    const BANNED: &[&str] = &[
        "neural",
        "archetype",
        "mayer",
        "gravity",
        "tealbreaker",
        "halfman",
        "antelope",
        "millipede",
        "justa",
        "headroom",
        "signature",
        "smooth operator",
        "dream delay",
        "studio verb",
    ];
    for f in suite_studio_blues::faces() {
        let text = document_text::print(&f.scene).to_lowercase();
        for b in BANNED {
            assert!(!text.contains(b), "{}: mentions `{b}`", f.file);
            assert!(!f.file.contains(b), "{}: file name mentions `{b}`", f.file);
        }
        for p in &f.params {
            assert!(
                !BANNED.iter().any(|b| p.name.to_lowercase().contains(b)),
                "{}: param {}",
                f.file,
                p.name
            );
        }
    }
}
