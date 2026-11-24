//! MIDI in: what a host hands the processor with a block.

/// One event, at a frame offset into the host block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidiEvent {
    /// Frames from the start of the block.
    pub offset: u32,
    pub channel: u8,
    pub message: MidiMessage,
}

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MidiMessage {
    /// Velocity 1–127.
    NoteOn {
        note: u8,
        velocity: u8,
    },
    NoteOff {
        note: u8,
    },
    Cc {
        cc: u8,
        value: u8,
    },
    /// −8192…8191.
    PitchBend(i16),
}

impl MidiEvent {
    #[must_use]
    pub const fn note_on(offset: u32, note: u8, velocity: u8) -> Self {
        Self {
            offset,
            channel: 0,
            message: MidiMessage::NoteOn { note, velocity },
        }
    }

    #[must_use]
    pub const fn note_off(offset: u32, note: u8) -> Self {
        Self {
            offset,
            channel: 0,
            message: MidiMessage::NoteOff { note },
        }
    }

    #[must_use]
    pub const fn cc(offset: u32, cc: u8, value: u8) -> Self {
        Self {
            offset,
            channel: 0,
            message: MidiMessage::Cc { cc, value },
        }
    }
}

impl MidiEvent {
    /// As a block takes it (the offset is the processor's: it splits the
    /// host block there).
    #[must_use]
    pub const fn to_block(&self) -> plugin_processors::Midi {
        use plugin_processors::Midi;
        let channel = self.channel;
        match self.message {
            MidiMessage::NoteOn { note, velocity } => Midi::NoteOn {
                channel,
                note,
                velocity,
            },
            MidiMessage::NoteOff { note } => Midi::NoteOff { channel, note },
            MidiMessage::Cc { cc, value } => Midi::Cc { channel, cc, value },
            MidiMessage::PitchBend(value) => Midi::PitchBend { channel, value },
        }
    }
}
