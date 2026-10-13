//! Media Item edge behaviors

use crate::input::mouse_modifiers::behaviors::shared::traits::{BehaviorDisplay, BehaviorId};

/// Media Item edge left drag behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemEdgeLeftDragBehavior {
    NoAction,
    MoveEdge,
    StretchItem,
    MoveEdgeIgnoringSnap,
    StretchItemIgnoringSnap,
    MoveEdgeIgnoringSelectionGrouping,
    StretchItemIgnoringSelectionGrouping,
    MoveEdgeIgnoringSnapAndSelectionGrouping,
    StretchItemIgnoringSnapAndSelectionGrouping,
    MoveEdgeRelativeEdgeEdit,
    StretchItemRelativeEdgeEdit,
    MoveEdgeIgnoringSnapRelativeEdgeEdit,
    StretchItemIgnoringSnapRelativeEdgeEdit,
    MoveEdgeWithoutChangingFadeTime,
    MoveEdgeIgnoringSnapWithoutChangingFadeTime,
    MoveEdgeIgnoringSelectionGroupingWithoutChangingFadeTime,
    MoveEdgeIgnoringSnapAndSelectionGroupingWithoutChangingFadeTime,
    MoveEdgeWithoutChangingFadeTimeRelativeEdgeEdit,
    MoveEdgeIgnoringSnapWithoutChangingFadeTimeRelativeEdgeEdit,
    SelectRazorEditArea,
    SelectRazorEditAreaIgnoringSnap,
    AddToRazorEditArea,
    AddToRazorEditAreaIgnoringSnap,
    SelectRazorEditAreaAndTime,
    SelectRazorEditAreaAndTimeIgnoringSnap,
    Unknown(u32),
}

impl BehaviorId for MediaItemEdgeLeftDragBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemEdgeLeftDragBehavior::NoAction,
            1 => MediaItemEdgeLeftDragBehavior::MoveEdge,
            2 => MediaItemEdgeLeftDragBehavior::StretchItem,
            3 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnap,
            4 => MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnap,
            5 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSelectionGrouping,
            6 => MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSelectionGrouping,
            7 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapAndSelectionGrouping,
            8 => MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnapAndSelectionGrouping,
            9 => MediaItemEdgeLeftDragBehavior::MoveEdgeRelativeEdgeEdit,
            10 => MediaItemEdgeLeftDragBehavior::StretchItemRelativeEdgeEdit,
            11 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapRelativeEdgeEdit,
            12 => MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnapRelativeEdgeEdit,
            13 => MediaItemEdgeLeftDragBehavior::MoveEdgeWithoutChangingFadeTime,
            14 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapWithoutChangingFadeTime,
            15 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSelectionGroupingWithoutChangingFadeTime,
            16 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapAndSelectionGroupingWithoutChangingFadeTime,
            17 => MediaItemEdgeLeftDragBehavior::MoveEdgeWithoutChangingFadeTimeRelativeEdgeEdit,
            18 => MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapWithoutChangingFadeTimeRelativeEdgeEdit,
            21 => MediaItemEdgeLeftDragBehavior::SelectRazorEditArea,
            22 => MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaIgnoringSnap,
            23 => MediaItemEdgeLeftDragBehavior::AddToRazorEditArea,
            24 => MediaItemEdgeLeftDragBehavior::AddToRazorEditAreaIgnoringSnap,
            25 => MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaAndTime,
            26 => MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaAndTimeIgnoringSnap,
            id => MediaItemEdgeLeftDragBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemEdgeLeftDragBehavior::NoAction => 0,
            MediaItemEdgeLeftDragBehavior::MoveEdge => 1,
            MediaItemEdgeLeftDragBehavior::StretchItem => 2,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnap => 3,
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnap => 4,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSelectionGrouping => 5,
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSelectionGrouping => 6,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapAndSelectionGrouping => 7,
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnapAndSelectionGrouping => 8,
            MediaItemEdgeLeftDragBehavior::MoveEdgeRelativeEdgeEdit => 9,
            MediaItemEdgeLeftDragBehavior::StretchItemRelativeEdgeEdit => 10,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapRelativeEdgeEdit => 11,
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnapRelativeEdgeEdit => 12,
            MediaItemEdgeLeftDragBehavior::MoveEdgeWithoutChangingFadeTime => 13,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapWithoutChangingFadeTime => 14,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSelectionGroupingWithoutChangingFadeTime => 15,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapAndSelectionGroupingWithoutChangingFadeTime => 16,
            MediaItemEdgeLeftDragBehavior::MoveEdgeWithoutChangingFadeTimeRelativeEdgeEdit => 17,
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapWithoutChangingFadeTimeRelativeEdgeEdit => 18,
            MediaItemEdgeLeftDragBehavior::SelectRazorEditArea => 21,
            MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaIgnoringSnap => 22,
            MediaItemEdgeLeftDragBehavior::AddToRazorEditArea => 23,
            MediaItemEdgeLeftDragBehavior::AddToRazorEditAreaIgnoringSnap => 24,
            MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaAndTime => 25,
            MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaAndTimeIgnoringSnap => 26,
            MediaItemEdgeLeftDragBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemEdgeLeftDragBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemEdgeLeftDragBehavior::NoAction => "No action",
            MediaItemEdgeLeftDragBehavior::MoveEdge => "Move edge",
            MediaItemEdgeLeftDragBehavior::StretchItem => "Stretch item",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnap => "Move edge ignoring snap",
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnap => "Stretch item ignoring snap",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSelectionGrouping => "Move edge ignoring selection/grouping",
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSelectionGrouping => "Stretch item ignoring selection/grouping",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapAndSelectionGrouping => "Move edge ignoring snap and selection/grouping",
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnapAndSelectionGrouping => "Stretch item ignoring snap and selection/grouping",
            MediaItemEdgeLeftDragBehavior::MoveEdgeRelativeEdgeEdit => "Move edge (relative edge edit)",
            MediaItemEdgeLeftDragBehavior::StretchItemRelativeEdgeEdit => "Stretch item (relative edge edit)",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapRelativeEdgeEdit => "Move edge ignoring snap (relative edge edit)",
            MediaItemEdgeLeftDragBehavior::StretchItemIgnoringSnapRelativeEdgeEdit => "Stretch item ignoring snap (relative edge edit)",
            MediaItemEdgeLeftDragBehavior::MoveEdgeWithoutChangingFadeTime => "Move edge without changing fade time",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapWithoutChangingFadeTime => "Move edge ignoring snap without changing fade time",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSelectionGroupingWithoutChangingFadeTime => "Move edge ignoring selection/grouping without changing fade time",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapAndSelectionGroupingWithoutChangingFadeTime => "Move edge ignoring snap and selection/grouping without changing fade time",
            MediaItemEdgeLeftDragBehavior::MoveEdgeWithoutChangingFadeTimeRelativeEdgeEdit => "Move edge without changing fade time (relative edge edit)",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnapWithoutChangingFadeTimeRelativeEdgeEdit => "Move edge ignoring snap without changing fade time (relative edge edit)",
            MediaItemEdgeLeftDragBehavior::SelectRazorEditArea => "Select razor edit area",
            MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaIgnoringSnap => "Select razor edit area ignoring snap",
            MediaItemEdgeLeftDragBehavior::AddToRazorEditArea => "Add to razor edit area",
            MediaItemEdgeLeftDragBehavior::AddToRazorEditAreaIgnoringSnap => "Add to razor edit area ignoring snap",
            MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaAndTime => "Select razor edit area and time",
            MediaItemEdgeLeftDragBehavior::SelectRazorEditAreaAndTimeIgnoringSnap => "Select razor edit area and time ignoring snap",
            MediaItemEdgeLeftDragBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}

/// Media Item edge double click behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaItemEdgeDoubleClickBehavior {
    NoAction,
    NoActionDuplicate, // According to docs, both 0 and 1 are "no action"
    Unknown(u32),
}

impl BehaviorId for MediaItemEdgeDoubleClickBehavior {
    fn from_behavior_id(behavior_id: u32) -> Self {
        match behavior_id {
            0 => MediaItemEdgeDoubleClickBehavior::NoAction,
            1 => MediaItemEdgeDoubleClickBehavior::NoActionDuplicate,
            id => MediaItemEdgeDoubleClickBehavior::Unknown(id),
        }
    }

    fn to_behavior_id(&self) -> u32 {
        match self {
            MediaItemEdgeDoubleClickBehavior::NoAction => 0,
            MediaItemEdgeDoubleClickBehavior::NoActionDuplicate => 1,
            MediaItemEdgeDoubleClickBehavior::Unknown(id) => *id,
        }
    }
}

impl BehaviorDisplay for MediaItemEdgeDoubleClickBehavior {
    fn display_name(&self) -> &'static str {
        match self {
            MediaItemEdgeDoubleClickBehavior::NoAction => "No action",
            MediaItemEdgeDoubleClickBehavior::NoActionDuplicate => "No action",
            MediaItemEdgeDoubleClickBehavior::Unknown(_) => "Unknown behavior",
        }
    }
}
