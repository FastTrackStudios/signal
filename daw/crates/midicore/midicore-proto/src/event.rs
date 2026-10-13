//! The structured MIDI event and its wire (byte) codec.
//!
//! [`MidiEvent`] is the rich, matchable form — one variant per message type,
//! each carrying its semantic newtypes ([`KeyNumber`], [`Velocity`], …). For the
//! allocation-free real-time path use [`crate::raw::RawShortMessage`], which
//! round-trips with the short (non-SysEx) subset of this enum.

use facet::Facet;

use crate::number::{
    Channel, ControllerNumber, ControllerValue, KeyNumber, PitchBend, Pressure, ProgramNumber,
    Velocity, U7,
};

/// A parsed MIDI event.
///
/// Channel-voice variants carry their [`Channel`]; system messages don't. A
/// flat, exhaustive enum so callers `match` without touching status bytes.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum MidiEvent {
    /// Note released. (A `NoteOn` with velocity 0 decodes to this.)
    NoteOff {
        channel: Channel,
        key: KeyNumber,
        velocity: Velocity,
    },
    /// Note struck.
    NoteOn {
        channel: Channel,
        key: KeyNumber,
        velocity: Velocity,
    },
    /// Per-note (polyphonic) aftertouch.
    PolyAftertouch {
        channel: Channel,
        key: KeyNumber,
        pressure: Pressure,
    },
    /// Control change (CC).
    ControlChange {
        channel: Channel,
        controller: ControllerNumber,
        value: ControllerValue,
    },
    /// Program (patch) change.
    ProgramChange {
        channel: Channel,
        program: ProgramNumber,
    },
    /// Channel-wide aftertouch.
    ChannelPressure {
        channel: Channel,
        pressure: Pressure,
    },
    /// Pitch bend.
    PitchBend { channel: Channel, bend: PitchBend },
    /// System-realtime: timing clock (24 per quarter note).
    Clock,
    /// System-realtime: start.
    Start,
    /// System-realtime: continue.
    Continue,
    /// System-realtime: stop.
    Stop,
    /// System-realtime: active sensing.
    ActiveSensing,
    /// System-realtime: reset.
    Reset,
    /// System-exclusive payload (without the enclosing `0xF0`/`0xF7`).
    SysEx(Vec<u8>),
}

/// Error decoding a MIDI byte stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The slice was empty or a message was truncated mid-way.
    Truncated,
    /// The leading byte was a data byte, not a status byte (running status is
    /// not resolvable without prior context).
    UnexpectedDataByte,
    /// A status byte this codec does not model (e.g. MTC quarter-frame).
    Unsupported(u8),
}

impl MidiEvent {
    /// The channel this event targets, if it is a channel-voice message.
    pub const fn channel(&self) -> Option<Channel> {
        match self {
            MidiEvent::NoteOff { channel, .. }
            | MidiEvent::NoteOn { channel, .. }
            | MidiEvent::PolyAftertouch { channel, .. }
            | MidiEvent::ControlChange { channel, .. }
            | MidiEvent::ProgramChange { channel, .. }
            | MidiEvent::ChannelPressure { channel, .. }
            | MidiEvent::PitchBend { channel, .. } => Some(*channel),
            _ => None,
        }
    }

    /// Decode a single event from the front of `bytes`, returning the event and
    /// the number of bytes consumed. Does not resolve running status.
    pub fn decode(bytes: &[u8]) -> Result<(MidiEvent, usize), DecodeError> {
        let status = *bytes.first().ok_or(DecodeError::Truncated)?;
        if status < 0x80 {
            return Err(DecodeError::UnexpectedDataByte);
        }

        // System messages (0xF0..=0xFF) have no channel nibble.
        if status >= 0xF0 {
            return match status {
                0xF8 => Ok((MidiEvent::Clock, 1)),
                0xFA => Ok((MidiEvent::Start, 1)),
                0xFB => Ok((MidiEvent::Continue, 1)),
                0xFC => Ok((MidiEvent::Stop, 1)),
                0xFE => Ok((MidiEvent::ActiveSensing, 1)),
                0xFF => Ok((MidiEvent::Reset, 1)),
                0xF0 => {
                    // SysEx runs until the 0xF7 terminator.
                    let end = bytes
                        .iter()
                        .position(|&b| b == 0xF7)
                        .ok_or(DecodeError::Truncated)?;
                    Ok((MidiEvent::SysEx(bytes[1..end].to_vec()), end + 1))
                }
                other => Err(DecodeError::Unsupported(other)),
            };
        }

        let channel = Channel::new(status & 0x0F);
        let b1 = bytes.get(1).copied();
        let b2 = bytes.get(2).copied();
        let d1 = || b1.ok_or(DecodeError::Truncated);
        let d2 = || b2.ok_or(DecodeError::Truncated);

        match status & 0xF0 {
            0x80 => Ok((
                MidiEvent::NoteOff {
                    channel,
                    key: KeyNumber::new(d1()?),
                    velocity: Velocity::new(d2()?),
                },
                3,
            )),
            0x90 => {
                let (key, vel) = (KeyNumber::new(d1()?), Velocity::new(d2()?));
                // Note-on with velocity 0 is the canonical "note off".
                let ev = if vel.get() == 0 {
                    MidiEvent::NoteOff {
                        channel,
                        key,
                        velocity: vel,
                    }
                } else {
                    MidiEvent::NoteOn {
                        channel,
                        key,
                        velocity: vel,
                    }
                };
                Ok((ev, 3))
            }
            0xA0 => Ok((
                MidiEvent::PolyAftertouch {
                    channel,
                    key: KeyNumber::new(d1()?),
                    pressure: Pressure::new(d2()?),
                },
                3,
            )),
            0xB0 => Ok((
                MidiEvent::ControlChange {
                    channel,
                    controller: ControllerNumber::new(d1()?),
                    value: ControllerValue::new(d2()?),
                },
                3,
            )),
            0xC0 => Ok((
                MidiEvent::ProgramChange {
                    channel,
                    program: ProgramNumber::new(d1()?),
                },
                2,
            )),
            0xD0 => Ok((
                MidiEvent::ChannelPressure {
                    channel,
                    pressure: Pressure::new(d1()?),
                },
                2,
            )),
            0xE0 => Ok((
                MidiEvent::PitchBend {
                    channel,
                    bend: PitchBend::from_halves(U7::new(d1()?), U7::new(d2()?)),
                },
                3,
            )),
            other => Err(DecodeError::Unsupported(other)),
        }
    }

    /// Encode into `out`, returning the number of bytes written, or `None` if
    /// `out` is too small. `SysEx` includes its `0xF0`/`0xF7` framing.
    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        fn put(out: &mut [u8], bytes: &[u8]) -> Option<usize> {
            if out.len() < bytes.len() {
                return None;
            }
            out[..bytes.len()].copy_from_slice(bytes);
            Some(bytes.len())
        }
        let ch = |c: &Channel| c.index();
        match self {
            MidiEvent::NoteOff {
                channel,
                key,
                velocity,
            } => put(out, &[0x80 | ch(channel), key.get(), velocity.get()]),
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => put(out, &[0x90 | ch(channel), key.get(), velocity.get()]),
            MidiEvent::PolyAftertouch {
                channel,
                key,
                pressure,
            } => put(out, &[0xA0 | ch(channel), key.get(), pressure.get()]),
            MidiEvent::ControlChange {
                channel,
                controller,
                value,
            } => put(out, &[0xB0 | ch(channel), controller.get(), value.get()]),
            MidiEvent::ProgramChange { channel, program } => {
                put(out, &[0xC0 | ch(channel), program.get()])
            }
            MidiEvent::ChannelPressure { channel, pressure } => {
                put(out, &[0xD0 | ch(channel), pressure.get()])
            }
            MidiEvent::PitchBend { channel, bend } => {
                let (lsb, msb) = bend.to_halves();
                put(out, &[0xE0 | ch(channel), lsb.get(), msb.get()])
            }
            MidiEvent::Clock => put(out, &[0xF8]),
            MidiEvent::Start => put(out, &[0xFA]),
            MidiEvent::Continue => put(out, &[0xFB]),
            MidiEvent::Stop => put(out, &[0xFC]),
            MidiEvent::ActiveSensing => put(out, &[0xFE]),
            MidiEvent::Reset => put(out, &[0xFF]),
            MidiEvent::SysEx(payload) => {
                if out.len() < payload.len() + 2 {
                    return None;
                }
                out[0] = 0xF0;
                out[1..1 + payload.len()].copy_from_slice(payload);
                out[1 + payload.len()] = 0xF7;
                Some(payload.len() + 2)
            }
        }
    }
}

/// Canonical one-line rendering of a MIDI message for a monitor/log — 1-based
/// channel (like a DAW), fixed-width verbs so a column of them aligns. This is
/// midicore's single source of truth for how a `MidiEvent` reads to a human;
/// UIs (e.g. `midicore-ui`'s monitor panel) format through it.
impl core::fmt::Display for MidiEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => write!(
                f,
                "ch{:<2} note-on  {:>3} vel {}",
                channel.number(),
                key.get(),
                velocity.get()
            ),
            MidiEvent::NoteOff { channel, key, .. } => {
                write!(f, "ch{:<2} note-off {:>3}", channel.number(), key.get())
            }
            MidiEvent::ControlChange {
                channel,
                controller,
                value,
            } => write!(
                f,
                "ch{:<2} cc {:>3} = {}",
                channel.number(),
                controller.get(),
                value.get()
            ),
            MidiEvent::ProgramChange { channel, program } => {
                write!(f, "ch{:<2} program {}", channel.number(), program.get())
            }
            MidiEvent::ChannelPressure { channel, pressure } => {
                write!(f, "ch{:<2} aftertouch {}", channel.number(), pressure.get())
            }
            MidiEvent::PolyAftertouch {
                channel,
                key,
                pressure,
            } => write!(
                f,
                "ch{:<2} poly-at {} = {}",
                channel.number(),
                key.get(),
                pressure.get()
            ),
            MidiEvent::PitchBend { channel, bend } => {
                write!(f, "ch{:<2} pitch-bend {}", channel.number(), bend.offset())
            }
            MidiEvent::Clock => write!(f, "clock"),
            MidiEvent::Start => write!(f, "start"),
            MidiEvent::Continue => write!(f, "continue"),
            MidiEvent::Stop => write!(f, "stop"),
            MidiEvent::ActiveSensing => write!(f, "active-sensing"),
            MidiEvent::Reset => write!(f, "reset"),
            MidiEvent::SysEx(bytes) => write!(f, "sysex ({} bytes)", bytes.len()),
        }
    }
}

/// The broad category of a MIDI message — lets a UI colour-code the monitor
/// without re-matching every variant. Part of midicore so the taxonomy is
/// shared, not reinvented per front-end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MidiKind {
    NoteOn,
    NoteOff,
    ControlChange,
    Aftertouch,
    PitchBend,
    Program,
    System,
}

impl MidiEvent {
    /// This message's [`MidiKind`] (for colouring / filtering a monitor).
    pub const fn kind(&self) -> MidiKind {
        match self {
            MidiEvent::NoteOn { .. } => MidiKind::NoteOn,
            MidiEvent::NoteOff { .. } => MidiKind::NoteOff,
            MidiEvent::ControlChange { .. } => MidiKind::ControlChange,
            MidiEvent::ChannelPressure { .. } | MidiEvent::PolyAftertouch { .. } => {
                MidiKind::Aftertouch
            }
            MidiEvent::PitchBend { .. } => MidiKind::PitchBend,
            MidiEvent::ProgramChange { .. } => MidiKind::Program,
            _ => MidiKind::System,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_on_roundtrips() {
        let ev = MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(60),
            velocity: Velocity::new(100),
        };
        let mut buf = [0u8; 3];
        let n = ev.encode(&mut buf).unwrap();
        assert_eq!(&buf[..n], &[0x90, 60, 100]);
        assert_eq!(MidiEvent::decode(&buf[..n]).unwrap(), (ev, 3));
    }

    #[test]
    fn note_on_zero_velocity_is_note_off() {
        let (ev, _) = MidiEvent::decode(&[0x90, 60, 0]).unwrap();
        assert!(matches!(ev, MidiEvent::NoteOff { .. }));
    }

    #[test]
    fn pitch_bend_center_is_8192() {
        let (ev, _) = MidiEvent::decode(&[0xE0, 0x00, 0x40]).unwrap();
        match ev {
            MidiEvent::PitchBend { bend, .. } => {
                assert_eq!(bend.get(), 8192);
                assert_eq!(bend.offset(), 0);
            }
            _ => panic!("expected pitch bend"),
        }
    }
}
