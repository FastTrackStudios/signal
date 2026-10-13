//! MIDI CC and other event types for editing

use facet::Facet;

/// A MIDI Control Change event in a take
#[derive(Clone, Debug, Facet)]
pub struct MidiCC {
    /// Index of this CC event in the take
    pub index: u32,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// Controller number (0-127)
    pub controller: u8,
    /// Controller value (0-127)
    pub value: u8,
    /// Position in PPQ
    pub position_ppq: f64,
    /// Whether this event is selected
    pub selected: bool,
}

/// A MIDI Pitch Bend event in a take
#[derive(Clone, Debug, Facet)]
pub struct MidiPitchBend {
    /// Index of this event in the take
    pub index: u32,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// Pitch bend value (-8192 to 8191)
    pub value: i16,
    /// Position in PPQ
    pub position_ppq: f64,
    /// Whether this event is selected
    pub selected: bool,
}

/// A MIDI Program Change event in a take
#[derive(Clone, Debug, Facet)]
pub struct MidiProgramChange {
    /// Index of this event in the take
    pub index: u32,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// Program number (0-127)
    pub program: u8,
    /// Position in PPQ
    pub position_ppq: f64,
}

/// A MIDI Channel Pressure (mono aftertouch) event in a take.
/// Affects all currently-playing notes on the channel.
#[derive(Clone, Debug, Facet)]
pub struct MidiChannelPressure {
    pub index: u32,
    pub channel: u8,
    pub pressure: u8,
    pub position_ppq: f64,
    pub selected: bool,
}

/// A MIDI Poly Pressure (per-note aftertouch) event in a take.
/// Affects a single note's velocity expression after attack.
#[derive(Clone, Debug, Facet)]
pub struct MidiPolyPressure {
    pub index: u32,
    pub channel: u8,
    /// Note number this aftertouch applies to.
    pub note: u8,
    pub pressure: u8,
    pub position_ppq: f64,
    pub selected: bool,
}

/// MPE/VST3-style per-note expression dimensions. Maps one-to-one
/// onto CLAP's `NoteExpressionType` and VST3's `NoteExpressionTypeIDs`,
/// so the renderer can translate without losing fidelity.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
pub enum NoteExpressionDim {
    /// Per-note volume scale. CLAP: `Volume`, VST3: `kVolumeTypeID`.
    Volume = 0,
    /// Per-note pan, `0..=1` (0 = full left). CLAP: `Pan`, VST3: `kPanTypeID`.
    Pan = 1,
    /// Per-note tuning in semitones (-120..+120). CLAP: `Tuning`,
    /// VST3: `kTuningTypeID`.
    Tuning = 2,
    /// Vibrato depth `0..=1`. CLAP: `Vibrato`, VST3: `kVibratoTypeID`.
    Vibrato = 3,
    /// Generic "expression" dimension `0..=1`. CLAP: `Expression`,
    /// VST3: `kExpressionTypeID`.
    Expression = 4,
    /// Timbre/brightness `0..=1`. CLAP: `Brightness`, VST3: `kBrightnessTypeID`.
    Brightness = 5,
    /// Per-note pressure `0..=1`. CLAP: `Pressure`, VST3: stored as
    /// `kCustomStart + 0` since the VST3 standard set has no Pressure
    /// slot (poly pressure is a separate event).
    Pressure = 6,
}

/// One per-note expression point. Targets a specific note voice via
/// `(channel, note)` and optionally `note_id` (matches the
/// note-on's noteId when emitted by the renderer for MPE-style
/// instruments).
#[derive(Clone, Debug, Facet)]
pub struct MidiNoteExpression {
    pub index: u32,
    pub channel: u8,
    /// Pitch the note voice is sounding. Use `0xFF` for "any note".
    pub note: u8,
    pub dimension: NoteExpressionDim,
    /// Dimension-specific value range. Volume = 0..4 (linear gain),
    /// Pan/Vibrato/Expression/Brightness/Pressure = 0..1, Tuning =
    /// semitones.
    pub value: f64,
    pub position_ppq: f64,
    pub selected: bool,
}

/// A MIDI System Exclusive event in a take
#[derive(Clone, Debug, Facet)]
pub struct MidiSysEx {
    /// Index of this event in the take
    pub index: u32,
    /// Position in PPQ
    pub position_ppq: f64,
    /// SysEx data (including F0 and F7)
    pub data: Vec<u8>,
}

impl Default for MidiCC {
    fn default() -> Self {
        Self {
            index: 0,
            channel: 0,
            controller: 0,
            value: 0,
            position_ppq: 0.0,
            selected: false,
        }
    }
}

impl Default for MidiPitchBend {
    fn default() -> Self {
        Self {
            index: 0,
            channel: 0,
            value: 0,
            position_ppq: 0.0,
            selected: false,
        }
    }
}

impl Default for MidiProgramChange {
    fn default() -> Self {
        Self {
            index: 0,
            channel: 0,
            program: 0,
            position_ppq: 0.0,
        }
    }
}

impl Default for MidiChannelPressure {
    fn default() -> Self {
        Self {
            index: 0,
            channel: 0,
            pressure: 0,
            position_ppq: 0.0,
            selected: false,
        }
    }
}

impl Default for MidiPolyPressure {
    fn default() -> Self {
        Self {
            index: 0,
            channel: 0,
            note: 0,
            pressure: 0,
            position_ppq: 0.0,
            selected: false,
        }
    }
}

impl Default for MidiNoteExpression {
    fn default() -> Self {
        Self {
            index: 0,
            channel: 0,
            note: 60,
            dimension: NoteExpressionDim::Volume,
            value: 1.0,
            position_ppq: 0.0,
            selected: false,
        }
    }
}

impl Default for MidiSysEx {
    fn default() -> Self {
        Self {
            index: 0,
            position_ppq: 0.0,
            data: Vec::new(),
        }
    }
}

/// Common MIDI CC numbers
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
pub enum CommonCC {
    /// Bank Select MSB
    BankSelectMsb = 0,
    /// Modulation Wheel
    ModWheel = 1,
    /// Breath Controller
    Breath = 2,
    /// Foot Controller
    Foot = 4,
    /// Portamento Time
    PortamentoTime = 5,
    /// Data Entry MSB
    DataEntryMsb = 6,
    /// Channel Volume
    Volume = 7,
    /// Balance
    Balance = 8,
    /// Pan
    Pan = 10,
    /// Expression
    Expression = 11,
    /// Bank Select LSB
    BankSelectLsb = 32,
    /// Sustain Pedal
    Sustain = 64,
    /// Portamento On/Off
    Portamento = 65,
    /// Sostenuto Pedal
    Sostenuto = 66,
    /// Soft Pedal
    SoftPedal = 67,
    /// Legato Footswitch
    Legato = 68,
    /// All Sound Off
    AllSoundOff = 120,
    /// Reset All Controllers
    ResetAllControllers = 121,
    /// All Notes Off
    AllNotesOff = 123,
}
