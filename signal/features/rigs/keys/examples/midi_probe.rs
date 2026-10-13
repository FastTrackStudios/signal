//! Headless end-to-end integration test of the keys rig's play path: open
//! the rig (audio + MIDI attach via the single `ensure_open` path), inject
//! chords into ALSA's always-present `Midi Through` loopback, and assert the
//! rig both SAW the events (`midi_recent`) and made SOUND (`master_peak`).
//!
//! ```bash
//! just keys-test        # = pw-jack cargo run --release -p signal-keys --example midi_probe
//! ```
//!
//! Runs under pipewire-jack (the MIDI backend is jack; outside the app's
//! env, wrap with `pw-jack`). Notes are re-sent every second — the pw links
//! for a fresh attach take a moment to settle, so the first chord can miss.
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

    // Inject chords into the ALSA loopback (captured by the omni attach) and
    // watch for them coming back through the monitor + the master meter.
    let mut out = match midicore::midir::MidiOutput::open(midicore::PortSelector::NameContains(
        "Midi Through".into(),
    )) {
        Ok(o) => {
            println!("injecting via {:?}", o.opened);
            Some(o)
        }
        Err(e) => {
            println!("no Midi Through output ({e}); feed MIDI externally");
            None
        }
    };
    let mut events = 0usize;
    let mut peak = 0.0f32;
    for i in 0..40 {
        // Re-send every second: a fresh attach's pw links can take a moment,
        // so the first chord may land before the rig's port is linked. Ons on
        // even ticks, offs on odd — each chord rings for 500 ms.
        if let Some(o) = out.as_mut() {
            let (status, vel) = if i % 2 == 0 {
                (0x90u8, 100u8)
            } else {
                (0x80, 64)
            };
            for n in [60u8, 64, 67] {
                let _ = o.send(&[status, n, vel]);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
        let recent = KeysRigSvc::midi_recent(&backend);
        let s = KeysRigSvc::status(&backend);
        events = events.max(recent.len());
        peak = peak.max(s.master_peak);
        if i % 4 == 0 {
            println!(
                "t={}s midi_recent={} master_peak={:.4}",
                i / 2,
                recent.len(),
                s.master_peak
            );
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
        if events == 0 {
            "no MIDI seen by the rig (attach broken?)"
        } else {
            "MIDI seen but master meter never moved"
        }
    );
    std::process::exit(1);
}
