//! MIDI CC Lane behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// MIDI CC Lane left drag behaviors
    pub enum MidiCcLaneLeftDragBehavior {
        NoAction => (0, "No action"),
        DrawEditCcEventsIgnoringSelection => (1, "Draw/edit CC events ignoring selection"),
        EditSelectedCcEventsIfAnyOtherwiseDrawEdit => (2, "Edit selected CC events if any, otherwise draw/edit"),
        EraseCcEvents => (3, "Erase CC events"),
        LinearRampCcEventsIgnoringSelection => (4, "Linear ramp CC events ignoring selection"),
        LinearRampCcEvents => (5, "Linear ramp CC events"),
        DrawEditCcEventsIgnoringSnapAndSelection => (6, "Draw/edit CC events ignoring snap and selection"),
        EditCcEventsIgnoringSelection => (7, "Edit CC events ignoring selection"),
        EditSelectedCcEventsIfAnyOtherwiseDrawEditIgnoringSnap => (8, "Edit selected CC events if any, otherwise draw/edit ignoring snap"),
        MarqueeSelectCc => (9, "Marquee select CC"),
        MarqueeToggleCcSelection => (10, "Marquee toggle CC selection"),
        MarqueeAddToCcSelection => (11, "Marquee add to CC selection"),
        MarqueeSelectCcAndTime => (12, "Marquee select CC and time"),
        MarqueeSelectCcAndTimeIgnoringSnap => (13, "Marquee select CC and time ignoring snap"),
        SelectTime => (14, "Select time"),
        SelectTimeIgnoringSnap => (15, "Select time ignoring snap"),
        EditCcEvents => (16, "Edit CC events"),
    }
}

define_behavior_enum! {
    /// MIDI CC Lane double click behaviors
    pub enum MidiCcLaneDoubleClickBehavior {
        NoAction => (0, "No action"),
        InsertCcEvent => (1, "Insert CC event"),
        InsertCcEventIgnoringSnap => (2, "Insert CC event ignoring snap"),
    }
}
