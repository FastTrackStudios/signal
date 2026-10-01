//! Setting stacks up from the switches — add, rename, learn a pedal, delete
//! — through the backend the app talks to, and the stacks surviving a
//! restart. Its own process: it points the config dir at a temp dir so the
//! player's real profile is never touched.

use signal_keys::KeysRigBackend;
use signal_keys::proto::keys::KeysRig as _;

#[test]
fn stacks_set_up_on_the_switches_are_kept() {
    let dir = std::env::temp_dir().join(format!("keys-stack-setup-{}", std::process::id()));
    // SAFETY: the only test in this binary, before any thread reads the env.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::remove_var("FTS_KEYS_PROFILE");
    }

    let rig = KeysRigBackend::new();
    let built_in = rig.perform().stacks.len();
    assert!(built_in > 0, "the worship profile ships stacks");

    rig.add_stack("Bridge".into());
    let p = rig.perform();
    assert_eq!(p.stacks.len(), built_in + 1);
    assert_eq!(p.stacks[built_in].name, "Bridge");

    rig.rename_stack(built_in as u32, "Outro".into());
    assert_eq!(rig.perform().stacks[built_in].name, "Outro");

    // Learning shows on the switch until a pedal (or a cancel) ends it.
    let target = format!("stack:{built_in}");
    rig.midi_learn(target.clone());
    assert!(rig.perform().learn.is_learning(&target));
    rig.midi_learn_cancel();
    assert_eq!(rig.perform().learn.learning, None);

    let first = rig.perform().stacks[1].name.clone();
    rig.delete_stack(0);
    let p = rig.perform();
    assert_eq!(p.stacks.len(), built_in);
    assert_eq!(p.stacks[0].name, first, "the rest moved down");

    // A restart loads the player's stacks, not the built-in ones.
    let again = KeysRigBackend::new();
    let names: Vec<String> = again.perform().stacks.iter().map(|s| s.name.clone()).collect();
    assert_eq!(names.len(), built_in);
    assert_eq!(names.last().map(String::as_str), Some("Outro"));

    let _ = std::fs::remove_dir_all(dir);
}
