//! MIDI Piano Roll behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// MIDI Piano Roll left drag behaviors
    pub enum MidiPianoRollLeftDragBehavior {
        NoAction => (0, "No action"),
        InsertNoteDragToExtendOrChangePitch => (1, "Insert note, drag to extend or change pitch"),
        InsertNoteIgnoringSnapDragToExtendOrChangePitch => (2, "Insert note ignoring snap, drag to extend or change pitch"),
        EraseNotes => (3, "Erase notes"),
        SelectTime => (4, "Select time"),
        PaintNotesAndChords => (5, "Paint notes and chords"),
        SelectTimeIgnoringSnap => (6, "Select time ignoring snap"),
        MarqueeSelectNotes => (7, "Marquee select notes"),
        MarqueeToggleNoteSelection => (8, "Marquee toggle note selection"),
        MarqueeAddToNoteSelection => (9, "Marquee add to note selection"),
        MarqueeSelectNotesAndTime => (10, "Marquee select notes and time"),
        MarqueeSelectNotesAndTimeIgnoringSnap => (11, "Marquee select notes and time ignoring snap"),
        InsertNoteDragToMove => (12, "Insert note, drag to move"),
        PaintARowOfNotesOfTheSamePitch => (13, "Paint a row of notes of the same pitch"),
        InsertNoteDragToExtend => (14, "Insert note, drag to extend"),
        InsertNoteIgnoringSnapDragToExtend => (15, "Insert note ignoring snap, drag to extend"),
        ScrubPreviewMidi => (16, "Scrub preview MIDI"),
        InsertNoteIgnoringSnapDragToMove => (17, "Insert note ignoring snap, drag to move"),
        InsertNoteIgnoringSnapDragToEditVelocity => (18, "Insert note ignoring snap, drag to edit velocity"),
        InsertNoteDragToEditVelocity => (19, "Insert note, drag to edit velocity"),
        PaintAStackOfNotesOfTheSameTimePosition => (20, "Paint a stack of notes of the same time position"),
        PaintNotesIgnoringSnap => (21, "Paint notes ignoring snap"),
        PaintNotes => (22, "Paint notes"),
        PaintAStraightLineOfNotes => (23, "Paint a straight line of notes"),
        PaintAStraightLineOfNotesIgnoringSnap => (24, "Paint a straight line of notes ignoring snap"),
        SelectNotesTouchedWhileDragging => (25, "Select notes touched while dragging"),
        ToggleSelectionForNotesTouchedWhileDragging => (26, "Toggle selection for notes touched while dragging"),
        CopySelectedNotes => (27, "Copy selected notes"),
        CopySelectedNotesIgnoringSnap => (28, "Copy selected notes ignoring snap"),
        MoveSelectedNotes => (29, "Move selected notes"),
        MoveSelectedNotesIgnoringSnap => (30, "Move selected notes ignoring snap"),
        InsertNote => (31, "Insert note"),
        InsertNoteIgnoringSnap => (32, "Insert note ignoring snap"),
        InsertNoteIgnoringScaleKeyDragToMove => (33, "Insert note ignoring scale/key, drag to move"),
        InsertNoteIgnoringSnapAndScaleKeyDragToMove => (34, "Insert note ignoring snap and scale/key, drag to move"),
        InsertNoteIgnoringScaleKeyDragToExtendOrChangePitch => (35, "Insert note ignoring scale/key, drag to extend or change pitch"),
        InsertNoteIgnoringSnapAndScaleKeyDragToExtendOrChangePitch => (36, "Insert note ignoring snap and scale/key, drag to extend or change pitch"),
    }
}

define_behavior_enum! {
    /// MIDI Piano Roll click behaviors
    pub enum MidiPianoRollClickBehavior {
        NoAction => (0, "No action"),
        DeselectAllNotesAndMoveEditCursor => (1, "Deselect all notes and move edit cursor"),
        DeselectAllNotesAndMoveEditCursorIgnoringSnap => (2, "Deselect all notes and move edit cursor ignoring snap"),
        DeselectAllNotes => (3, "Deselect all notes"),
        InsertNote => (4, "Insert note"),
        InsertNoteIgnoringSnap => (5, "Insert note ignoring snap"),
        SetDrawChannelHigher => (6, "Set draw channel higher"),
        SetDrawChannelLower => (7, "Set draw channel lower"),
        InsertNoteLeavingOtherNotesSelected => (8, "Insert note, leaving other notes selected"),
        InsertNoteIgnoringSnapLeavingOtherNotesSelected => (9, "Insert note ignoring snap, leaving other notes selected"),
    }
}

define_behavior_enum! {
    /// MIDI Piano Roll double click behaviors
    pub enum MidiPianoRollDoubleClickBehavior {
        NoAction => (0, "No action"),
        InsertNote => (1, "Insert note"),
        InsertNoteIgnoringSnap => (2, "Insert note ignoring snap"),
    }
}
