//! Headless exercise of the keys **mixer**: boot the profile, print the
//! engine/layer tree, ride a fader, press stacks — the Control/Perform views'
//! whole surface without a GUI.
//!
//! ```bash
//! FTS_KEYSCAPE_PACKS=/path/to/packs \
//!     cargo run -p signal-keys --example mixer_probe
//! ```

use signal_keys_proto::keys::KeysRig as KeysRigSvc;

fn dump(backend: &signal_keys::KeysRigBackend) {
    let m = KeysRigSvc::mixer(backend);
    println!("\nprofile {} (master {:+.1} dB)", m.profile, m.master_db);
    for e in &m.engines {
        println!(
            "  {:<6} {:+5.1} dB{}",
            e.name,
            e.gain_db,
            if e.muted { "  [MUTE]" } else { "" }
        );
        for l in &e.layers {
            println!(
                "    {:<9} {:+5.1} dB  {:<22} {}{}{}",
                l.name,
                l.gain_db,
                if l.patch.is_empty() { "—".into() } else { l.patch.clone() },
                if l.live { "live" } else { "empty" },
                if l.muted { " MUTE" } else { "" },
                if l.soloed { " SOLO" } else { "" },
            );
        }
    }
}

fn main() {
    tracing_subscriber::fmt().with_env_filter("warn").init();
    let backend = signal_keys::KeysRigBackend::new();

    let presets = KeysRigSvc::presets(&backend);
    println!(
        "library: {} preset(s){}",
        presets.len(),
        presets
            .first()
            .map(|p| format!(" (first: {})", p.name))
            .unwrap_or_default()
    );

    let perf = KeysRigSvc::perform(&backend);
    println!("stacks: {}", perf.stacks.len());
    for s in &perf.stacks {
        println!("  {:<11} {}", s.name, s.blurb);
    }

    dump(&backend);

    // Ride a fader + mute a lane — pure atomics on the live program.
    KeysRigSvc::set_layer_gain(&backend, "Keys A".into(), -6.0);
    KeysRigSvc::set_layer_mute(&backend, "Keys B".into(), true);
    KeysRigSvc::set_engine_gain(&backend, "Pad".into(), -3.0);
    let m = KeysRigSvc::mixer(&backend);
    let keys = m.engines.iter().find(|e| e.name == "Keys").expect("Keys engine");
    assert_eq!(keys.layers[0].gain_db, -6.0, "fader did not stick");
    assert!(keys.layers[1].muted, "mute did not stick");
    println!("\nfaders ride ✓");

    // Stack recall — scene levels across every lane.
    for (i, s) in KeysRigSvc::perform(&backend).stacks.iter().enumerate() {
        KeysRigSvc::press_stack(&backend, i as u32);
        let m = KeysRigSvc::mixer(&backend);
        let live: Vec<String> = m
            .engines
            .iter()
            .flat_map(|e| e.layers.iter())
            .filter(|l| !l.muted)
            .map(|l| format!("{} {:+.0}", l.name, l.gain_db))
            .collect();
        println!("{:<11} → {}", s.name, live.join(" · "));
    }
    let perf = KeysRigSvc::perform(&backend);
    assert_eq!(perf.active_stack, 4, "last stack should be active");
    println!("\nstacks recall ✓");
}
