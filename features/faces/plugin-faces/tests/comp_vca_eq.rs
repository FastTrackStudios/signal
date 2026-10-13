//! The VCA-compressor and hardware-EQ faces lint clean against their
//! plugin's merged catalog, and every binding resolves.

use frame::design::lint::{Config, Severity, lint};
use frame::surface::proto::Source;

const FILES: &[&str] = &[
    "08-bus-comp",
    "09-vca-comp",
    "10-character-comp",
    "01-program-eq",
    "02-console-eq",
    "03-proportional-eq",
    "04-classa-eq",
];

fn faces() -> Vec<plugin_faces::Face> {
    let mut v = plugin_faces::comp_vca::faces();
    v.extend(plugin_faces::eq::faces());
    v
}

#[test]
fn every_face_is_here() {
    let files: Vec<&str> = faces().iter().map(|f| f.file).collect();
    assert_eq!(files.len(), FILES.len());
    for f in FILES {
        assert!(files.contains(f), "missing face {f}");
    }
}

#[test]
fn every_face_lints_without_errors() {
    let all = plugin_faces::all();
    let catalogs = plugin_faces::catalogs(&all);
    for f in faces() {
        let catalog = &catalogs[f.plugin];
        let issues = lint(&f.scene, catalog, &Config::default());
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
fn every_binding_resolves_to_the_faces_own_entries() {
    for f in faces() {
        let mut bound = 0;
        for id in f.scene.walk(f.scene.root) {
            let Some(c) = f.scene.node(id).and_then(|n| n.control.as_ref()) else {
                continue;
            };
            let Some(b) = &c.binding else {
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
    }
}

#[test]
fn compressors_meter_gain_reduction() {
    for f in plugin_faces::comp_vca::faces() {
        let gr = format!(
            "{}/gr",
            f.params[0].addr.0.split('/').next().unwrap_or_default()
        );
        assert!(
            f.streams.iter().any(|s| s.id.0 == gr),
            "{} has no {gr} stream",
            f.file
        );
    }
}
