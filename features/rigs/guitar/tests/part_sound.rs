//! A section's sound: a patch, a whole stack, or a preset's variation —
//! each set over the RPC, shown in the perf model, and played when the
//! section comes up. Design-mode rig over a throwaway library.

use signal_guitar::GuitarRigBackend;
use signal_guitar::proto::rig::Rig;

/// The tests set the process environment the library reads: one at a time.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn a_section_plays_a_stack_or_a_variation() {
    let _env = ENV
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = std::env::temp_dir().join(format!("part-sound-{}", std::process::id()));
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
    Rig::set_perform_mode(&rig, 2);
    Rig::add_part(&rig, "Sound Check".into());
    let at = Rig::perf(&rig)
        .parts
        .iter()
        .position(|p| p.name == "Sound Check")
        .expect("the part was added");

    // A whole stack: the part says so, and lands on the stack's first patch.
    let perf = Rig::perf(&rig);
    let stack = perf
        .stacks
        .iter()
        .find(|s| s.patches.len() > 1)
        .expect("the profile has a stack with a rotation")
        .clone();
    Rig::set_part_stack(&rig, "Sound Check".into(), stack.name.clone());
    let part = Rig::perf(&rig).parts[at].clone();
    assert_eq!(part.stack, stack.name);
    assert!(part.patch.is_empty(), "one sound at a time");
    assert!(Rig::select_part(&rig, at as u32).ok);
    let playing = Rig::patches(&rig).into_iter().find(|p| p.active).map(|p| p.name);
    assert_eq!(playing.as_deref(), Some(stack.patches[0].as_str()), "it plays the stack's first patch");

    // A preset's variation: it replaces the stack, and plays.
    let comp = Rig::compositions(&rig);
    let preset = comp
        .presets
        .iter()
        .find(|p| !p.snapshots.is_empty())
        .expect("the shipped library has presets");
    let variation = preset.snapshots[0].name.clone();
    Rig::set_part_preset(&rig, "Sound Check".into(), preset.name.clone(), variation.clone());
    let part = Rig::perf(&rig).parts[at].clone();
    assert_eq!(part.preset, format!("{} · {variation}", preset.name));
    assert!(part.stack.is_empty(), "one sound at a time");
    assert!(Rig::select_part(&rig, at as u32).ok);
    let comp = Rig::compositions(&rig);
    assert_eq!((comp.active_preset.as_str(), comp.active_snapshot.as_str()), (preset.name.as_str(), variation.as_str()));

    // A patch again clears both.
    Rig::set_part_patch(&rig, "Sound Check".into(), stack.patches[0].clone());
    let part = Rig::perf(&rig).parts[at].clone();
    assert!(part.stack.is_empty() && part.preset.is_empty());
}
