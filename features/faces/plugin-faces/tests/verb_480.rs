//! The 480 — a reverb machine the host puts on any algorithm — lints clean
//! in every shape and finish, binds only what the algorithms' reverb faces
//! bind (plus the program's name), and takes no algorithm's place in
//! `faces.json`.

use std::collections::BTreeSet;

use frame::design::lint::{Config, Severity, lint};
use frame::surface::proto::Source;

fn is_480(file: &str) -> bool {
    file.starts_with("96-verb-480")
}

/// Every param address a face's controls are bound to (each its own entry).
fn bound(f: &plugin_faces::Face) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for id in f.scene.walk(f.scene.root) {
        let Some(c) = f.scene.node(id).and_then(|n| n.control.as_ref()) else {
            continue;
        };
        let Some(b) = &c.binding else {
            panic!("{}: control {id} is unbound", f.file)
        };
        if let Source::Param { addr } = &b.source {
            assert!(
                f.params.iter().any(|p| &p.addr == addr),
                "{}: unknown param {}",
                f.file,
                addr.0
            );
            out.insert(addr.0.clone());
        }
    }
    out
}

#[test]
fn every_shape_and_finish_is_here() {
    let files: BTreeSet<&str> = plugin_faces::all()
        .iter()
        .map(|f| f.file)
        .filter(|f| is_480(f))
        .collect();
    let want: BTreeSet<&str> = [
        "96-verb-480",
        "96-verb-480-dark",
        "96-verb-480-phone",
        "96-verb-480-phone-dark",
        "96-verb-480-back",
        "96-verb-480-back-dark",
        "96-verb-480-back-phone",
        "96-verb-480-back-phone-dark",
        "96-verb-480-tall",
        "96-verb-480-tall-dark",
    ]
    .into_iter()
    .collect();
    assert_eq!(files, want);
}

#[test]
fn every_face_lints_without_errors() {
    let all = plugin_faces::all();
    let catalogs = plugin_faces::catalogs(&all);
    for f in all.iter().filter(|f| is_480(f.file)) {
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
fn it_binds_what_the_reverb_faces_bind_and_its_program() {
    let all = plugin_faces::all();
    let mut theirs: BTreeSet<String> = BTreeSet::new();
    // The algorithms' faces — not the routing grid's block cells, which are
    // pictures of a block, not a face for one.
    for f in all.iter().filter(|f| {
        f.plugin == "rig-faces"
            && f.file.contains("-verb-")
            && !f.file.ends_with("-block")
            && !is_480(f.file)
    }) {
        theirs.extend(bound(f));
    }
    theirs.insert("verb/program".into());
    for f in all.iter().filter(|f| is_480(f.file)) {
        let mine = bound(f);
        let extra: Vec<&String> = mine.difference(&theirs).collect();
        assert!(
            extra.is_empty(),
            "{}: binds addresses no reverb face does: {extra:?}",
            f.file
        );
    }
    // The front carries the whole main set: the Time knob, its switch and
    // readout, the main controls, bypass, nameplate, flip and the program.
    let front = all
        .iter()
        .find(|f| f.file == "96-verb-480")
        .map(bound)
        .unwrap_or_default();
    for want in [
        "time_knob",
        "sync",
        "time_text",
        "size",
        "predelay",
        "damping",
        "modulation",
        "level",
        "on",
        "preset",
        "preset_prev",
        "preset_next",
        "preset_browse",
        "algo_menu",
        "flip",
        "program",
        "algorithm",
    ] {
        assert!(
            front.contains(&format!("verb/{want}")),
            "the front doesn't bind verb/{want}"
        );
    }
}

#[test]
fn it_is_listed_by_its_own_name_after_every_algorithm() {
    let doc: serde_json::Value =
        serde_json::from_str(&plugin_faces::rig_faces::manifest()).expect("faces.json");
    let reverbs = doc["time"]["reverb"].as_array().expect("time.reverb");
    let last = reverbs.last().expect("an entry");
    assert_eq!(last["match"], serde_json::json!(["480"]));
    assert_eq!(last["face"], "96-verb-480");
    assert_eq!(
        reverbs.len(),
        plugin_faces::rig_time::kit::VERB_ALGOS.len() + 1
    );
    for (e, name) in reverbs.iter().zip(plugin_faces::rig_time::kit::VERB_ALGOS) {
        assert_eq!(e["match"], serde_json::json!([name.to_lowercase()]));
    }
}
