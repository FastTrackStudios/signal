//! Arrange View mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Arrange View middle drag behaviors
    pub enum ArrangeMiddleDragBehavior {
        NoAction => (0, "No action"),
        ScrubAudio => (1, "Scrub audio"),
        HandScroll => (2, "Hand scroll"),
        ScrollBrowserStyle => (3, "Scroll browser-style"),
        JogAudio => (4, "Jog audio"),
        ScrubAudioLoopedSegment => (5, "Scrub audio (looped-segment mode)"),
        JogAudioLoopedSegment => (6, "Jog audio (looped-segment mode)"),
        MarqueeZoom => (7, "Marquee zoom"),
        MoveEditCursorWithoutScrubJog => (8, "Move edit cursor without scrub/jog"),
        HandScrollAndHorizontalZoom => (9, "Hand scroll and horizontal zoom"),
        HorizontalZoom => (11, "Horizontal zoom"),
        SetEditCursorAndHorizontalZoom => (13, "Set edit cursor and horizontal zoom"),
        SetEditCursorAndHandScroll => (15, "Set edit cursor and hand scroll"),
        SetEditCursorHandScrollAndHorizontalZoom => (16, "Set edit cursor, hand scroll and horizontal zoom"),
    }
}

define_behavior_enum! {
    /// Arrange View middle click behaviors
    pub enum ArrangeMiddleClickBehavior {
        NoAction => (0, "No action"),
        RestorePreviousZoomScroll => (1, "Restore previous zoom/scroll"),
        RestorePreviousZoomLevel => (2, "Restore previous zoom level"),
        MoveEditCursorIgnoringSnap => (3, "Move edit cursor ignoring snap"),
    }
}

define_behavior_enum! {
    /// Arrange View right drag behaviors
    pub enum ArrangeRightDragBehavior {
        NoAction => (1, "No Action"),
        MarqueeSelectItems => (2, "Marquee Select Items"),
        MarqueeAddToItemSelection => (3, "Marquee Add to Item Selection"),
        MarqueeToggleItemSelection => (4, "Marquee Toggle Item Selection"),
        MarqueeSelectItemsAndTime => (5, "Marquee Select Items and Time"),
        MarqueeSelectItemsAndTimeIgnoringSnap => (6, "Marquee Select Items and Time (ignoring snap)"),
    }
}
