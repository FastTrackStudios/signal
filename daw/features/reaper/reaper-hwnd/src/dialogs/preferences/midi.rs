//! Preferences -> MIDI page child window IDs.

use crate::ChildId;

/// Preferences -> MIDI page child window IDs.
pub struct MidiPrefs;

impl MidiPrefs {
    /// MIDI input quantize dropdown - Class: ComboBox
    pub const INPUT_QUANTIZE: ChildId = ChildId(1000);
    /// Input quantize label - Class: Static
    pub const INPUT_QUANTIZE_LABEL: ChildId = ChildId(1001);
    /// MIDI output latency compensation inputbox - Class: Edit
    pub const OUTPUT_LATENCY_COMP: ChildId = ChildId(1002);
    /// Latency compensation label - Class: Static
    pub const LATENCY_COMP_LABEL: ChildId = ChildId(1003);
    /// Send all-notes-off on stop - Class: Button
    pub const ALL_NOTES_OFF_ON_STOP: ChildId = ChildId(1004);
    /// Reset CC values on stop - Class: Button
    pub const RESET_CC_ON_STOP: ChildId = ChildId(1005);
    /// MIDI settings label - Class: Static
    pub const MIDI_LABEL: ChildId = ChildId(1006);
}
