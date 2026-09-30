//! Saved engine and layer presets through the backend the browser talks to:
//! save a lane, load it into another, save an engine and load it into
//! another, rename and delete — and an audition that is undone puts the rig
//! back. Its own process: the config dir is a temp dir, so the player's
//! library and profile are never touched.
//!
//! Needs the sample libraries (`FTS_PACK_LIBRARY`, …) to open the rig
//! headless; skipped without them.

use signal_keys::KeysRigBackend;
use signal_keys::proto::KeysMixer;
use signal_keys::proto::keys::KeysRig as _;

fn lane<'a>(m: &'a KeysMixer, name: &str) -> &'a signal_keys::proto::KeysLayerModel {
    m.engines
        .iter()
        .flat_map(|e| &e.layers)
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no lane {name}"))
}

fn sources(m: &KeysMixer, name: &str) -> Vec<String> {
    lane(m, name).modules.iter().map(|m| m.patch.clone()).collect()
}

fn engine_lanes(m: &KeysMixer, engine: &str) -> Vec<String> {
    m.engines
        .iter()
        .find(|e| e.name == engine)
        .map(|e| e.layers.iter().map(|l| l.name.clone()).collect())
        .unwrap_or_default()
}

fn saved(rig: &KeysRigBackend, name: &str) -> Option<u32> {
    rig.presets()
        .iter()
        .position(|p| p.user && p.name == name)
        .map(|i| i as u32)
}

#[test]
fn saved_presets_load_into_other_slots_and_auditions_undo() {
    if std::env::var_os("FTS_PACK_LIBRARY").is_none() {
        eprintln!("skipped: no sample libraries (FTS_PACK_LIBRARY)");
        return;
    }
    let dir = std::env::temp_dir().join(format!("keys-saved-presets-{}", std::process::id()));
    // SAFETY: the only test in this binary, before any thread reads the env.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::remove_var("FTS_KEYS_PROFILE");
    }
    let rig = KeysRigBackend::new();
    assert!(rig.debug_open_headless(48_000), "open the rig headless");

    // ── Layer preset: Keys 1's sound, into the Pad lane ─────────────────
    let before = rig.mixer();
    let keys_sound = sources(&before, "Keys 1");
    let pad_before = sources(&before, "Pad");
    assert_ne!(keys_sound, pad_before, "the two lanes start different");
    rig.save_layer_preset("Pad".into(), "Pad Sound".into());
    rig.save_layer_preset("Keys 1".into(), "My Piano".into());
    let idx = saved(&rig, "My Piano").expect("the saved layer preset is listed");
    let row = &rig.presets()[idx as usize];
    assert_eq!(row.scope, "layer");
    assert_eq!(row.tags, vec!["Keys".to_string()]);

    let pad_gain = lane(&before, "Pad").gain_db;
    rig.set_layer_patch("Pad".into(), 0, idx);
    let after = rig.mixer();
    assert_eq!(sources(&after, "Pad"), keys_sound, "Pad plays the saved sound");
    assert_eq!(lane(&after, "Pad").gain_db, pad_gain, "Pad keeps its fader");
    assert_eq!(lane(&after, "Pad").preset, "My Piano");

    // ── Audition: load, then undo ───────────────────────────────────────
    let pad_sound = saved(&rig, "Pad Sound").expect("Pad's own sound saved");
    rig.audition_begin();
    rig.set_layer_patch("Pad".into(), 0, pad_sound);
    assert_eq!(sources(&rig.mixer(), "Pad"), pad_before, "auditioning plays it");
    rig.audition_end(false);
    assert_eq!(
        sources(&rig.mixer(), "Pad"),
        keys_sound,
        "an undone audition puts back what was loaded before it"
    );

    // ── Engine preset: the Keys engine, into Pad ────────────────────────
    let keys_lanes = engine_lanes(&after, "Keys");
    let pad_lanes = engine_lanes(&after, "Pad");
    rig.save_engine_preset("Keys".into(), "Piano Engine".into());
    let e = saved(&rig, "Piano Engine").expect("the saved engine preset is listed");
    assert_eq!(rig.presets()[e as usize].scope, "engine");
    rig.load_engine_preset("Pad".into(), e);
    let m = rig.mixer();
    let now = engine_lanes(&m, "Pad");
    assert_eq!(now.len(), keys_lanes.len(), "Pad holds the Keys engine's lanes");
    assert_eq!(now[0], pad_lanes[0], "the first lane keeps Pad's lane name");
    assert_eq!(engine_lanes(&m, "Keys"), keys_lanes, "Keys itself is untouched");
    let all: Vec<String> = m.engines.iter().flat_map(|e| e.layers.iter().map(|l| l.name.clone())).collect();
    let mut unique = all.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(all.len(), unique.len(), "lane names stay unique: {all:?}");

    // ── Manage ──────────────────────────────────────────────────────────
    rig.duplicate_user_preset(idx, "My Piano 2".into());
    assert!(saved(&rig, "My Piano 2").is_some());
    assert!(saved(&rig, "My Piano").is_some(), "duplicate keeps the original");
    rig.rename_user_preset(saved(&rig, "My Piano 2").unwrap(), "Felt".into());
    assert!(saved(&rig, "Felt").is_some() && saved(&rig, "My Piano 2").is_none());
    rig.delete_user_preset(saved(&rig, "Felt").unwrap());
    assert!(saved(&rig, "Felt").is_none());

    // ── A restart lists them again ──────────────────────────────────────
    let again = KeysRigBackend::new();
    assert!(saved(&again, "My Piano").is_some() && saved(&again, "Piano Engine").is_some());

    let _ = std::fs::remove_dir_all(dir);
}
