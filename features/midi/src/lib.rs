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
//! use crate::{MidiInput, PortSelector};
//! let input = MidiInput::open(PortSelector::All, |ev| { /* ... */ })?;
//! ```
//!
//! Backends implement [`InputBackend`]; see [`backend`] for the contract.

#[cfg(feature = "coremidi")]
pub mod coremidi;
#[cfg(feature = "midir")]
pub mod midir;
#[cfg(feature = "pipewire")]
pub mod pipewire;
pub mod proto;
#[cfg(feature = "ui")]
pub mod ui;
#[cfg(feature = "webmidi")]
pub mod webmidi;

pub use crate::proto::*;

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

/// One shared hardware-MIDI input for the whole process, fanned out to
/// every subscriber (every rig, the engine's live MIDI).
#[cfg(all(
    feature = "native",
    not(target_arch = "wasm32"),
    any(target_os = "linux", target_os = "macos", feature = "midir")
))]
pub mod hub;

/// Instrumented MIDI attach: the one wide event every (re)attach emits.
#[cfg(all(feature = "midir", not(target_arch = "wasm32")))]
pub mod telemetry;
