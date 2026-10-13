//! MIDI Editor child window IDs.

use crate::ChildId;

/// MIDI Editor child window IDs.
pub struct MidiEditor;

impl MidiEditor {
    /// MIDI notes view - Class: REAPERMIDIView
    pub const NOTES_VIEW: ChildId = ChildId(1001);
    /// MIDI piano view - Class: REAPERMIDIPiano
    pub const PIANO_VIEW: ChildId = ChildId(1003);
    /// CC lane selector (left of notes view) - matches SWS `WndControlIDs::midi_ccLaneSelector`
    pub const CC_LANE_SELECTOR: ChildId = ChildId(0x3F1);
}
