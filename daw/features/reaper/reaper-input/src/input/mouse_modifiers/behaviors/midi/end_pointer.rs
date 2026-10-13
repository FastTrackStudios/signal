//! MIDI End Pointer behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// MIDI End Pointer behaviors
    pub enum MidiEndPointerBehavior {
        NoAction => (0, "No action"),
        EditMidiSourceLoopLength => (1, "Edit MIDI source loop length"),
        EditMidiSourceLoopLengthIgnoringSnap => (2, "Edit MIDI source loop length ignoring snap"),
        StretchMidiSourceLoopLength => (3, "Stretch MIDI source loop length"),
        StretchMidiSourceLoopLengthIgnoringSnap => (4, "Stretch MIDI source loop length ignoring snap"),
    }
}
