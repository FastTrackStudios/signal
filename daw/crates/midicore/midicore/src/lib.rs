//! `midicore` — the single app-facing MIDI facade.
//!
//! Depend on **this crate only**. It re-exports the wire types + byte codec +
//! `#[architect::rpc]` service traits from `midicore-proto`, and — behind the
//! `native` feature — the platform's hardware input as [`MidiInput`]:
//! PipeWire on Linux, CoreMIDI on macOS, Web MIDI in the browser. Nothing
//! outside the midicore workspace should name a backend crate directly.
//!
//! ```ignore
//! midicore = { version = "0.1", features = ["native"] }   // in Cargo.toml
//! use midicore::{MidiInput, PortSelector};
//! let input = MidiInput::open(PortSelector::All, |ev| { /* ... */ })?;
//! ```
//!
//! Backends implement [`InputBackend`]; see [`backend`] for the contract.

pub use midicore_proto::*;

mod monitor;
pub use monitor::{MidiMonitor, MIDI_MONITOR_CAP};

/// Map a stored port name to a [`PortSelector`] — the selection convention
/// every rig otherwise re-derives. A non-empty name narrows to that port
/// (`NameContains`); no name opens *every* input (`All`), the omni default a
/// live rig wants (PipeWire fans every device into one stream).
// r[impl primitives.midi.attach]
pub fn selector_for(name: Option<&str>) -> PortSelector {
    match name {
        Some(n) if !n.is_empty() => PortSelector::NameContains(n.to_string()),
        _ => PortSelector::All,
    }
}

/// The platform's native MIDI input — see [`native`].
#[cfg(all(
    feature = "native",
    any(
        target_os = "linux",
        target_os = "macos",
        target_arch = "wasm32",
        feature = "midir"
    )
))]
pub mod native;
#[cfg(all(
    feature = "native",
    any(
        target_os = "linux",
        target_os = "macos",
        target_arch = "wasm32",
        feature = "midir"
    )
))]
pub use native::{input_ports, input_sources, MidiInput, DEFAULT_INPUT_NAME};

// The MIDI attach lifecycle (drop-old-before-open, monitor-tap sink wiring,
// hot-plug rescan) — one helper per pattern, so no rig re-derives it.
#[cfg(feature = "midir")]
pub mod attach;

/// The **native PipeWire** MIDI backend, by name. Prefer [`MidiInput`]
/// (feature `native`), which is this on Linux and the platform's own backend
/// elsewhere.
///
/// The **native PipeWire** MIDI backend (`MidiInput`, `input_ports`, …).
///
/// Enabled by the `pipewire` feature; Linux only. Prefer this over
/// [`midir`] on a PipeWire host: it is one graph node rather than one per
/// port, it discovers hot-plug from the registry instead of polling (which
/// on the midir/JACK path meant creating and destroying a client several
/// times a second), and it needs no `pw-jack` wrapper.
#[cfg(all(feature = "pipewire", target_os = "linux"))]
pub mod pipewire {
    pub use midicore_pipewire::*;
}

/// The midir-backed OS MIDI backend (`MidiInput`, `MidiStream`, `input_ports`,
/// …). Enabled by the `midir` feature; native platforms only.
#[cfg(feature = "midir")]
pub mod midir {
    pub use midicore_midir::*;
}
