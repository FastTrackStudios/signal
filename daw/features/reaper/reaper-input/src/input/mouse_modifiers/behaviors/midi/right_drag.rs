//! MIDI Right Drag behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// MIDI Right Drag behaviors
    pub enum MidiRightDragBehavior {
        NoAction => (0, "No action"),
        MarqueeSelectNotesCc => (1, "Marquee select notes/CC"),
        MarqueeSelectNotesCcAndTime => (2, "Marquee select notes/CC and time"),
        MarqueeSelectNotesCcAndTimeIgnoringSnap => (3, "Marquee select notes/CC and time ignoring snap"),
        SelectTime => (4, "Select time"),
        SelectTimeIgnoringSnap => (5, "Select time ignoring snap"),
        EraseNotesCc => (6, "Erase notes/CC"),
        MarqueeToggleNoteCcSelection => (7, "Marquee toggle note/CC selection"),
        MarqueeAddToNotesCcSelection => (8, "Marquee add to notes/CC selection"),
        HandScroll => (9, "Hand scroll"),
        EraseNotesCcImmediatelySuppressesRightClickContextMenu => (10, "Erase notes/CC immediately (suppresses right-click context menu)"),
        SelectNotesTouchedWhileDragging => (11, "Select notes touched while dragging"),
        ToggleSelectionForNotesTouchedWhileDragging => (12, "Toggle selection for notes touched while dragging"),
    }
}
