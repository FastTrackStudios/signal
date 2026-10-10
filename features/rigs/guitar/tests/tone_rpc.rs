//! Presets end to end on a design-mode rig over a throwaway library: a
//! patch saved as a preset plays it and names it; another patch choosing
//! it plays it too; the sidebar lists it as the Preset module's, live on
//! both.

use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig;

static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn a_preset_is_saved_from_one_patch_and_played_on_another() {
    let _env = ENV.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = std::env::temp_dir().join(format!("tone-rpc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the tests hold `ENV` while they touch the environment.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DIR", root.join("rig"));
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
    }
    let rig = GuitarRigBackend::new();
    rig.open_blocking();
    let live_tone = |rig: &GuitarRigBackend| {
        Rig::compositions(rig).active_modules.iter().find(|m| m.module == "Preset").map(|m| m.preset.clone())
    };

    assert!(Rig::select_patch(&rig, 0).ok);
    let before: Vec<_> = Rig::chain(&rig).iter().map(|b| (b.name.clone(), b.bypassed)).collect();
    Rig::save_tone(&rig, "Ambient Delay Flute".into());
    let comp = Rig::compositions(&rig);
    let entry = comp.modules.iter().find(|m| m.module == "Preset" && m.name == "Ambient Delay Flute").expect("listed as a preset");
    assert_eq!(entry.used_by.len(), 1, "the patch it came from plays it");
    assert_eq!(live_tone(&rig).as_deref(), Some("Ambient Delay Flute"));
    let after: Vec<_> = Rig::chain(&rig).iter().map(|b| (b.name.clone(), b.bypassed)).collect();
    assert_eq!(before, after, "the patch plays as it did");
    assert!(root.join("rig").join("tones.styx").exists(), "saved to tones.styx");

    // Another patch plays it.
    assert!(Rig::select_patch(&rig, 1).ok);
    let a = Rig::choose_tone(&rig, "Ambient Delay Flute".into());
    assert!(a.ok, "{}", a.message);
    assert_eq!(live_tone(&rig).as_deref(), Some("Ambient Delay Flute"));
    let comp = Rig::compositions(&rig);
    let entry = comp.modules.iter().find(|m| m.module == "Preset" && m.name == "Ambient Delay Flute").expect("still listed");
    assert_eq!(entry.used_by.len(), 2);

    // In use: not deleted; renamed, both follow.
    Rig::delete_tone(&rig, "Ambient Delay Flute".into());
    assert!(Rig::compositions(&rig).modules.iter().any(|m| m.module == "Preset"));
    Rig::rename_tone(&rig, "Ambient Delay Flute".into(), "Flute Wash".into());
    assert_eq!(live_tone(&rig).as_deref(), Some("Flute Wash"));
    assert!(!Rig::choose_tone(&rig, "No Such Preset".into()).ok);

    // Preset mode: the preset up on the bench, edited there, saved back —
    // and the bench never in the profile's file.
    Rig::set_perform_mode(&rig, 0);
    let a = Rig::edit_tone(&rig, "Flute Wash".into());
    assert!(a.ok, "{}", a.message);
    assert_eq!(Rig::status(&rig).active_patch.as_deref(), Some("\u{25C6} Preset"), "the bench is up");
    assert_eq!(live_tone(&rig).as_deref(), Some("Flute Wash"));
    // An edit on the bench (a block preset picked there — knob edits are
    // recorded only with a running engine, which a design-mode rig has not).
    let chorus = Rig::compositions(&rig).block_presets.into_iter().find(|b| b.block_type == "chorus").expect("a shipped chorus preset");
    let a = Rig::choose_block(&rig, "Chorus".into(), chorus.name.clone());
    assert!(a.ok, "{}", a.message);
    Rig::save_tone(&rig, "Flute Wash".into());
    let tones = std::fs::read_to_string(root.join("rig").join("tones.styx")).unwrap();
    assert!(tones.contains(&chorus.name), "the bench's edit went to the preset: {tones}");
    let a = Rig::new_tone(&rig, "Flute Wash Copy".into());
    assert!(a.ok, "{}", a.message);
    assert_eq!(live_tone(&rig).as_deref(), Some("Flute Wash Copy"));
    Rig::set_perform_mode(&rig, 1);
    assert_ne!(live_tone(&rig).as_deref(), Some("Flute Wash Copy"), "the bench is put away");
    let profiles = std::fs::read_dir(root.join("rig").join("profiles")).unwrap();
    for f in profiles.flatten() {
        let text = std::fs::read_to_string(f.path()).unwrap_or_default();
        assert!(!text.contains("\u{25C6} Preset"), "the bench is never saved: {}", f.path().display());
    }
}
