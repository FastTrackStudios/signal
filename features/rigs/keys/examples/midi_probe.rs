//! Headless end-to-end proof of the keys rig's play path: open the rig
//! (audio + MIDI attach via the single `ensure_open` path), then watch the
//! MIDI monitor and the master meter while notes arrive.
//!
//! Feed it MIDI from another terminal while it runs — any captured port
//! works, ALSA's always-present loopback is the scripted choice:
//!
//! ```bash
//! cargo run --release -p signal-keys --example midi_probe &
//! aplaymidi -p 'Midi Through' /tmp/probe.mid
//! ```
//!
//! Exit code 0 = MIDI events were seen AND the master meter moved (the rig
//! is audible end-to-end). 1 = the rig opened but stayed deaf or silent.
//! 2 = the rig never opened.

use signal_keys_proto::keys::KeysRig as KeysRigSvc;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let backend = signal_keys::KeysRigBackend::new();
    KeysRigSvc::start(&backend);

    // Wait for the rig to open (pack preload can take a few seconds).
    let mut running = false;
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let s = KeysRigSvc::status(&backend);
        if s.running {
            running = true;
            break;
        }
        if let Some(e) = s.last_error.as_deref() {
            if !e.starts_with("opening") {
                println!("OPEN FAILED: {e}");
                std::process::exit(2);
            }
        }
    }
    if !running {
        println!("OPEN TIMEOUT");
        std::process::exit(2);
    }
    let s = KeysRigSvc::status(&backend);
    println!(
        "OPEN OK loaded={:?} midi_port={:?}",
        s.loaded_preset, s.midi_port
    );

    // Watch for injected MIDI and meter movement.
    let mut events = 0usize;
    let mut peak = 0.0f32;
    for i in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let recent = KeysRigSvc::midi_recent(&backend);
        let s = KeysRigSvc::status(&backend);
        events = events.max(recent.len());
        peak = peak.max(s.master_peak);
        if i % 4 == 0 {
            println!("t={}s midi_recent={} master_peak={:.4}", i / 2, recent.len(), s.master_peak);
        }
        if events > 0 && peak > 1e-4 {
            break;
        }
    }
    println!("RESULT midi_events={events} max_master_peak={peak:.4}");
    if events > 0 && peak > 1e-4 {
        println!("PASS: MIDI in → audio out");
        std::process::exit(0);
    }
    println!(
        "FAIL: {}",
        if events == 0 { "no MIDI seen by the rig (attach broken?)" } else { "MIDI seen but master meter never moved" }
    );
    std::process::exit(1);
}
