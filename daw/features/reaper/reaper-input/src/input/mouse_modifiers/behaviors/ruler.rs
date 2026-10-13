//! Ruler mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Ruler left drag behaviors
    pub enum RulerLeftDragBehavior {
        NoAction => (0, "No action"),
        EditLoopPoint => (1, "Edit loop point"),
        MoveLoopPoints => (3, "Move loop points"),
        MoveLoopPointsIgnoringSnap => (4, "Move loop points ignoring snap"),
        EditLoopPointAndTimeSelectionTogether => (5, "Edit loop point and time selection together"),
        MoveLoopPointsAndTimeSelectionTogether => (7, "Move loop points and time selection together"),
        MoveLoopPointsAndTimeSelectionTogetherIgnoringSnap => (8, "Move loop points and time selection together ignoring snap"),
        HandScroll => (9, "Hand scroll"),
        HandScrollAndHorizontalZoom => (10, "Hand scroll and horizontal zoom"),
        HorizontalZoom => (12, "Horizontal zoom"),
        SetEditCursorAndHorizontalZoom => (14, "Set edit cursor and horizontal zoom"),
    }
}

define_behavior_enum! {
    /// Ruler click behaviors
    pub enum RulerClickBehavior {
        NoAction => (0, "No action"),
        MoveEditCursor => (1, "Move edit cursor"),
        MoveEditCursorIgnoringSnap => (2, "Move edit cursor ignoring snap"),
        ClearLoopPoints => (3, "Clear loop points"),
        ExtendLoopPoints => (4, "Extend loop points"),
        ExtendLoopPointsIgnoringSnap => (5, "Extend loop points ignoring snap"),
        SeekPlaybackWithoutMovingEditCursor => (6, "Seek playback without moving edit cursor"),
        RestorePreviousZoomScroll => (7, "Restore previous zoom/scroll"),
        RestorePreviousZoomLevel => (8, "Restore previous zoom level"),
    }
}

define_behavior_enum! {
    /// Ruler double click behaviors
    pub enum RulerDoubleClickBehavior {
        NoAction => (0, "No action"),
        SetLoopPointsToRegion => (1, "Set loop points to region"),
        SetTimeSelectionToRegion => (2, "Set time selection to region"),
        SetLoopPointsAndTimeSelectionToRegion => (3, "Set loop points and time selection to region"),
        RestorePreviousZoomScroll => (4, "Restore previous zoom/scroll"),
        RestorePreviousZoomLevel => (5, "Restore previous zoom level"),
    }
}
