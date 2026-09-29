use super::*;
use signal_keys_proto::{KeysEngineModel, KeysLayerModel, KeysMeter, KeysModule, KeysStack};

fn mac(id: &str, group: &str, name: &str, value: f32, min: f32, max: f32, unit: &str) -> KeysMacro {
    KeysMacro {
        id: id.into(),
        name: name.into(),
        group: group.into(),
        value,
        min,
        max,
        unit: unit.into(),
        live: true,
        bipolar: false,
        spread: String::new(),
    }
}

fn globals(prefix: &str) -> Vec<KeysMacro> {
    vec![
        mac(&format!("{prefix}filter.cutoff"), "Filter", "Cutoff", 2000.0, 20.0, 20000.0, "Hz"),
        mac(&format!("{prefix}amp.attack"), "Amp Env", "A", 10.0, 0.0, 5000.0, "ms"),
        mac(&format!("{prefix}amb.bypass"), "Ambience", "Bypass", 0.0, 0.0, 1.0, ""),
        mac(&format!("{prefix}dly.algo"), "Delay", "Machine", 3.0, 0.0, 12.0, ""),
    ]
}

fn module(index: u32, slot: &str, patch: &str) -> KeysModule {
    KeysModule { index, slot: slot.into(), patch: patch.into(), gain_db: -3.0, enabled: true, live: !patch.is_empty(), ..KeysModule::default() }
}

fn snapshot() -> KeysSnapshot {
    let layer = KeysLayerModel {
        name: "Keys A".into(),
        engine: "Keys".into(),
        gain_db: -6.0,
        soloed: true,
        modules: vec![module(0, "A", "C7 Grand"), module(1, "B", "")],
        ..KeysLayerModel::default()
    };
    let mixer = KeysMixer {
        profile: "Worship".into(),
        engines: vec![KeysEngineModel { name: "Keys".into(), gain_db: 0.0, muted: false, layers: vec![layer], drone: None }],
        master_db: -2.0,
    };
    let mut snap = KeysSnapshot {
        mixer,
        perform: KeysPerform {
            stacks: vec![KeysStack { name: "Verse".into(), ..KeysStack::default() }, KeysStack { name: "Chorus".into(), ..KeysStack::default() }],
            perform_mode: 1,
            ..KeysPerform::default()
        },
        rig_macros: globals("r."),
        ..KeysSnapshot::default()
    };
    snap.engines.insert("Keys".into(), KeysEngineDetail { engine: "Keys".into(), macros: globals("e."), ..KeysEngineDetail::default() });
    let mut layer_globals = globals("l.");
    // The layer's modules disagree on attack: the rig serves an offset.
    layer_globals[1] = KeysMacro { value: 0.25, min: -1.0, max: 1.0, unit: String::new(), bipolar: true, ..layer_globals[1].clone() };
    snap.layer_macros.insert("Keys A".into(), layer_globals);
    snap.module_macros.insert(
        ("Keys A".into(), 0),
        vec![
            mac("source.level", "Source", "Level", 0.0, -24.0, 12.0, "dB"),
            mac("source.transpose", "Source", "Transpose", 0.0, -24.0, 24.0, "st"),
            mac("source.fine", "Source", "Fine", 0.0, -100.0, 100.0, "c"),
            mac("source.unison", "Source", "Unison", 2.0, 1.0, 8.0, "v"),
            mac("dly.time", "Delay", "Time", 350.0, 20.0, 2000.0, "ms"),
        ],
    );
    snap
}

fn addrs(m: &KeysModel) -> Vec<&str> {
    m.catalog.params.iter().map(|p| p.addr.0.as_str()).collect()
}

#[test]
fn catalog_shape_follows_the_rig_tree() {
    let m = keys_model(&snapshot());
    let a = addrs(&m);
    for want in [
        "rig/filter.cutoff",
        "master/gain_db",
        "perform/mode",
        "Keys/gain_db",
        "Keys/mute",
        "Keys/filter.cutoff",
        "Keys/Keys A/gain_db",
        "Keys/Keys A/solo",
        "Keys/Keys A/exclude_global",
        "Keys/Keys A/amp.attack",
        "Keys/Keys A/A/gain_db",
        "Keys/Keys A/A/on",
        "Keys/Keys A/A/source.level",
    ] {
        assert!(a.contains(&want), "missing {want}");
    }
    // The empty module B has no params; addresses are unique.
    assert!(!a.iter().any(|x| x.starts_with("Keys/Keys A/B/")));
    let mut uniq = a.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(uniq.len(), a.len());
    // 4 rig + master + mode + engine (2 + 4) + layer (4 + 4) + module (2 + 5)
    assert_eq!(a.len(), 4 + 2 + 6 + 8 + 7);

    assert_eq!(m.catalog.action(&EntryId::new("stack/1")).unwrap().name, "Chorus");
    assert!(m.catalog.action(&EntryId::new("start")).is_some());
    assert!(m.catalog.collection(&EntryId::new("presets")).is_some());
    let streams: Vec<&str> = m.catalog.streams.iter().map(|s| s.id.0.as_str()).collect();
    assert_eq!(streams, ["master_level", "Keys/level", "Keys/Keys A/level", "Keys/Keys A/A/level", "voices"]);
    for p in &m.catalog.params {
        assert!((0.0..=1.0).contains(&p.default), "{} default {}", p.addr, p.default);
    }
}

#[test]
fn units_scales_and_kinds() {
    assert_eq!(unit("dB"), Unit::Db);
    assert_eq!(unit("Hz"), Unit::Hz);
    assert_eq!(unit("ms"), Unit::Milliseconds);
    assert_eq!(unit("s"), Unit::Seconds);
    assert_eq!(unit("%"), Unit::Percent);
    assert_eq!(unit("st"), Unit::Semitones);
    assert_eq!(unit("c"), Unit::Cents);
    assert_eq!(unit(""), Unit::None);
    assert_eq!(unit("v"), Unit::Custom { suffix: "v".into() });

    let m = keys_model(&snapshot());
    let p = |a: &str| m.catalog.param(&ParamAddr::new(a)).unwrap();
    assert_eq!(p("rig/filter.cutoff").range.scale, Scale::Logarithmic, "Hz with min > 0");
    assert_eq!(p("rig/amp.attack").range.scale, Scale::Linear, "ms from 0 can't be log");
    assert_eq!(p("Keys/Keys A/A/dly.time").range.scale, Scale::Logarithmic);
    assert_eq!(p("rig/amb.bypass").kind, ParamKind::Toggle);
    assert_eq!(p("rig/dly.algo").range.steps, Some(13));
    assert_eq!(p("Keys/Keys A/A/source.unison").range.steps, Some(8));
    assert_eq!(p("Keys/Keys A/A/source.fine").range.unit, Unit::Cents);
    assert_eq!(p("perform/mode").display(0.5), "Profile");
    // The offset Global Control is bipolar −1..1, centred.
    let off = p("Keys/Keys A/amp.attack");
    assert!(off.bipolar && off.default == 0.5 && off.range.min == -1.0);
}

#[test]
fn featured_is_the_rig_level() {
    let m = keys_model(&snapshot());
    let featured: Vec<&str> = m.catalog.params.iter().filter(|p| p.featured).map(|p| p.addr.0.as_str()).collect();
    assert_eq!(featured, ["rig/filter.cutoff", "rig/amp.attack", "rig/amb.bypass", "rig/dly.algo", "master/gain_db"]);
}

#[test]
fn routing_round_trips_real_values() {
    let m = keys_model(&snapshot());
    let v = |a: &str| m.values[&ParamAddr::new(a)];
    let route = |a: &str, n: f64| route_param(&m, &ParamAddr::new(a), n).unwrap();

    match route("rig/filter.cutoff", v("rig/filter.cutoff")) {
        KeysCall::RigGlobal { id, value } => {
            assert_eq!(id, "r.filter.cutoff", "the rig's own id, prefix kept on the wire");
            assert!((value - 2000.0).abs() < 0.1, "{value}");
        }
        other => panic!("{other:?}"),
    }
    match route("Keys/filter.cutoff", 1.0) {
        KeysCall::EngineGlobal { engine, id, value } => assert_eq!((engine.as_str(), id.as_str(), value), ("Keys", "e.filter.cutoff", 20000.0)),
        other => panic!("{other:?}"),
    }
    match route("Keys/Keys A/amp.attack", v("Keys/Keys A/amp.attack")) {
        KeysCall::LayerGlobal { layer, id, value } => {
            assert_eq!((layer.as_str(), id.as_str()), ("Keys A", "l.amp.attack"));
            assert!((value - 0.25).abs() < 1e-6, "offset round trips: {value}");
        }
        other => panic!("{other:?}"),
    }
    match route("Keys/Keys A/A/dly.time", v("Keys/Keys A/A/dly.time")) {
        KeysCall::LayerMacro { layer, module, id, value } => {
            assert_eq!((layer.as_str(), module, id.as_str()), ("Keys A", 0, "dly.time"));
            assert!((value - 350.0).abs() < 0.01, "{value}");
        }
        other => panic!("{other:?}"),
    }
    match route("master/gain_db", v("master/gain_db")) {
        KeysCall::MasterGain(db) => assert!((db + 2.0).abs() < 1e-4),
        other => panic!("{other:?}"),
    }
    match route("Keys/Keys A/gain_db", v("Keys/Keys A/gain_db")) {
        KeysCall::LayerGain { layer, db } => assert!(layer == "Keys A" && (db + 6.0).abs() < 1e-4),
        other => panic!("{other:?}"),
    }
    assert_eq!(v("Keys/Keys A/solo"), 1.0);
    assert_eq!(route("Keys/Keys A/solo", 0.0), KeysCall::LayerSolo { layer: "Keys A".into(), soloed: false });
    assert_eq!(route("Keys/Keys A/A/on", 0.0), KeysCall::ModuleEnabled { layer: "Keys A".into(), module: 0, on: false });
    assert_eq!(route("Keys/mute", 1.0), KeysCall::EngineMute { engine: "Keys".into(), muted: true });
    assert_eq!(route("perform/mode", 1.0), KeysCall::PerformMode(2));
    assert_eq!(v("perform/mode"), 0.5);
    assert!(route_param(&m, &ParamAddr::new("Keys/Keys A/B/source.level"), 0.5).is_err());
}

#[test]
fn actions_selects_and_telemetry() {
    assert_eq!(route_action(&EntryId::new("stack/1")).unwrap(), KeysCall::Stack(1));
    assert_eq!(route_action(&EntryId::new("start")).unwrap(), KeysCall::Start);
    assert!(route_action(&EntryId::new("tuner")).is_err());
    assert_eq!(route_select(&EntryId::new("presets"), 4).unwrap(), KeysCall::Preset(4));
    assert!(route_select(&EntryId::new("songs"), 0).is_err());

    let m = keys_model(&snapshot());
    let status = KeysStatus {
        master_peak: 0.5,
        voices: 3,
        meters: vec![
            KeysMeter { kind: "layer".into(), name: "Keys A".into(), peak: 1.0 },
            KeysMeter { kind: "module".into(), name: "Keys A A".into(), peak: 0.0 },
            KeysMeter { kind: "layer".into(), name: "Unknown".into(), peak: 1.0 },
        ],
        ..KeysStatus::default()
    };
    let frames = status_frames(&status, &m.meters);
    let get = |id: &str| frames.iter().find(|f| f.id.0 == id).map(|f| f.values.clone());
    assert!((get("master_level").unwrap()[0] - -6.0206).abs() < 1e-3);
    assert!(get("Keys/Keys A/level").unwrap()[0].abs() < 1e-9);
    assert_eq!(get("Keys/Keys A/A/level").unwrap(), vec![-90.0], "silence floors at −90 dBFS");
    assert_eq!(get("voices").unwrap(), vec![3.0]);
    assert_eq!(frames.len(), 4, "meters the catalog doesn't know are dropped");
}

#[test]
fn merge_keeps_defaults_and_bumps_version_on_shape_change() {
    let snap = snapshot();
    let first = keys_model(&snap);
    let mut moved = snap;
    moved.rig_macros[0].value = 500.0;
    let same = merge_model(&first, keys_model(&moved));
    assert_eq!(same.catalog, first.catalog, "a value move is not a catalog change");
    assert_ne!(same.values, first.values);

    let mut flipped = moved;
    flipped.rig_macros[0] = KeysMacro { value: 0.0, min: -1.0, max: 1.0, unit: String::new(), bipolar: true, ..flipped.rig_macros[0].clone() };
    let next = merge_model(&same, keys_model(&flipped));
    assert_eq!(next.catalog.version, first.catalog.version + 1);
    assert!(next.catalog.param(&ParamAddr::new("rig/filter.cutoff")).unwrap().bipolar);
}
