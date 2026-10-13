//! The pick RPCs end to end on a design-mode rig over a throwaway library:
//! a pick that takes says so, and one the rig refuses says why — the
//! browser shows Loaded or Didn't load from exactly this answer.

use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig;

/// The tests set the process environment the library reads: one at a time.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn a_pick_says_whether_it_took_and_why_not() {
    let _env = ENV
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = std::env::temp_dir().join(format!("pick-rpc-{}", std::process::id()));
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

    // Refusals come back with their reason.
    let a = Rig::choose_module(&rig, "Delay".into(), "No Such Delay".into(), String::new());
    assert!(!a.ok);
    assert!(a.message.contains("No Such Delay"), "{}", a.message);
    let a = Rig::choose_block(&rig, "DLY 1".into(), "No Such Preset".into());
    assert!(!a.ok);
    assert!(a.message.contains("No Such Preset"), "{}", a.message);
    assert!(!Rig::select_profile(&rig, "No Such Profile".into()).ok);
    assert!(!Rig::set_drive_pedal(&rig, "Drive 1".into(), "No Such Pedal".into()).ok);
    assert!(!Rig::set_block_param(&rig, "gone".into(), "mix".into(), 0.5).ok);
    assert!(!Rig::set_block_option(&rig, "gone".into(), 0).ok);
    assert!(!Rig::select_patch(&rig, 9_999).ok);

    // A pick from the library takes — and a refusal before it does not
    // linger into its answer.
    let comp = Rig::compositions(&rig);
    let delay = comp
        .modules
        .iter()
        .find(|p| p.module.eq_ignore_ascii_case("Delay"))
        .expect("the shipped library has Delay presets");
    let a = Rig::choose_module(&rig, "Delay".into(), delay.name.clone(), String::new());
    assert!(a.ok, "{}", a.message);
    assert!(Rig::select_patch(&rig, 0).ok);
}
