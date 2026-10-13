//! Watch the native PipeWire MIDI backend: what it links, and what arrives.
//!
//! ```sh
//! cargo run -p midicore-pipewire --example pw_midi_probe          # every device
//! cargo run -p midicore-pipewire --example pw_midi_probe -- S88   # one device
//! ```
//! No `pw-jack` — that is the point.
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::sync::Arc;

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    println!("ports:");
    for p in midicore_pipewire::input_ports() {
        println!("  {p}");
    }

    let filter = std::env::args().nth(1);
    let selector = match filter.as_deref() {
        Some(n) if !n.is_empty() => midicore_proto::PortSelector::NameContains(n.to_string()),
        _ => midicore_proto::PortSelector::All,
    };

    let count = Arc::new(AtomicU64::new(0));
    let c = count.clone();
    let input = midicore_pipewire::MidiInput::open(selector, move |ev| {
        let n = c.fetch_add(1, Ordering::Relaxed);
        if n < 40 {
            println!("{:?}", ev.event);
        }
    })?;

    println!("linked: {:#?}", input.ports());
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_secs(5));
        println!(
            "events={} linked={}",
            count.load(Ordering::Relaxed),
            input.ports().len()
        );
    }
    Ok(())
}
