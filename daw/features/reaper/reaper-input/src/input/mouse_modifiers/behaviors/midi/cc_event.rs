//! MIDI CC Event behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// MIDI CC Event left drag behaviors
    pub enum MidiCcEventLeftDragBehavior {
        NoAction => (0, "No action"),
        MoveCcEvent => (1, "Move CC event"),
        MoveCcEventIgnoringSnap => (2, "Move CC event ignoring snap"),
        EraseCcEvent => (3, "Erase CC event"),
        MoveCcEventOnOneAxisOnly => (4, "Move CC event on one axis only"),
        MoveCcEventOnOneAxisOnlyIgnoringSnap => (5, "Move CC event on one axis only ignoring snap"),
        CopyCcEvent => (6, "Copy CC event"),
        CopyCcEventIgnoringSnap => (7, "Copy CC event ignoring snap"),
        MoveCcHorizontally => (8, "Move CC horizontally"),
        MoveCcHorizontallyIgnoringSnap => (9, "Move CC horizontally ignoring snap"),
        MoveCcVertically => (10, "Move CC vertically"),
        MarqueeSelectCc => (11, "Marquee select CC"),
        MarqueeToggleCcSelection => (12, "Marquee toggle CC selection"),
        MarqueeAddToCcSelection => (13, "Marquee add to CC selection"),
        MarqueeSelectCcAndTime => (14, "Marquee select CC and time"),
        MarqueeSelectCcAndTimeIgnoringSnap => (15, "Marquee select CC and time ignoring snap"),
        SelectTime => (16, "Select time"),
        SelectTimeIgnoringSnap => (17, "Select time ignoring snap"),
        DrawEditCcEventsIgnoringSelection => (18, "Draw/edit CC events ignoring selection"),
        LinearRampCcEventsIgnoringSelection => (19, "Linear ramp CC events ignoring selection"),
        DrawEditCcEventsIgnoringSnapAndSelection => (20, "Draw/edit CC events ignoring snap and selection"),
        EditSelectedCcEventsIfAnyOtherwiseDrawEditIgnoringSnap => (21, "Edit selected CC events if any, otherwise draw/edit ignoring snap"),
        EditCcEventsIgnoringSelection => (22, "Edit CC events ignoring selection"),
        EditCcEvents => (23, "Edit CC events"),
        LinearRampCcEvents => (24, "Linear ramp CC events"),
    }
}

define_behavior_enum! {
    /// MIDI CC Event double click behaviors
    pub enum MidiCcEventDoubleClickBehavior {
        NoAction => (0, "No action"),
        InsertCcEvent => (1, "Insert CC event"),
        InsertCcEventIgnoringSnap => (2, "Insert CC event ignoring snap"),
    }
}
