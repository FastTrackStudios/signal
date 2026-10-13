//! Track mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Track left drag behaviors
    pub enum TrackLeftDragBehavior {
        NoAction => (0, "No action"),
        DrawACopyOfTheSelectedMediaItem => (1, "Draw a copy of the selected media item"),
        DrawACopyOfTheSelectedMediaItemIgnoringSnap => (2, "Draw a copy of the selected media item ignoring snap"),
        DrawACopyOfTheSelectedMediaItemOnTheSameTrack => (3, "Draw a copy of the selected media item on the same track"),
        DrawACopyOfTheSelectedMediaItemOnTheSameTrackIgnoringSnap => (4, "Draw a copy of the selected media item on the same track ignoring snap"),
        DrawAnEmptyMidiItem => (5, "Draw an empty MIDI item"),
        DrawAnEmptyMidiItemIgnoringSnap => (6, "Draw an empty MIDI item ignoring snap"),
        SelectTime => (7, "Select time"),
        SelectTimeIgnoringSnap => (8, "Select time ignoring snap"),
        MarqueeSelectItems => (9, "Marquee select items"),
        MarqueeSelectItemsAndTime => (10, "Marquee select items and time"),
        MarqueeSelectItemsAndTimeIgnoringSnap => (11, "Marquee select items and time ignoring snap"),
        MarqueeToggleItemSelection => (12, "Marquee toggle item selection"),
        MarqueeAddToItemSelection => (13, "Marquee add to item selection"),
        MoveTimeSelection => (14, "Move time selection"),
        MoveTimeSelectionIgnoringSnap => (15, "Move time selection ignoring snap"),
        DrawACopyOfTheSelectedMediaItemPoolingMidiSourceData => (16, "Draw a copy of the selected media item, pooling MIDI source data"),
        DrawACopyOfTheSelectedMediaItemIgnoringSnapPoolingMidiSourceData => (17, "Draw a copy of the selected media item ignoring snap, pooling MIDI source data"),
        DrawACopyOfTheSelectedMediaItemOnTheSameTrackPoolingMidiSourceData => (18, "Draw a copy of the selected media item on the same track, pooling MIDI source data"),
        DrawACopyOfTheSelectedMediaItemOnTheSameTrackIgnoringSnapPoolingMidiSourceData => (19, "Draw a copy of the selected media item on the same track ignoring snap, pooling MIDI source data"),
        EditLoopPoints => (20, "Edit loop points"),
        EditLoopPointsIgnoringSnap => (21, "Edit loop points ignoring snap"),
        MarqueeZoom => (22, "Marquee zoom"),
    }
}

define_behavior_enum! {
    /// Track click behaviors
    pub enum TrackClickBehavior {
        NoAction => (0, "No action"),
        DeselectAllItemsAndMoveEditCursor => (1, "Deselect all items and move edit cursor"),
        DeselectAllItemsAndMoveEditCursorIgnoringSnap => (2, "Deselect all items and move edit cursor ignoring snap"),
        DeselectAllItems => (3, "Deselect all items"),
        ClearTimeSelection => (4, "Clear time selection"),
        ExtendTimeSelection => (5, "Extend time selection"),
        ExtendTimeSelectionIgnoringSnap => (6, "Extend time selection ignoring snap"),
        RestorePreviousZoomScroll => (7, "Restore previous zoom/scroll"),
        RestorePreviousZoomLevel => (8, "Restore previous zoom level"),
        SelectRazorEditArea => (25, "Select razor edit area"),
        SelectRazorEditAreaIgnoringSnap => (26, "Select razor edit area ignoring snap"),
        AddToRazorEditArea => (27, "Add to razor edit area"),
        AddToRazorEditAreaIgnoringSnap => (28, "Add to razor edit area ignoring snap"),
        SelectRazorEditAreaAndTime => (29, "Select razor edit area and time"),
        SelectRazorEditAreaAndTimeIgnoringSnap => (30, "Select razor edit area and time ignoring snap"),
    }
}

define_behavior_enum! {
    /// Track double click behaviors
    pub enum TrackDoubleClickBehavior {
        NoAction => (0, "No action"),
    }
}

define_behavior_enum! {
    /// Track control panel double click behaviors
    pub enum TrackControlPanelDoubleClickBehavior {
        NoAction => (0, "No action"),
        SelectAllMediaItemsOnTrack => (1, "Select all media items on track"),
        ZoomViewToTrack => (2, "Zoom view to track"),
        ToggleSelectionForAllMediaItemsOnTrack => (3, "Toggle selection for all media items on track"),
        AddAllMediaItemsOnTrackToSelection => (4, "Add all media items on track to selection"),
        RestorePreviousZoomScroll => (5, "Restore previous zoom/scroll"),
        RestorePreviousZoomLevel => (6, "Restore previous zoom level"),
    }
}
