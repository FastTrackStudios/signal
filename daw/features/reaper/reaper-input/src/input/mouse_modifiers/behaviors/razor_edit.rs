//! Razor Edit mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Razor Edit area left drag behaviors
    pub enum RazorEditAreaLeftDragBehavior {
        NoAction => (0, "No action"),
        MoveAreas => (1, "Move areas"),
        MoveAreasIgnoringSnap => (2, "Move areas ignoring snap"),
        CopyAreas => (3, "Copy areas"),
        CopyAreasIgnoringSnap => (4, "Copy areas ignoring snap"),
        MoveAreasWithoutContents => (5, "Move areas without contents"),
        MoveAreasWithoutContentsIgnoringSnap => (6, "Move areas without contents ignoring snap"),
        MoveAreasVertically => (7, "Move areas vertically"),
        MoveAreasOnOneAxisOnly => (8, "Move areas on one axis only"),
        CopyAreasVertically => (9, "Copy areas vertically"),
        MoveAreasHorizontally => (10, "Move areas horizontally"),
        MoveAreasOnOneAxisOnlyIgnoringSnap => (11, "Move areas on one axis only ignoring snap"),
        CopyAreasHorizontally => (12, "Copy areas horizontally"),
        MoveAreasHorizontallyIgnoringSnap => (13, "Move areas horizontally ignoring snap"),
        CopyAreasHorizontallyIgnoringSnap => (14, "Copy areas horizontally ignoring snap"),
        CopyAreasOnOneAxisOnly => (15, "Copy areas on one axis only"),
        CopyAreasOnOneAxisOnlyIgnoringSnap => (16, "Copy areas on one axis only ignoring snap"),
    }
}

define_behavior_enum! {
    /// Razor Edit area click behaviors
    pub enum RazorEditAreaClickBehavior {
        NoAction => (0, "No action"),
        RemoveOneArea => (1, "Remove one area"),
        DeleteAreasContents => (2, "Delete areas contents"),
        RemoveAreas => (3, "Remove areas"),
        SplitMediaItemsAtAreaEdges => (4, "Split media items at area edges"),
        MoveAreasBackwards => (5, "Move areas backwards"),
        MoveAreasForwards => (6, "Move areas forwards"),
        MoveAreasUpWithoutContents => (7, "Move areas up without contents"),
        MoveAreasDownWithoutContents => (8, "Move areas down without contents"),
    }
}

define_behavior_enum! {
    /// Razor Edit edge behaviors
    pub enum RazorEditEdgeBehavior {
        NoAction => (0, "No action"),
        MoveEdges => (1, "Move edges"),
        MoveEdgesIgnoringSnap => (2, "Move edges ignoring snap"),
        StretchAreas => (3, "Stretch areas"),
        StretchAreasIgnoringSnap => (4, "Stretch areas ignoring snap"),
    }
}

define_behavior_enum! {
    /// Razor Edit envelope area behaviors
    pub enum RazorEditEnvelopeAreaBehavior {
        NoAction => (0, "No action"),
        MoveOrTiltEnvelopeVertically => (1, "Move or tilt envelope vertically"),
        ExpandOrCompressEnvelopeRange => (2, "Expand or compress envelope range"),
        ExpandOrCompressEnvelopeRangeTowardTopBottom => (3, "Expand or compress envelope range toward top/bottom"),
        MoveOrTiltEnvelopeVerticallyFine => (4, "Move or tilt envelope vertically (fine)"),
    }
}
