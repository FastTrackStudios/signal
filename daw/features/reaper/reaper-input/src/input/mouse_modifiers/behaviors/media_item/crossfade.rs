//! Media Item crossfade behaviors

#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    /// Media Item crossfade left drag behaviors
    pub enum MediaItemCrossfadeLeftDragBehavior {
        NoAction => (0, "No action"),
        MoveBothFadesIgnoringSnap => (1, "Move both fades ignoring snap"),
        MoveBothFadesIgnoringSnapAndSelectionGrouping => (2, "Move both fades ignoring snap and selection/grouping"),
        MoveBothFadesIgnoringSnapRelativeEdgeEdit => (3, "Move both fades ignoring snap (relative edge edit)"),
        MoveBothFadesAndStretchItemsIgnoringSnap => (4, "Move both fades and stretch items ignoring snap"),
        MoveBothFadesAndStretchItemsIgnoringSnapAndSelectionGrouping => (5, "Move both fades and stretch items ignoring snap and selection/grouping"),
        MoveBothFadesAndStretchItemsIgnoringSnapRelativeEdgeEdit => (6, "Move both fades and stretch items ignoring snap (relative edge edit)"),
        AdjustBothFadeCurvesHorizontally => (7, "Adjust both fade curves horizontally"),
        AdjustBothFadeCurvesHorizontallyIgnoringSelectionGrouping => (8, "Adjust both fade curves horizontally ignoring selection/grouping"),
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSnap => (9, "Adjust length of both fades preserving intersection ignoring snap"),
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSnapAndSelectionGrouping => (10, "Adjust length of both fades preserving intersection ignoring snap and selection/grouping"),
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSnapRelativeEdgeEdit => (11, "Adjust length of both fades preserving intersection ignoring snap (relative edge edit)"),
        AdjustBothFadeCurvesHorizontallyAndVertically => (12, "Adjust both fade curves horizontally and vertically"),
        AdjustBothFadeCurvesHorizontallyAndVerticallyIgnoringSelectionGrouping => (13, "Adjust both fade curves horizontally and vertically ignoring selection/grouping"),
        MoveBothFades => (14, "Move both fades"),
        MoveBothFadesIgnoringSelectionGrouping => (15, "Move both fades ignoring selection/grouping"),
        MoveBothFadesRelativeEdgeEdit => (16, "Move both fades (relative edge edit)"),
        MoveBothFadesAndStretchItems => (17, "Move both fades and stretch items"),
        MoveBothFadesAndStretchItemsIgnoringSelectionGrouping => (18, "Move both fades and stretch items ignoring selection/grouping"),
        MoveBothFadesAndStretchItemsRelativeEdgeEdit => (19, "Move both fades and stretch items (relative edge edit)"),
        AdjustLengthOfBothFadesPreservingIntersection => (20, "Adjust length of both fades preserving intersection"),
        AdjustLengthOfBothFadesPreservingIntersectionIgnoringSelectionGrouping => (21, "Adjust length of both fades preserving intersection ignoring selection/grouping"),
        AdjustLengthOfBothFadesPreservingIntersectionRelativeEdgeEdit => (22, "Adjust length of both fades preserving intersection (relative edge edit)"),
    }
}

define_behavior_enum! {
    /// Media Item crossfade click behaviors
    pub enum MediaItemCrossfadeClickBehavior {
        NoAction => (0, "No action"),
        SetBothFadesToNextShape => (1, "Set both fades to next shape"),
        SetBothFadesToPreviousShape => (2, "Set both fades to previous shape"),
        SetBothFadesToNextShapeIgnoringSelection => (3, "Set both fades to next shape ignoring selection"),
        SetBothFadesToPreviousShapeIgnoringSelection => (4, "Set both fades to previous shape ignoring selection"),
        OpenCrossfadeEditor => (5, "Open crossfade editor"),
    }
}

define_behavior_enum! {
    /// Media Item crossfade double click behaviors
    pub enum MediaItemCrossfadeDoubleClickBehavior {
        NoAction => (0, "No action"),
        OpenCrossfadeEditor => (1, "Open crossfade editor"),
        ResetToDefaultCrossfade => (2, "Reset to default crossfade"),
    }
}
