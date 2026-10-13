//! Project mouse modifier behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Project marker lanes behaviors
    pub enum ProjectMarkerLanesBehavior {
        NoAction => (0, "No action"),
        HandScroll => (1, "Hand scroll"),
        HandScrollAndHorizontalZoom => (2, "Hand scroll and horizontal zoom"),
        HorizontalZoom => (4, "Horizontal zoom"),
        SetEditCursorAndHorizontalZoom => (6, "Set edit cursor and horizontal zoom"),
        SetEditCursorAndHandScroll => (8, "Set edit cursor and hand scroll"),
        SetEditCursorHandScrollAndHorizontalZoom => (9, "Set edit cursor, hand scroll and horizontal zoom"),
    }
}

define_behavior_enum! {
    /// Project marker/region edge behaviors
    pub enum ProjectMarkerRegionEdgeBehavior {
        NoAction => (0, "No action"),
        MoveProjectMarkerRegionEdge => (1, "Move project marker/region edge"),
        MoveProjectMarkerRegionEdgeIgnoringSnap => (2, "Move project marker/region edge ignoring snap"),
    }
}

define_behavior_enum! {
    /// Project region behaviors
    pub enum ProjectRegionBehavior {
        NoAction => (0, "No action"),
        MoveContentsOfProjectRegion => (1, "Move contents of project region"),
        MoveContentsOfProjectRegionIgnoringSnap => (2, "Move contents of project region ignoring snap"),
        CopyContentsOfProjectRegions => (3, "Copy contents of project regions"),
        CopyContentsOfProjectRegionsIgnoringSnap => (4, "Copy contents of project regions ignoring snap"),
        MoveProjectRegionsButNotContents => (5, "Move project regions but not contents"),
        MoveProjectRegionsButNotContentsIgnoringSnap => (6, "Move project regions but not contents ignoring snap"),
        CopyProjectRegionsButNotContents => (7, "Copy project regions but not contents"),
        CopyProjectRegionsButNotContentsIgnoringSnap => (8, "Copy project regions but not contents ignoring snap"),
    }
}

define_behavior_enum! {
    /// Project tempo marker behaviors
    pub enum ProjectTempoMarkerBehavior {
        NoAction => (0, "No action"),
        MoveProjectTempoTimeSignatureMarker => (1, "Move project tempo/time signature marker"),
        MoveProjectTempoTimeSignatureMarkerIgnoringSnap => (2, "Move project tempo/time signature marker ignoring snap"),
        MoveProjectTempoTimeSignatureMarkerAdjustingPreviousTempo => (3, "Move project tempo/time signature marker, adjusting previous tempo"),
        MoveProjectTempoTimeSignatureMarkerAdjustingPreviousAndCurrentTempo => (4, "Move project tempo/time signature marker, adjusting previous and current tempo"),
    }
}
