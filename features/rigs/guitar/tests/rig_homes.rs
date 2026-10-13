//! Two rigs in one process, each in a home of its own: a guitar rig and a
//! bass rig side by side, as a DAW hosting both runs them. Each seeds its
//! own library from its own instrument's shipped defaults, a save through
//! one is never seen by the other, and the process's own instrument and
//! library are left as they were. Design-mode rigs over throwaway folders.

use std::path::Path;

use guitar::instrument::{self, Instrument};
use guitar::library::rig_dir;
use guitar::proto::rig::Rig;
use guitar::{GuitarRigBackend, RigHome};

fn profile_names(rig: &GuitarRigBackend) -> Vec<String> {
    Rig::library(rig)
        .profiles
        .into_iter()
        .map(|p| p.name)
        .collect()
}

fn stacks(rig: &GuitarRigBackend) -> Vec<String> {
    Rig::perf(rig).stacks.into_iter().map(|s| s.name).collect()
}

fn has(dir: &Path, file: &str) -> bool {
    dir.join(file).is_file()
}

/// The bass's shipped capture: in a bass library, never in a guitar's.
const BASS_CAPTURE: &str = "models/AGS Med- AGS. Gain 5, Bass 5, Mid 5, Treble 5.nam";

#[test]
fn a_guitar_rig_and_a_bass_rig_run_side_by_side_each_in_its_own_home() {
    let root = std::env::temp_dir().join(format!("rig-homes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // SAFETY: the only test in this binary, before any rig starts.
    unsafe {
        std::env::remove_var("SIGNAL_RIG_DIR");
        std::env::remove_var("SIGNAL_INSTRUMENT");
        std::env::set_var("XDG_CONFIG_HOME", root.join("xdg"));
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
        std::env::set_var("SIGNAL_RIG_EPHEMERAL", "0");
        std::env::set_var("SIGNAL_RELOAD_LOG", root.join("config-reload.log"));
        std::env::set_var("SIGNAL_DROP_LOG", root.join("dropouts.log"));
    }
    let process_instrument = instrument::current();
    let process_dir = rig_dir();
    assert_eq!(
        process_instrument,
        Instrument::Guitar,
        "the process starts as the guitar"
    );

    // A switch there and back comes home, without touching the process.
    let a = root.join("guitar");
    let b = root.join("bass");
    let there = RigHome::new(Instrument::Guitar, &a).with_instrument(Instrument::Bass);
    assert_eq!(there.instrument(), Instrument::Bass);
    assert_ne!(
        there.dir(),
        a.as_path(),
        "the bass never plays from the guitar's library"
    );
    assert_eq!(there.with_instrument(Instrument::Guitar).dir(), a.as_path());
    assert!(!RigHome::new(Instrument::Bass, &b).is_process_default());

    let guitar = GuitarRigBackend::with_home(RigHome::new(Instrument::Guitar, &a));
    let bass = GuitarRigBackend::with_home(RigHome::new(Instrument::Bass, &b));
    guitar.open_blocking();
    bass.open_blocking();

    // The process's instrument and library are as they were: building the
    // bass rig did not make the process the bass, nor move its library.
    assert_eq!(instrument::current(), process_instrument);
    assert_eq!(rig_dir(), process_dir);
    assert!(
        !process_dir.join("profiles").exists(),
        "the process's own library was never seeded"
    );

    // Each is its own instrument.
    assert_eq!(Rig::perf(&guitar).instrument, "guitar");
    assert_eq!(Rig::perf(&bass).instrument, "bass");
    assert_eq!(guitar.home().dir(), a.as_path());
    assert_eq!(bass.home().dir(), b.as_path());

    // Each seeded from its own shipped defaults: the guitar's profiles and
    // captures in A, the bass's in B.
    assert!(
        has(&a, "profiles/blues.styx"),
        "the guitar's Blues seeded in its home"
    );
    assert!(!has(&b, "profiles/blues.styx"), "the bass ships no Blues");
    assert!(
        has(&b, "profiles/metal.styx"),
        "the bass's Metal seeded in its home"
    );
    assert!(has(&b, "profiles/worship.styx"));
    let metal =
        |dir: &Path| std::fs::read_to_string(dir.join("profiles/metal.styx")).unwrap_or_default();
    assert_ne!(metal(&a), metal(&b), "each Metal is its own instrument's");
    assert!(
        has(&b, BASS_CAPTURE),
        "the bass's captures seeded in its home"
    );
    assert!(!has(&a, BASS_CAPTURE), "and not in the guitar's");
    let guitar_profiles = profile_names(&guitar);
    let bass_profiles = profile_names(&bass);
    assert!(
        guitar_profiles.iter().any(|p| p == "Blues"),
        "{guitar_profiles:?}"
    );
    assert!(
        !bass_profiles.iter().any(|p| p == "Blues"),
        "{bass_profiles:?}"
    );
    assert!(
        bass_profiles.iter().any(|p| p == "Metal"),
        "{bass_profiles:?}"
    );
    assert!(
        stacks(&guitar).iter().any(|s| s == "Lead"),
        "the guitar's Worship: {:?}",
        stacks(&guitar)
    );
    assert_eq!(
        stacks(&bass),
        ["Clean", "Crunch", "Drive", "Synth", "Fuzz", "Ambient"]
    );

    // A profile saved through one is in its library only.
    Rig::add_profile(&guitar, "Guitar Only".into(), String::new());
    Rig::add_profile(&bass, "Bass Only".into(), String::new());
    assert!(has(&a, "profiles/guitar-only.styx"));
    assert!(
        !has(&b, "profiles/guitar-only.styx"),
        "the guitar's save reached the bass's library"
    );
    assert!(has(&b, "profiles/bass-only.styx"));
    assert!(
        !has(&a, "profiles/bass-only.styx"),
        "the bass's save reached the guitar's library"
    );
    assert!(profile_names(&guitar).iter().any(|p| p == "Guitar Only"));
    assert!(!profile_names(&bass).iter().any(|p| p == "Guitar Only"));
    assert!(!profile_names(&guitar).iter().any(|p| p == "Bass Only"));

    // A rig with a home of its own switches itself, not the process: the
    // guitar rig plays the bass from a library of its own, and back.
    guitar.switch_instrument(Instrument::Bass);
    assert_eq!(Rig::perf(&guitar).instrument, "bass");
    assert_ne!(guitar.home().dir(), a.as_path());
    assert_ne!(
        guitar.home().dir(),
        b.as_path(),
        "never the other rig's library"
    );
    assert_eq!(
        instrument::current(),
        process_instrument,
        "the process is still the guitar"
    );
    assert!(
        !root.join("xdg/signal/instrument").exists(),
        "a rig's own switch is not remembered as the process's"
    );
    guitar.switch_instrument(Instrument::Guitar);
    assert_eq!(guitar.home().dir(), a.as_path());
    assert!(
        profile_names(&guitar).iter().any(|p| p == "Guitar Only"),
        "back in its own library"
    );
    // The bass rig never noticed.
    assert_eq!(Rig::perf(&bass).instrument, "bass");
    assert_eq!(bass.home().dir(), b.as_path());

    let _ = std::fs::remove_dir_all(&root);
}
