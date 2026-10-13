//! MIDI message types for real-time I/O.
//!
//! The canonical event type is [`midicore::MidiEvent`] — a structured enum over
//! constrained newtypes ([`Channel`], [`KeyNumber`], [`Velocity`], …) that makes
//! illegal MIDI values unrepresentable. It is re-exported here (and, via the
//! `live_midi` glob, at the `daw_proto` crate root) so existing call sites keep
//! a stable path. The old hand-rolled `MidiMessage` enum (plain `u8`/`i16`
//! fields, `note` where midicore says `key`) is gone; construct events with the
//! newtypes and read them back with the newtype accessors (`.get()`, `.index()`).

use facet::Facet;

// The canonical MIDI event + the newtypes callers need to build/read it.
pub use midicore::{
    Channel, ControllerNumber, ControllerValue, KeyNumber, MidiEvent, PitchBend, Pressure,
    ProgramNumber, RawShortMessage, ShortMessage, Velocity,
};

/// Host-side conveniences on [`MidiEvent`] that daw call sites relied on when
/// this was a bespoke enum. Kept as an extension trait since `MidiEvent` is
/// defined in `midicore`.
pub trait MidiEventExt {
    /// Convert to raw MIDI bytes `(status, data1, data2)`.
    ///
    /// Returns `None` for SysEx (not a short message). Two-byte messages
    /// (program change / channel pressure) report `data2 = 0`, matching the
    /// old `MidiMessage::to_raw_bytes` behavior.
    fn to_raw_bytes(&self) -> Option<(u8, u8, u8)>;
}

impl MidiEventExt for MidiEvent {
    fn to_raw_bytes(&self) -> Option<(u8, u8, u8)> {
        RawShortMessage::from_event(self).map(|r| {
            let [status, d1, d2] = r.bytes();
            (status, d1, d2)
        })
    }
}

/// Target queue for StuffMIDIMessage injection.
///
/// Controls where injected MIDI is routed within REAPER.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Facet)]
pub enum StuffMidiTarget {
    /// Virtual MIDI keyboard queue — routes to armed tracks with VKB input.
    #[default]
    VirtualMidiKeyboard,
    /// MIDI-as-control-input queue — routes to control surfaces / actions.
    ControlInput,
    /// Virtual MIDI keyboard on the currently selected channel.
    VirtualMidiKeyboardCurrentChannel,
}

/// When to send a MIDI message
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Facet)]
pub enum SendMidiTiming {
    /// Send immediately
    #[default]
    Instantly,
    /// Send at a specific frame offset (in 1/1024000 second units)
    AtFrameOffset(u32),
}
