//! Media Item fade behaviors

use crate::input::mouse_modifiers::behaviors::shared::traits::{BehaviorDisplay, BehaviorId};

/// Media Item fade left drag behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemFadeLeftDragBehavior {
    MoveFadeIgnoringSnap,
    MoveFadeIgnoringSnapDuplicate,
    MoveCrossfadeIgnoringSnap,
    MoveFadeAndStretchCrossfadedItemsIgnoringSnap,
    MoveCrossfadeAndStretchCrossfadedItemsIgnoringSnap,
    MoveFadeIgnoringSnapAndSelectionGrouping,
    MoveCrossfadeIgnoringSnapAndSelectionGrouping,
    MoveFadeAndStretchCrossfadedItemsIgnoringSnapAndSelectionGrouping,
    MoveCrossfadeAndStretchItemsIgnoringSnapAndSelectionGrouping,
    MoveFadeIgnoringSnapRelativeEdgeEdit,
    MoveCrossfadeIgnoringSnapRelativeEdgeEdit,
    MoveFadeAndStretchCrossfadedItemsIgnoringSnapRelativeEdgeEdit,
    MoveCrossfadeAndStretchItemsIgnoringSnapRelativeEdgeEdit,
    AdjustFadeCurve,
    AdjustFadeCurveIgnoringSelectionGrouping,
    AdjustFadeCurveIgnoringCrossfadedItems,
    AdjustFadeCurveIgnoringCrossfadedItemsAndSelectionGrouping,
    MoveFadeIgnoringSnapAndCrossfadedItem,
    MoveFadeIgnoringSnapSelectionGroupingAndCrossfadedItems,
    MoveFadeIgnoringSnapAndCrossfadedItemsRelativeEdgeEdit,
    MoveFade,
    MoveCrossfade,
    MoveFadeAndStretchCrossfadedItems,
    MoveCrossfadeAndStretchItems,
    MoveFadeIgnoringSelectionGrouping,
    MoveCrossfadeIgnoringSelectionGrouping,
    MoveFadeAndStretchCrossfadedItemsIgnoringSelectionGrouping,
    MoveCrossfadeAndStretchItemsIgnoringSelectionGrouping,
    MoveFadeRelativeEdgeEdit,
    MoveCrossfadeRelativeEdgeEdit,
    MoveFadeAndStretchCrossfadedItemsRelativeEdgeEdit,
    MoveCrossfadeAndStretchItemsRelativeEdgeEdit,
    MoveFadeIgnoringCrossfadedItems,
    MoveFadeIgnoringCrossfadedItemsAndSelectionGrouping,
    MoveFadeIgnoringCrossfadedItemsRelativeEdgeEdit,
    Unknown(u32),
}

impl BehaviorId for MediaItemFadeLeftDragBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnap,
            1 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapDuplicate,
            2 => MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnap,
            3 => MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnap,
            4 => MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchCrossfadedItemsIgnoringSnap,
            5 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndSelectionGrouping,
            6 => MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnapAndSelectionGrouping,
            7 => MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnapAndSelectionGrouping,
            8 => MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSnapAndSelectionGrouping,
            9 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapRelativeEdgeEdit,
            10 => MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnapRelativeEdgeEdit,
            11 => MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnapRelativeEdgeEdit,
            12 => MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSnapRelativeEdgeEdit,
            13 => MediaItemFadeLeftDragBehavior::AdjustFadeCurve,
            14 => MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringSelectionGrouping,
            15 => MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringCrossfadedItems,
            16 => MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringCrossfadedItemsAndSelectionGrouping,
            17 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndCrossfadedItem,
            18 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapSelectionGroupingAndCrossfadedItems,
            19 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndCrossfadedItemsRelativeEdgeEdit,
            20 => MediaItemFadeLeftDragBehavior::MoveFade,
            21 => MediaItemFadeLeftDragBehavior::MoveCrossfade,
            22 => MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItems,
            23 => MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItems,
            24 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSelectionGrouping,
            25 => MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSelectionGrouping,
            26 => MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSelectionGrouping,
            27 => MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSelectionGrouping,
            28 => MediaItemFadeLeftDragBehavior::MoveFadeRelativeEdgeEdit,
            29 => MediaItemFadeLeftDragBehavior::MoveCrossfadeRelativeEdgeEdit,
            30 => MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsRelativeEdgeEdit,
            31 => MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsRelativeEdgeEdit,
            32 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItems,
            33 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItemsAndSelectionGrouping,
            34 => MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItemsRelativeEdgeEdit,
            id => MediaItemFadeLeftDragBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnap => 0,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapDuplicate => 1,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnap => 2,
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnap => 3,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchCrossfadedItemsIgnoringSnap => 4,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndSelectionGrouping => 5,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnapAndSelectionGrouping => 6,
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnapAndSelectionGrouping => 7,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSnapAndSelectionGrouping => 8,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapRelativeEdgeEdit => 9,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnapRelativeEdgeEdit => 10,
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnapRelativeEdgeEdit => 11,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSnapRelativeEdgeEdit => 12,
            MediaItemFadeLeftDragBehavior::AdjustFadeCurve => 13,
            MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringSelectionGrouping => 14,
            MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringCrossfadedItems => 15,
            MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringCrossfadedItemsAndSelectionGrouping => 16,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndCrossfadedItem => 17,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapSelectionGroupingAndCrossfadedItems => 18,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndCrossfadedItemsRelativeEdgeEdit => 19,
            MediaItemFadeLeftDragBehavior::MoveFade => 20,
            MediaItemFadeLeftDragBehavior::MoveCrossfade => 21,
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItems => 22,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItems => 23,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSelectionGrouping => 24,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSelectionGrouping => 25,
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSelectionGrouping => 26,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSelectionGrouping => 27,
            MediaItemFadeLeftDragBehavior::MoveFadeRelativeEdgeEdit => 28,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeRelativeEdgeEdit => 29,
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsRelativeEdgeEdit => 30,
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsRelativeEdgeEdit => 31,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItems => 32,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItemsAndSelectionGrouping => 33,
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItemsRelativeEdgeEdit => 34,
            MediaItemFadeLeftDragBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemFadeLeftDragBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnap => "Move fade ignoring snap",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapDuplicate => "Move fade ignoring snap",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnap => "Move crossfade ignoring snap",
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnap => "Move fade and stretch crossfaded items ignoring snap",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchCrossfadedItemsIgnoringSnap => "Move crossfade and stretch crossfaded items ignoring snap",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndSelectionGrouping => "Move fade ignoring snap and selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnapAndSelectionGrouping => "Move crossfade ignoring snap and selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnapAndSelectionGrouping => "Move fade and stretch crossfaded items ignoring snap and selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSnapAndSelectionGrouping => "Move crossfade and stretch items ignoring snap and selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapRelativeEdgeEdit => "Move fade ignoring snap (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSnapRelativeEdgeEdit => "Move crossfade ignoring snap (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSnapRelativeEdgeEdit => "Move fade and stretch crossfaded items ignoring snap (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSnapRelativeEdgeEdit => "Move crossfade and stretch items ignoring snap (relative edge edit)",
            MediaItemFadeLeftDragBehavior::AdjustFadeCurve => "Adjust fade curve",
            MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringSelectionGrouping => "Adjust fade curve ignoring selection/grouping",
            MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringCrossfadedItems => "Adjust fade curve ignoring crossfaded items",
            MediaItemFadeLeftDragBehavior::AdjustFadeCurveIgnoringCrossfadedItemsAndSelectionGrouping => "Adjust fade curve ignoring crossfaded items and selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndCrossfadedItem => "Move fade ignoring snap and crossfaded item",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapSelectionGroupingAndCrossfadedItems => "Move fade ignoring snap, selection/grouping, and crossfaded items",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSnapAndCrossfadedItemsRelativeEdgeEdit => "Move fade ignoring snap and crossfaded items (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveFade => "Move fade",
            MediaItemFadeLeftDragBehavior::MoveCrossfade => "Move crossfade",
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItems => "Move fade and stretch crossfaded items",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItems => "Move crossfade and stretch items",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringSelectionGrouping => "Move fade ignoring selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeIgnoringSelectionGrouping => "Move crossfade ignoring selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsIgnoringSelectionGrouping => "Move fade and stretch crossfaded items ignoring selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsIgnoringSelectionGrouping => "Move crossfade and stretch items ignoring selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveFadeRelativeEdgeEdit => "Move fade (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeRelativeEdgeEdit => "Move crossfade (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveFadeAndStretchCrossfadedItemsRelativeEdgeEdit => "Move fade and stretch crossfaded items (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveCrossfadeAndStretchItemsRelativeEdgeEdit => "Move crossfade and stretch items (relative edge edit)",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItems => "Move fade ignoring crossfaded items",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItemsAndSelectionGrouping => "Move fade ignoring crossfaded items and selection/grouping",
            MediaItemFadeLeftDragBehavior::MoveFadeIgnoringCrossfadedItemsRelativeEdgeEdit => "Move fade ignoring crossfaded items (relative edge edit)",
            MediaItemFadeLeftDragBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}

/// Media Item fade click behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemFadeClickBehavior {
    NoAction,
    DeleteFadeCrossfade,
    DeleteFadeCrossfadeIgnoringSelection,
    SetFadeCrossfadeToNextShape,
    SetFadeCrossfadeToPreviousShape,
    SetFadeCrossfadeToPreviousShapeIgnoringSelection,
    SetFadeCrossfadeToNextShapeIgnoringSelection,
    OpenCrossfadeEditor,
    Unknown(u32),
}

impl BehaviorId for MediaItemFadeClickBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemFadeClickBehavior::NoAction,
            1 => MediaItemFadeClickBehavior::DeleteFadeCrossfade,
            2 => MediaItemFadeClickBehavior::DeleteFadeCrossfadeIgnoringSelection,
            3 => MediaItemFadeClickBehavior::SetFadeCrossfadeToNextShape,
            4 => MediaItemFadeClickBehavior::SetFadeCrossfadeToPreviousShape,
            5 => MediaItemFadeClickBehavior::SetFadeCrossfadeToPreviousShapeIgnoringSelection,
            6 => MediaItemFadeClickBehavior::SetFadeCrossfadeToNextShapeIgnoringSelection,
            7 => MediaItemFadeClickBehavior::OpenCrossfadeEditor,
            id => MediaItemFadeClickBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemFadeClickBehavior::NoAction => 0,
            MediaItemFadeClickBehavior::DeleteFadeCrossfade => 1,
            MediaItemFadeClickBehavior::DeleteFadeCrossfadeIgnoringSelection => 2,
            MediaItemFadeClickBehavior::SetFadeCrossfadeToNextShape => 3,
            MediaItemFadeClickBehavior::SetFadeCrossfadeToPreviousShape => 4,
            MediaItemFadeClickBehavior::SetFadeCrossfadeToPreviousShapeIgnoringSelection => 5,
            MediaItemFadeClickBehavior::SetFadeCrossfadeToNextShapeIgnoringSelection => 6,
            MediaItemFadeClickBehavior::OpenCrossfadeEditor => 7,
            MediaItemFadeClickBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemFadeClickBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemFadeClickBehavior::NoAction => "No action",
            MediaItemFadeClickBehavior::DeleteFadeCrossfade => "Delete fade/crossfade",
            MediaItemFadeClickBehavior::DeleteFadeCrossfadeIgnoringSelection => {
                "Delete fade/crossfade ignoring selection"
            }
            MediaItemFadeClickBehavior::SetFadeCrossfadeToNextShape => {
                "Set fade/crossfade to next shape"
            }
            MediaItemFadeClickBehavior::SetFadeCrossfadeToPreviousShape => {
                "Set fade/crossfade to previous shape"
            }
            MediaItemFadeClickBehavior::SetFadeCrossfadeToPreviousShapeIgnoringSelection => {
                "Set fade/crossfade to previous shape ignoring selection"
            }
            MediaItemFadeClickBehavior::SetFadeCrossfadeToNextShapeIgnoringSelection => {
                "Set fade/crossfade to next shape ignoring selection"
            }
            MediaItemFadeClickBehavior::OpenCrossfadeEditor => "Open crossfade editor",
            MediaItemFadeClickBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}

/// Media Item fade double click behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemFadeDoubleClickBehavior {
    NoAction,
    DeleteItemFadeCrossfade,
    OpenCrossfadeEditor,
    Unknown(u32),
}

impl BehaviorId for MediaItemFadeDoubleClickBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemFadeDoubleClickBehavior::NoAction,
            1 => MediaItemFadeDoubleClickBehavior::DeleteItemFadeCrossfade,
            2 => MediaItemFadeDoubleClickBehavior::OpenCrossfadeEditor,
            id => MediaItemFadeDoubleClickBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemFadeDoubleClickBehavior::NoAction => 0,
            MediaItemFadeDoubleClickBehavior::DeleteItemFadeCrossfade => 1,
            MediaItemFadeDoubleClickBehavior::OpenCrossfadeEditor => 2,
            MediaItemFadeDoubleClickBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemFadeDoubleClickBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemFadeDoubleClickBehavior::NoAction => "No action",
            MediaItemFadeDoubleClickBehavior::DeleteItemFadeCrossfade => {
                "Delete item fade/crossfade"
            }
            MediaItemFadeDoubleClickBehavior::OpenCrossfadeEditor => "Open crossfade editor",
            MediaItemFadeDoubleClickBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}
