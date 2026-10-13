//! MIDI Note behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// MIDI Note left drag behaviors
    pub enum MidiNoteLeftDragBehavior {
        NoAction => (0, "No action"),
        MoveNote => (1, "Move note"),
        MoveNoteIgnoringSnap => (2, "Move note ignoring snap"),
        EraseNotes => (3, "Erase notes"),
        SelectTime => (4, "Select time"),
        MoveNoteOnOneAxisOnly => (5, "Move note on one axis only"),
        MoveNoteOnOneAxisOnlyIgnoringSnap => (6, "Move note on one axis only ignoring snap"),
        CopyNote => (7, "Copy note"),
        CopyNoteIgnoringSnap => (8, "Copy note ignoring snap"),
        EditNoteVelocity => (9, "Edit note velocity"),
        EditNoteVelocityFine => (10, "Edit note velocity (fine)"),
        MoveNoteHorizontally => (11, "Move note horizontally"),
        MoveNoteHorizontallyIgnoringSnap => (12, "Move note horizontally ignoring snap"),
        MoveNoteVertically => (13, "Move note vertically"),
        SelectTimeIgnoringSnap => (14, "Select time ignoring snap"),
        MarqueeSelectNotes => (15, "Marquee select notes"),
        MarqueeToggleNoteSelection => (16, "Marquee toggle note selection"),
        MarqueeAddToNoteSelection => (17, "Marquee add to note selection"),
        MarqueeSelectNotesAndTime => (18, "Marquee select notes and time"),
        MarqueeSelectNotesAndTimeIgnoringSnap => (19, "Marquee select notes and time ignoring snap"),
        StretchNotePositionsIgnoringSnapArpeggiate => (20, "Stretch note positions ignoring snap (arpeggiate)"),
        StretchNoteSelectionVerticallyArpeggiate => (21, "Stretch note selection vertically (arpeggiate)"),
        StretchNoteSelectionVerticallyArpeggiateDuplicate => (22, "Stretch note selection vertically (arpeggiate)"),
        StretchNoteLengthsIgnoringSnapArpeggiatorLegato => (23, "Stretch note lengths ignoring snap (arpeggiator legato)"),
        StretchNoteLengthsArpeggiateLegato => (24, "Stretch note lengths (arpeggiate legato)"),
        CopyNoteHorizontally => (25, "Copy note horizontally"),
        CopyNoteHorizontallyIgnoringSnap => (26, "Copy note horizontally ignoring snap"),
        CopyNoteVertically => (27, "Copy note vertically"),
        SelectNotesTouchedWhileDragging => (28, "Select notes touched while dragging"),
        ToggleSelectionForNotesTouchedWhileDragging => (29, "Toggle selection for notes touched while dragging"),
        MoveNoteIgnoringSelection => (30, "Move note ignoring selection"),
        MoveNoteIgnoringSnapAndSelection => (31, "Move note ignoring snap and selection"),
        MoveNoteVerticallyIgnoringScaleKey => (32, "Move note vertically ignoring scale/key"),
    }
}

define_behavior_enum! {
    /// MIDI Note click behaviors
    pub enum MidiNoteClickBehavior {
        NoAction => (0, "No action"),
        SelectNote => (1, "Select note"),
        SelectNoteAndMoveEditCursor => (2, "Select note and move edit cursor"),
        SelectNoteAndMoveEditCursorIgnoringSnap => (3, "Select note and move edit cursor ignoring snap"),
        ToggleNoteSelection => (4, "Toggle note selection"),
        AddNoteToSelection => (5, "Add note to selection"),
        EraseNote => (6, "Erase note"),
        ToggleNoteMute => (7, "Toggle note mute"),
        SetNoteChannelHigher => (8, "Set note channel higher"),
        SetNoteChannelLower => (9, "Set note channel lower"),
        DoubleNoteLength => (10, "Double note length"),
        HalveNoteLength => (11, "Halve note length"),
        SelectNoteAndAllLaterNotes => (12, "Select note and all later notes"),
        AddNoteAndAllLaterNotesToSelection => (13, "Add note and all later notes to selection"),
        SelectNoteAndAllLaterNotesOfSamePitch => (14, "Select note and all later notes of same pitch"),
        AddNoteAndAllLaterNotesOfSamePitchToSelection => (15, "Add note and all later notes of same pitch to selection"),
        SelectAllNotesInMeasure => (16, "Select all notes in measure"),
        AddAllNotesInMeasureToSelection => (17, "Add all notes in measure to selection"),
    }
}

define_behavior_enum! {
    /// MIDI Note double click behaviors
    pub enum MidiNoteDoubleClickBehavior {
        NoAction => (0, "No action"),
        EraseNote => (1, "Erase note"),
        ActivateMidiItemWhenClickingANoteThatIsNotInTheActiveItem => (2, "Activate MIDI item (when clicking a note that is not in the active item)"),
    }
}

define_behavior_enum! {
    /// MIDI Note edge behaviors
    pub enum MidiNoteEdgeBehavior {
        NoAction => (0, "No action"),
        MoveNoteEdge => (1, "Move note edge"),
        MoveNoteEdgeIgnoringSnap => (2, "Move note edge ignoring snap"),
        StretchNotes => (3, "Stretch notes"),
        StretchNotesIgnoringSnap => (4, "Stretch notes ignoring snap"),
        MoveNoteEdgeIgnoringSelection => (5, "Move note edge ignoring selection"),
        MoveNoteEdgeIgnoringSnapAndSelection => (6, "Move note edge ignoring snap and selection"),
    }
}
