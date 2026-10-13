//! The platform's native MIDI input, behind one type.
//!
//! [`MidiInput`] is what an app opens; it never names a backend. Which one it
//! wraps is decided here, per target:
//!
//! | target | [`Backend`] |
//! |---|---|
//! | Linux | `midicore_pipewire::MidiInput` — one PipeWire node |
//! | macOS | `midicore_macos::CoreMidiInput` — one CoreMIDI client + port |
//! | wasm32 | `midicore_wasm::WebMidiInput` — one Web MIDI access |
//! | anything else (with `midir`) | `midicore_midir::SelectableInput` |
//!
//! Every one implements [`InputBackend`](crate::InputBackend); see that
//! module for the contract (one input, a union of selectors, hot-plug handled
//! inside).

use crate::{InputBackend, InputConfig, MaybeSend, PortInfo, PortSelector, TimedEvent};

#[cfg(target_os = "linux")]
pub use midicore_pipewire::MidiInput as Backend;

#[cfg(target_os = "macos")]
pub use midicore_macos::CoreMidiInput as Backend;

/// Prompt for Web MIDI access early (ideally from a user gesture) and learn
/// whether it was refused. Without it, [`input_sources`] is empty until an
/// input has been opened and the browser has answered.
#[cfg(target_arch = "wasm32")]
pub use midicore_wasm::request_access;
#[cfg(target_arch = "wasm32")]
pub use midicore_wasm::WebMidiInput as Backend;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_arch = "wasm32")))]
pub use midicore_midir::SelectableInput as Backend;

/// What an input calls itself to the OS (the PipeWire node, the CoreMIDI
/// client) when the caller does not say.
pub const DEFAULT_INPUT_NAME: &str = "Signal";

/// Every MIDI source available right now, sorted by name.
pub fn input_sources() -> Vec<PortInfo> {
    <Backend as InputBackend>::sources()
}

/// [`input_sources`], names only.
pub fn input_ports() -> Vec<String> {
    input_sources().into_iter().map(|p| p.name).collect()
}

/// The process's hardware MIDI input. Drop to close it.
pub struct MidiInput {
    backend: Backend,
}

impl MidiInput {
    /// Which backend this platform uses, for logs.
    pub const BACKEND: &'static str = <Backend as InputBackend>::NAME;

    /// Open an input named [`DEFAULT_INPUT_NAME`] fed by `selector`.
    ///
    /// # Errors
    ///
    /// Only when the platform's MIDI system is unreachable; no matching
    /// device is not an error (see [`InputBackend::open`]).
    pub fn open<F>(selector: PortSelector, sink: F) -> eyre::Result<Self>
    where
        F: Fn(TimedEvent) + MaybeSend + 'static,
    {
        Self::open_with(
            InputConfig::new(DEFAULT_INPUT_NAME).selecting(vec![selector]),
            sink,
        )
    }

    /// Open an input with an explicit name and selector set.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_with<F>(config: InputConfig, sink: F) -> eyre::Result<Self>
    where
        F: Fn(TimedEvent) + MaybeSend + 'static,
    {
        // Through the trait, always: a backend's own inherent `open` (PipeWire
        // has one) would otherwise shadow it.
        let backend = <Backend as InputBackend>::open(config, sink)
            .map_err(|e| eyre::eyre!("open MIDI input ({}): {e}", Self::BACKEND))?;
        Ok(Self { backend })
    }

    /// Feed the input from the union of `selectors`. Only the difference is
    /// connected or disconnected.
    pub fn select(&self, selectors: Vec<PortSelector>) {
        InputBackend::select(&self.backend, selectors);
    }

    /// Feed the input from `selector` alone.
    pub fn set_selector(&self, selector: PortSelector) {
        self.select(vec![selector]);
    }

    /// The sources feeding the input right now.
    pub fn connected(&self) -> Vec<PortInfo> {
        InputBackend::connected(&self.backend)
    }

    /// [`Self::connected`], names only.
    pub fn ports(&self) -> Vec<String> {
        self.connected().into_iter().map(|p| p.name).collect()
    }
}

/// Bind the process's CoreMIDI device list to a live run loop — call first
/// thing in `main`, before anything touches MIDI. On macOS this is what keeps
/// devices that appear after launch (a pedal switched on) visible at all;
/// see `midicore_macos::init`. Elsewhere, nothing to do.
pub fn init() {
    #[cfg(target_os = "macos")]
    midicore_macos::init();
}

/// Send `bytes` to every MIDI destination whose name contains `device` —
/// for lighting a controller's LEDs from the rig's state. Returns how many
/// it reached (0: not there, or no native output on this platform).
pub fn send_to(device: &str, bytes: &[u8]) -> usize {
    #[cfg(target_os = "macos")]
    {
        static OUT: std::sync::OnceLock<Option<midicore_macos::CoreMidiOutput>> =
            std::sync::OnceLock::new();
        OUT.get_or_init(|| midicore_macos::CoreMidiOutput::open("Signal Out").ok())
            .as_ref()
            .map_or(0, |o| o.send_to(device, bytes))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (device, bytes);
        0
    }
}
