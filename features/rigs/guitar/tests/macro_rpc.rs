//! The macro RPCs end to end on a design-mode rig over a throwaway library:
//! every call says what it did, and a save with nowhere to go offers to
//! make somewhere rather than failing silently.

use signal_guitar::GuitarRigBackend;
use signal_guitar::library::RigLibrary;
use signal_guitar::proto::rig::Rig;
use signal_guitar::proto::{MacroSave, MacroTune};

#[test]
fn every_macro_call_reports_what_it_did() {
    let root = std::env::temp_dir().join(format!("macro-rpc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: this test binary runs this one test, before any thread.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DIR", root.join("rig"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        // A design run that saves, into the throwaway library.
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
    }
    let rig = GuitarRigBackend::new();
    rig.open_blocking();

    // Unknown knobs and ops say so.
    assert!(!Rig::set_macro(&rig, "nope".into(), 0.5).ok);
    assert!(!Rig::reset_macro(&rig, "nope".into()).ok);
    assert!(!Rig::set_macro_pad(&rig, "space".into(), true).ok, "Space has no pad");

    let bar = Rig::macros(&rig);
    let space = bar.iter().find(|k| k.id == "space").expect("the rig has delays and reverbs");
    let t = space.tune.iter().find(|t| t.param == "level").cloned().expect("Space moves a wet level");
    let op = |op: &str, value: f32| MacroTune {
        knob: t.knob.clone(),
        block: t.block.clone(),
        param: t.param.clone(),
        op: op.into(),
        value,
        text: String::new(),
    };
    let save = |knob: &str, scope: &str, name: &str| MacroSave { knob: knob.into(), scope: scope.into(), name: name.into() };

    let r = Rig::save_macro_tune(&rig, save("space", "block", ""));
    assert!(!r.ok && r.message.contains("Nothing tuned"), "{r:?}");
    assert!(!Rig::tune_macro(&rig, op("sideways", 1.0)).ok);
    assert!(Rig::tune_macro(&rig, op("max", t.base + 3.0)).ok);
    assert!(Rig::macros(&rig).iter().find(|k| k.id == "space").unwrap().tuned);

    // No block preset on the block: an offer, and nothing written.
    let r = Rig::save_macro_tune(&rig, save("space", "block", ""));
    assert!(!r.ok && r.offer == "new_block_preset", "{r:?}");
    assert!(RigLibrary::load_compositions().blocks.is_empty());
    // Saved as a new one: the preset exists, with the tuning in it.
    let r = Rig::save_macro_tune(&rig, save("space", "block", "Test Wash"));
    assert!(r.ok && r.message.contains("Test Wash"), "{r:?}");
    let comp = RigLibrary::load_compositions();
    let made = comp.block_preset("Test Wash").expect("the new block preset");
    assert!(made.macros.iter().any(|d| d.knob == "space" && d.param == "level"));
    assert!(!Rig::macros(&rig).iter().find(|k| k.id == "space").unwrap().tuned, "saved, so nothing unsaved");
    // The panel's Reset clears it again.
    let r = Rig::reset_macro_scope(&rig, "space".into(), "block".into());
    assert!(r.ok && r.message.contains("Test Wash"), "{r:?}");
    let r = Rig::reset_macro_scope(&rig, "space".into(), "block".into());
    assert!(!r.ok, "nothing left to clear: {r:?}");

    // Module scope with no module snapshot: an offer, then a new snapshot.
    let delay = Rig::macros(&rig).into_iter().find(|k| k.id == "delay").unwrap();
    let fb = delay.tune.iter().find(|t| t.param == "feedback").cloned().unwrap();
    let r = Rig::tune_macro(
        &rig,
        MacroTune { knob: fb.knob.clone(), block: fb.block.clone(), param: fb.param.clone(), op: "max".into(), value: 0.7, text: String::new() },
    );
    assert!(r.ok, "{r:?}");
    let r = Rig::save_macro_tune(&rig, save("delay", "module", ""));
    assert!(!r.ok && r.offer == "new_module_snapshot", "{r:?}");
    let r = Rig::save_macro_tune(&rig, save("delay", "module", "Tuned Echo"));
    assert!(r.ok, "{r:?}");
    let comp = RigLibrary::load_compositions();
    let snap = comp
        .modules
        .iter()
        .flat_map(|m| m.snapshots.iter())
        .find(|s| s.name == "Tuned Echo")
        .expect("the new module snapshot");
    assert!(snap.macros.iter().any(|d| d.param == "feedback" && d.max == Some(0.7)));

    // Positions: a patch always keeps its own; a preset snapshot only
    // when the patch plays one.
    assert!(Rig::set_macro(&rig, "space".into(), 0.8).ok);
    let r = Rig::save_macro_positions(&rig, "patch".into());
    assert!(r.ok, "{r:?}");
    if Rig::macros(&rig)[0].snapshot.is_empty() {
        let r = Rig::save_macro_positions(&rig, "snapshot".into());
        assert!(!r.ok && r.message.contains("no preset snapshot"), "{r:?}");
    }
    assert!(Rig::reset_macro(&rig, "space".into()).ok);
    let r = Rig::discard_macro_tune(&rig, "space".into());
    assert!(r.ok);

    // A run that writes nothing says so instead of pretending.
    // SAFETY: still the only thread touching the environment.
    unsafe {
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "1");
    }
    let r = Rig::save_macro_positions(&rig, "patch".into());
    assert!(!r.ok && r.message.starts_with("Not saved"), "{r:?}");

    let _ = std::fs::remove_dir_all(&root);
}
