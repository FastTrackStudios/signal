//! The in-process MIDI **input backend** contract.
//!
//! [`service`](crate::service) is the *remote* surface — ports and streams as
//! RPC. This is the other side of that boundary: what a platform adapter
//! implements to be the process's hardware MIDI input.
//!
//! | platform | backend crate | how sources reach the input |
//! |---|---|---|
//! | Linux | `midicore-pipewire` | one graph node; sources are *linked* into its port |
//! | macOS | `midicore-macos` | one CoreMIDI client + input port; sources are *connected* to it |
//! | browser | `midicore-wasm` | one Web MIDI access; `midimessage` listeners on each input |
//! | elsewhere | `midicore-midir` | one midir connection per source |
//!
//! Consumers never name a backend: `midicore::MidiInput` is whichever one the
//! platform has.
//!
//! # The shape
//!
//! **One input, many sources.** An input is a single sink that any number of
//! sources feed. Which sources is a *set* of [`PortSelector`]s, unioned: one
//! input serves every rig in a process and each rig names the devices it wants
//! independently, so "what feeds the input" is the union of their answers.
//! An empty set feeds nothing — a rig-less process, not an error.
//!
//! **Selection happens at the source, not the sink.** An event does not carry
//! the device it came from (the PipeWire backend merges every source into one
//! port, so it cannot), which means a selector decides what gets *connected*,
//! never what gets delivered.
//!
//! **Hot-plug is the backend's job.** A device that appears and matches a
//! selector is connected; one that disappears is dropped. Callers re-read
//! [`InputBackend::connected`] if they care; they never re-open.
//!
//! **Dropping the input closes it** and disconnects everything feeding it.

use crate::event::MidiEvent;
use crate::port::{PortInfo, PortSelector, TimedEvent};

/// `Send` on native targets, nothing on wasm.
///
/// Native backends call the sink from an OS MIDI thread, so it must be `Send`.
/// The browser is single-threaded and its sinks routinely capture `Rc` and JS
/// handles, so demanding `Send` there would rule out every real caller. As a
/// supertrait on native, a `F: MaybeSend` bound lets a backend rely on `Send`.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}

/// `Send` on native targets, nothing on wasm.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

/// What to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputConfig {
    /// How the input names itself to the OS: the PipeWire node name, the
    /// CoreMIDI client name. Backends with no such notion ignore it.
    pub name: String,
    /// The sources to connect from the start. See the module docs.
    pub selectors: Vec<PortSelector>,
}

impl InputConfig {
    /// An input called `name` that connects nothing until selected.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            selectors: Vec::new(),
        }
    }

    /// Connect the sources `selectors` names, from the moment it opens.
    #[must_use]
    pub fn selecting(mut self, selectors: Vec<PortSelector>) -> Self {
        self.selectors = selectors;
        self
    }
}

/// A backend could not open (or could not reach) the platform's MIDI system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendError(pub String);

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BackendError {}

impl From<String> for BackendError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// A platform's hardware MIDI input. See the module docs for the contract.
pub trait InputBackend: Sized {
    /// The backend, as it should appear in logs: `"pipewire"`, `"coremidi"`.
    const NAME: &'static str;

    /// Every MIDI source available to connect right now, sorted by name.
    ///
    /// A port's [`PortInfo::id`] is its name on every backend, so a
    /// [`PortSelector::Id`] stored in a preset means the same thing everywhere.
    /// Must be cheap enough to poll: it is how a UI shows what is plugged in.
    fn sources() -> Vec<PortInfo>;

    /// Open the input and start delivering events from the selected sources
    /// to `sink`.
    ///
    /// `sink` runs on the backend's MIDI thread (the browser's event loop on
    /// wasm): keep it cheap and never block in it.
    ///
    /// # Errors
    ///
    /// Only when the platform's MIDI system itself is unreachable. No device
    /// matching the selectors is **not** an error — the input opens connected
    /// to nothing and picks devices up as they arrive.
    fn open<F>(config: InputConfig, sink: F) -> Result<Self, BackendError>
    where
        F: Fn(TimedEvent) + MaybeSend + 'static;

    /// Replace the set of selectors. Only the difference is connected or
    /// disconnected; a set that has not changed disturbs nothing.
    fn select(&self, selectors: Vec<PortSelector>);

    /// The sources connected right now, sorted by name. A virtual port the
    /// backend created for a [`PortSelector::Virtual`] is included, flagged
    /// [`PortInfo::virtual_port`].
    fn connected(&self) -> Vec<PortInfo>;
}

/// Decode every MIDI 1.0 message in `bytes` and hand each to `f`.
///
/// A CoreMIDI packet or a raw byte buffer can hold several messages, and may
/// lean on running status (a data byte continuing the previous status) inside
/// the buffer. Anything that cannot be decoded ends the walk: a wrong note is
/// worse than a missing one on stage.
pub fn decode_all(bytes: &[u8], mut f: impl FnMut(MidiEvent)) {
    let mut rest = bytes;
    let mut running: Option<u8> = None;
    let mut scratch = [0u8; 3];
    while let Some(&first) = rest.first() {
        if first >= 0x80 {
            let Ok((event, used)) = MidiEvent::decode(rest) else {
                return;
            };
            // Only channel messages set running status; system messages
            // (0xF0..) cancel it, as the MIDI 1.0 spec says.
            running = (first < 0xF0).then_some(first);
            f(event);
            rest = &rest[used..];
        } else {
            let Some(status) = running else { return };
            let len = if matches!(status & 0xF0, 0xC0 | 0xD0) {
                1
            } else {
                2
            };
            if rest.len() < len {
                return;
            }
            scratch[0] = status;
            scratch[1..=len].copy_from_slice(&rest[..len]);
            let Ok((event, _)) = MidiEvent::decode(&scratch[..=len]) else {
                return;
            };
            f(event);
            rest = &rest[len..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(bytes: &[u8]) -> Vec<MidiEvent> {
        let mut out = Vec::new();
        decode_all(bytes, |e| out.push(e));
        out
    }

    #[test]
    fn a_buffer_of_several_messages_yields_each() {
        let out = decoded(&[0x90, 60, 100, 0x80, 60, 0, 0xF8]);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[2], MidiEvent::Clock));
    }

    /// Running status: the second note-on has no status byte of its own.
    #[test]
    fn running_status_continues_the_previous_channel_message() {
        let out = decoded(&[0x90, 60, 100, 64, 90]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].channel(), out[1].channel());
    }

    /// Program change is one data byte, so running status steps by one.
    #[test]
    fn running_status_knows_two_byte_messages() {
        assert_eq!(decoded(&[0xC0, 5, 6, 7]).len(), 3);
    }

    /// A system message cancels running status; a stray data byte after it
    /// is dropped rather than guessed at.
    #[test]
    fn a_system_message_cancels_running_status() {
        assert_eq!(decoded(&[0x90, 60, 100, 0xF8, 61, 100]).len(), 2);
    }

    #[test]
    fn a_truncated_message_ends_the_walk() {
        assert_eq!(decoded(&[0x90, 60]).len(), 0);
        assert_eq!(decoded(&[0x90, 60, 100, 0x90, 61]).len(), 1);
    }
}
