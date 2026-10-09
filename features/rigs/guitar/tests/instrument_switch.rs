//! The rig switches instrument live: the guitar's library and the bass's
//! are each their own, the switch saves where the one left off, plays the
//! other's, and comes back to the first as it was. Design-mode rig over a
//! throwaway config root.

use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig;

fn stacks(rig: &GuitarRigBackend) -> Vec<String> {
    Rig::perf(rig).stacks.into_iter().map(|s| s.name).collect()
}

#[test]
fn the_rig_switches_between_the_guitar_and_the_bass() {
    let root = std::env::temp_dir().join(format!("instrument-switch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the only test in this binary, before the rig starts.
    unsafe {
        std::env::remove_var("SIGNAL_RIG_DIR");
        std::env::set_var("SIGNAL_INSTRUMENT", "guitar");
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
    }
    let rig = GuitarRigBackend::new();
    rig.open_blocking();
    assert_eq!(Rig::perf(&rig).instrument, "guitar");
    let guitar = stacks(&rig);
    assert!(guitar.iter().any(|s| s == "Lead"), "the guitar's Worship: {guitar:?}");

    // To the bass: its own library, its own stacks.
    Rig::set_instrument(&rig, "bass".into());
    let perf = Rig::perf(&rig);
    assert_eq!(perf.instrument, "bass");
    assert_eq!(perf.profile_name, "Worship");
    assert_eq!(stacks(&rig), ["Clean", "Crunch", "Drive", "Synth", "Fuzz"]);
    assert!(!Rig::chain(&rig).is_empty(), "the bass's chain is on screen");
    // Nothing of the guitar's carried over: every stack shows the bass's own
    // patches.
    // (A patch named like its stack shows as "Default".)
    let bass_patches = ["Amp", "DI", "Crunch", "Drive", "Moog", "Env", "Fuzz", "Default"];
    let all: Vec<String> = Rig::perf(&rig).stacks.iter().map(|s| format!("{}: {} {:?}", s.name, s.current_patch, s.patches)).collect();
    for st in Rig::perf(&rig).stacks {
        assert!(bass_patches.contains(&st.current_patch.as_str()), "{}: a guitar patch carried over: {} — {all:?}", st.name, st.current_patch);
        for p in &st.patches {
            assert!(bass_patches.contains(&p.as_str()), "{}: a guitar patch carried over: {p}", st.name);
        }
    }
    let config = root.join("xdg/signal");
    assert!(config.join("bass-rig/profiles").is_dir(), "the bass's library is its own directory");
    assert!(config.join("bass-rig/models/AGS Med- AGS. Gain 5, Bass 5, Mid 5, Treble 5.nam").is_file(), "its captures seeded");
    assert_eq!(std::fs::read_to_string(config.join("instrument")).unwrap(), "bass", "remembered for next time");

    // And back: the guitar as it was.
    Rig::set_instrument(&rig, "guitar".into());
    assert_eq!(Rig::perf(&rig).instrument, "guitar");
    assert_eq!(stacks(&rig), guitar);
    let _ = std::fs::remove_dir_all(&root);
}
