//! The reverb and modulation faces lint clean against their plugin's
//! merged catalog, and every binding resolves to the face's own entries.

use design_lint::{Config, Severity, lint};
use surface_proto::{ControlKind, Source};

const REVERB: &[&str] = &[
    "01-ir",
    "02-hall",
    "03-plate",
    "04-room",
    "05-spring",
    "06-ambient",
    "07-random",
    "08-special",
];
const MODULATION: &[&str] = &[
    "01-chorus",
    "02-flanger",
    "03-vibrato",
    "04-tremolo",
    "05-wah",
];

fn faces() -> Vec<plugin_faces::Face> {
    let mut v = plugin_faces::reverb::faces();
    v.extend(plugin_faces::modulation::faces());
    v
}

#[test]
fn every_face_is_here() {
    let reverb: Vec<&str> = plugin_faces::reverb::faces()
        .iter()
        .map(|f| f.file)
        .collect();
    assert_eq!(reverb, REVERB);
    let modulation: Vec<&str> = plugin_faces::modulation::faces()
        .iter()
        .map(|f| f.file)
        .collect();
    assert_eq!(modulation, MODULATION);
    for f in plugin_faces::reverb::faces() {
        assert_eq!(f.plugin, "reverb", "{}", f.file);
    }
    for f in plugin_faces::modulation::faces() {
        assert_eq!(f.plugin, "modulation", "{}", f.file);
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
fn params_are_namespaced_per_face() {
    for f in faces() {
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
        for s in &f.streams {
            assert!(
                s.id.0.starts_with(&format!("{ns}/")),
                "{}: stream {} is outside `{ns}/`",
                f.file,
                s.id.0
            );
        }
    }
}

#[test]
fn reverbs_declare_their_levels_and_time_units() {
    for f in plugin_faces::reverb::faces() {
        let ns = f.params[0]
            .addr
            .0
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string();
        for s in ["in", "out"] {
            assert!(
                f.streams.iter().any(|x| x.id.0 == format!("{ns}/{s}")),
                "{} has no {ns}/{s} stream",
                f.file
            );
        }
        // Every reverb bar the spring (whose dwell is how hard it's driven)
        // states its decay in seconds.
        if f.file != "05-spring" {
            assert!(
                f.params
                    .iter()
                    .any(|p| p.range.unit == surface_proto::Unit::Seconds),
                "{}: no decay in seconds",
                f.file
            );
        }
    }
}

#[test]
fn every_stompbox_has_a_rate_lamp_rate_in_hz_and_a_stomp() {
    for f in plugin_faces::modulation::faces() {
        let ns = f.params[0]
            .addr
            .0
            .split('/')
            .next()
            .unwrap_or_default()
            .to_string();
        let rate = f
            .params
            .iter()
            .find(|p| p.addr.0 == format!("{ns}/rate"))
            .unwrap_or_else(|| panic!("{}: no rate", f.file));
        assert_eq!(rate.range.unit, surface_proto::Unit::Hz, "{}", f.file);
        let mut lamp = false;
        let mut stomp = false;
        for id in f.scene.walk(f.scene.root) {
            let Some(c) = f.scene.node(id).and_then(|n| n.control.as_ref()) else {
                continue;
            };
            match (&c.kind, c.binding.as_ref().map(|b| &b.source)) {
                (ControlKind::Readout, Some(Source::Stream { id, .. }))
                    if id.0 == format!("{ns}/lfo") =>
                {
                    lamp = true
                }
                (ControlKind::Footswitch { .. }, Some(Source::Param { addr }))
                    if addr.0 == format!("{ns}/on") =>
                {
                    stomp = true
                }
                _ => {}
            }
        }
        assert!(lamp, "{}: no LFO rate lamp", f.file);
        assert!(stomp, "{}: no bypass footswitch", f.file);
    }
}

#[test]
fn vibrato_has_no_mix_because_it_is_wet_only() {
    let v = plugin_faces::modulation::faces()
        .into_iter()
        .find(|f| f.file == "03-vibrato")
        .expect("vibrato");
    assert!(!v.params.iter().any(|p| p.addr.0.ends_with("/mix")));
}
