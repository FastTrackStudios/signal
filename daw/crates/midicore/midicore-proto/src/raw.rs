//! The packed, `Copy`, real-time short-message form.
//!
//! [`RawShortMessage`] is three bytes, `Copy`, and never allocates — the shape
//! you want on the audio/MIDI hot path and in fixed-size queues. It carries any
//! *short* message (channel-voice + system, ≤3 bytes) but **not** SysEx, which
//! is variable-length and stays in [`MidiEvent`]. The two forms convert
//! losslessly for the short subset, so callers pick raw for throughput and
//! structured ([`MidiEvent`]) for matching.
//!
//! Modeled on `helgoboss-midi`'s `ShortMessage` / `RawShortMessage` split.

use facet::Facet;

use crate::event::MidiEvent;
use crate::number::{
    Channel, ControllerNumber, ControllerValue, KeyNumber, PitchBend, Velocity, U7,
};

/// Number of meaningful bytes for a short-message status byte.
const fn message_len(status: u8) -> usize {
    if status >= 0xF0 {
        // System real-time (>= 0xF8) is genuinely 1 byte; other
        // system-common is treated conservatively as 1 (unsupported by
        // the codec).
        1
    } else {
        match status & 0xF0 {
            0xC0 | 0xD0 => 2, // program change / channel pressure
            _ => 3,
        }
    }
}

/// Read-only accessors shared by every short-message representation.
pub trait ShortMessage {
    fn status_byte(&self) -> u8;
    fn data_byte_1(&self) -> U7;
    fn data_byte_2(&self) -> U7;

    /// The channel, for channel-voice status bytes (`0x80..=0xEF`).
    fn channel(&self) -> Option<Channel> {
        let s = self.status_byte();
        if (0x80..=0xEF).contains(&s) {
            Some(Channel::new(s & 0x0F))
        } else {
            None
        }
    }
}

/// A short MIDI message packed into three bytes. `Copy`, allocation-free.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Facet)]
pub struct RawShortMessage([u8; 3]);

// len() is 1..=3 by construction — an `is_empty` would be misleading.
#[allow(clippy::len_without_is_empty)]
impl RawShortMessage {
    /// Build from explicit bytes (data bytes are masked to 7 bits).
    pub const fn from_bytes(status: u8, data1: u8, data2: u8) -> Self {
        Self([status, data1 & 0x7f, data2 & 0x7f])
    }

    /// The three raw bytes; only the first [`RawShortMessage::len`] are meaningful.
    pub const fn bytes(&self) -> [u8; 3] {
        self.0
    }

    /// Number of meaningful bytes (1–3) given the status byte.
    pub const fn len(&self) -> usize {
        message_len(self.0[0])
    }

    /// Convert to the structured form; `None` only for status bytes the codec
    /// doesn't model.
    pub fn to_event(self) -> Option<MidiEvent> {
        MidiEvent::decode(&self.0[..self.len()])
            .ok()
            .map(|(e, _)| e)
    }

    /// Pack a structured event, or `None` if it is SysEx (not a short message).
    pub fn from_event(event: &MidiEvent) -> Option<Self> {
        if matches!(event, MidiEvent::SysEx(_)) {
            return None;
        }
        let mut buf = [0u8; 3];
        let n = event.encode(&mut buf)?;
        if n > 3 {
            return None;
        }
        Some(Self(buf))
    }

    // ── Ergonomic constructors (no `MidiEvent` allocation) ──
    pub fn note_on(channel: Channel, key: KeyNumber, velocity: Velocity) -> Self {
        Self([0x90 | channel.index(), key.get(), velocity.get()])
    }
    pub fn note_off(channel: Channel, key: KeyNumber, velocity: Velocity) -> Self {
        Self([0x80 | channel.index(), key.get(), velocity.get()])
    }
    pub fn control_change(
        channel: Channel,
        controller: ControllerNumber,
        value: ControllerValue,
    ) -> Self {
        Self([0xB0 | channel.index(), controller.get(), value.get()])
    }
    pub fn pitch_bend(channel: Channel, bend: PitchBend) -> Self {
        let (lsb, msb) = bend.to_halves();
        Self([0xE0 | channel.index(), lsb.get(), msb.get()])
    }

    // ── Ergonomic note/CC predicates for hot-path drain loops (UIs that poll a
    //    stream and only care about notes + CCs). Byte-level, no allocation. ──

    /// True for a note-on with non-zero velocity (velocity 0 is a note-off).
    pub const fn is_note_on(&self) -> bool {
        (self.0[0] & 0xF0) == 0x90 && self.0[2] > 0
    }
    /// True for a note-off, including the note-on-velocity-0 convention.
    pub const fn is_note_off(&self) -> bool {
        (self.0[0] & 0xF0) == 0x80 || ((self.0[0] & 0xF0) == 0x90 && self.0[2] == 0)
    }
    /// True for a Control Change (status `0xB0`).
    pub const fn is_cc(&self) -> bool {
        (self.0[0] & 0xF0) == 0xB0
    }
    /// Note number (data byte 1) for note messages.
    pub const fn note(&self) -> u8 {
        self.0[1]
    }
    /// Velocity (data byte 2) for note messages.
    pub const fn velocity(&self) -> u8 {
        self.0[2]
    }
    /// Controller number (data byte 1) for a CC.
    pub const fn controller(&self) -> u8 {
        self.0[1]
    }
    /// Controller value (data byte 2) for a CC.
    pub const fn cc_value(&self) -> u8 {
        self.0[2]
    }
    /// Channel (low nibble of the status byte).
    pub const fn channel_raw(&self) -> u8 {
        self.0[0] & 0x0F
    }
    /// The raw status byte (data-0).
    pub const fn status(&self) -> u8 {
        self.0[0]
    }
}

impl ShortMessage for RawShortMessage {
    fn status_byte(&self) -> u8 {
        self.0[0]
    }
    fn data_byte_1(&self) -> U7 {
        U7::new(self.0[1] & 0x7f)
    }
    fn data_byte_2(&self) -> U7 {
        U7::new(self.0[2] & 0x7f)
    }
}

/// A short message packs back and forth with the structured form.
impl TryFrom<&MidiEvent> for RawShortMessage {
    type Error = ();
    fn try_from(event: &MidiEvent) -> Result<Self, ()> {
        RawShortMessage::from_event(event).ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_note_on_roundtrips_to_structured() {
        let raw = RawShortMessage::note_on(Channel::new(2), KeyNumber::new(64), Velocity::new(96));
        assert_eq!(raw.len(), 3);
        match raw.to_event().unwrap() {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => {
                assert_eq!(channel.number(), 3);
                assert_eq!(key.get(), 64);
                assert_eq!(velocity.get(), 96);
            }
            other => panic!("expected note-on, got {other:?}"),
        }
    }

    #[test]
    fn program_change_is_two_bytes() {
        let raw = RawShortMessage::from_bytes(0xC0, 5, 0);
        assert_eq!(raw.len(), 2);
    }

    #[test]
    fn sysex_has_no_raw_form() {
        assert!(RawShortMessage::from_event(&MidiEvent::SysEx(vec![1, 2, 3])).is_none());
    }

    #[test]
    fn raw_is_copy() {
        let a = RawShortMessage::note_off(Channel::new(0), KeyNumber::new(60), Velocity::new(0));
        let b = a; // Copy
        assert_eq!(a, b);
    }
}
