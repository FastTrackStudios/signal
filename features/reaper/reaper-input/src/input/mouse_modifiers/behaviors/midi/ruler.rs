//! MIDI Ruler behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// MIDI Ruler left drag behaviors
    pub enum MidiRulerLeftDragBehavior {
        NoAction => (0, "No action"),
        EditLoopPointRulerOrTimeSelectionPianoRoll => (1, "Edit loop point (ruler) or time selection (piano roll)"),
        EditLoopPointRulerOrTimeSelectionPianoRollIgnoringSnap => (2, "Edit loop point (ruler) or time selection (piano roll) ignoring snap"),
        MoveLoopPointsRulerOrTimeSelectionPianoRoll => (3, "Move loop points (ruler) or time selection (piano roll)"),
        MoveLoopPointsRulerOrTimeSelectionTogether => (4, "Move loop points (ruler) or time selection together"),
        EditLoopPointAndTimeSelectionTogether => (5, "Edit loop point and time selection together"),
        EditLoopPointAndTimeSelectionTogetherIgnoringSnap => (6, "Edit loop point and time selection together ignoring snap"),
        MoveLoopPointsAndTimeSelectionTogether => (7, "Move loop points and time selection together"),
        MoveLoopPointsAndTimeSelectionTogetherIgnoringSnap => (8, "Move loop points and time selection together ignoring snap"),
    }
}

define_behavior_enum! {
    /// MIDI Ruler click behaviors
    pub enum MidiRulerClickBehavior {
        NoAction => (0, "No action"),
        MoveEditCursor => (1, "Move edit cursor"),
        MoveEditCursorIgnoringSnap => (2, "Move edit cursor ignoring snap"),
        SelectNotesOrCcInTimeSelection => (3, "Select notes or CC in time selection"),
        ClearLoopOrTimeSelection => (4, "Clear loop or time selection"),
    }
}

define_behavior_enum! {
    /// MIDI Ruler double click behaviors
    pub enum MidiRulerDoubleClickBehavior {
        NoAction => (0, "No action"),
        ZoomToSelectedNotesCc => (1, "Zoom to selected notes/CC"),
        FileCloseWindow => (2, "File: Close window"),
    }
}
