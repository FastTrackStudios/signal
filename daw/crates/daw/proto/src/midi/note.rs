//! MIDI note types for editing

use facet::Facet;

/// A MIDI note in a take
#[derive(Clone, Debug, Facet)]
pub struct MidiNote {
    /// Index of this note in the take
    pub index: u32,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// Note pitch (0-127, 60 = middle C)
    pub pitch: u8,
    /// Note velocity (1-127)
    pub velocity: u8,
    /// Start position in PPQ (quarter notes from take start)
    pub start_ppq: f64,
    /// Duration in PPQ
    pub length_ppq: f64,
    /// Whether this note is selected
    pub selected: bool,
    /// Whether this note is muted
    pub muted: bool,
}

/// Parameters for creating a new MIDI note
#[derive(Clone, Debug, Facet)]
pub struct MidiNoteCreate {
    /// MIDI channel (0-15)
    pub channel: u8,
    /// Note pitch (0-127)
    pub pitch: u8,
    /// Note velocity (1-127)
    pub velocity: u8,
    /// Start position in PPQ
    pub start_ppq: f64,
    /// Duration in PPQ
    pub length_ppq: f64,
}

impl MidiNoteCreate {
    /// Create a new note with default channel 0
    pub fn new(pitch: u8, velocity: u8, start_ppq: f64, length_ppq: f64) -> Self {
        Self {
            channel: 0,
            pitch: pitch & 0x7F,
            velocity: velocity.clamp(1, 127),
            start_ppq,
            length_ppq,
        }
    }

    /// Create with specific channel
    pub fn with_channel(
        channel: u8,
        pitch: u8,
        velocity: u8,
        start_ppq: f64,
        length_ppq: f64,
    ) -> Self {
        Self {
            channel: channel & 0x0F,
            pitch: pitch & 0x7F,
            velocity: velocity.clamp(1, 127),
            start_ppq,
            length_ppq,
        }
    }
}

impl MidiNote {
    /// Get the end position in PPQ
    pub fn end_ppq(&self) -> f64 {
        self.start_ppq + self.length_ppq
    }

    /// Check if this note overlaps with a PPQ range
    pub fn overlaps(&self, start: f64, end: f64) -> bool {
        self.start_ppq < end && start < self.end_ppq()
    }

    /// Get note name (e.g., "C4", "F#5")
    pub fn note_name(&self) -> String {
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        let octave = (self.pitch / 12) as i32 - 1;
        let name = NAMES[(self.pitch % 12) as usize];
        format!("{}{}", name, octave)
    }
}

impl Default for MidiNote {
    fn default() -> Self {
        Self {
            index: 0,
            channel: 0,
            pitch: 60, // Middle C
            velocity: 100,
            start_ppq: 0.0,
            length_ppq: 1.0, // Quarter note
            selected: false,
            muted: false,
        }
    }
}

impl Default for MidiNoteCreate {
    fn default() -> Self {
        Self {
            channel: 0,
            pitch: 60,
            velocity: 100,
            start_ppq: 0.0,
            length_ppq: 1.0,
        }
    }
}
