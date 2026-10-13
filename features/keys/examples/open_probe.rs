//! Headless repro of the phone's keys bring-up: construct the backend,
//! auto-start (open audio + load default preset), report status — the
//! exact path behind "audio open panicked: no reactor running".
//!
//! ```bash
//! FTS_KEYSCAPE_PACKS=/path/to/packs \
//!     cargo run -p signal-keys --example open_probe
//! ```

use signal_keys_proto::keys::KeysRig as KeysRigSvc;

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let backend = signal_keys::KeysRigBackend::new();
    println!(
        "presets: {:?}",
        KeysRigSvc::presets(&backend)
            .iter()
            .map(|p| &p.name)
            .collect::<Vec<_>>()
    );
    KeysRigSvc::start(&backend);
    for _ in 0..16 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let s = KeysRigSvc::status(&backend);
        println!(
            "running={} loaded={:?} err={:?}",
            s.running, s.loaded_preset, s.last_error
        );
        if s.running
            || s.last_error
                .as_deref()
                .is_some_and(|e| !e.starts_with("opening"))
        {
            break;
        }
    }
    let s = KeysRigSvc::status(&backend);
    if s.running {
        println!("OPEN OK");
    } else {
        println!("OPEN FAILED: {:?}", s.last_error);
        std::process::exit(1);
    }
}
