use crate::input::mouse_modifiers::behaviors::media_item::{
    MediaItemClickBehavior, MediaItemEdgeLeftDragBehavior, MediaItemLeftDragBehavior,
    MediaItemLowerLeftDragBehavior,
};
use crate::input::mouse_modifiers::behaviors::ruler::RulerLeftDragBehavior;
use crate::input::mouse_modifiers::behaviors::track::{TrackClickBehavior, TrackLeftDragBehavior};
use crate::input::mouse_modifiers::core::MouseModifierFlag;
use crate::input::mouse_modifiers::manager::MouseModifierSetting;

pub fn media_item_edge() -> Vec<MouseModifierSetting> {
    vec![
        MouseModifierSetting::default_behavior(
            "MM_CTX_ITEMEDGE",
            MediaItemEdgeLeftDragBehavior::MoveEdge,
        ),
        MouseModifierSetting::shift_behavior(
            "MM_CTX_ITEMEDGE",
            MediaItemEdgeLeftDragBehavior::MoveEdgeIgnoringSnap,
        ),
    ]
}

pub fn media_item_body() -> Vec<MouseModifierSetting> {
    vec![
        MouseModifierSetting::default_behavior(
            "MM_CTX_ITEM",
            MediaItemLeftDragBehavior::MoveItemIgnoringTimeSelection,
        ),
        MouseModifierSetting::shift_behavior(
            "MM_CTX_ITEM",
            MediaItemLeftDragBehavior::MoveItemIgnoringSnapAndTimeSelection,
        ),
        MouseModifierSetting::alt_behavior("MM_CTX_ITEM", MediaItemLeftDragBehavior::CopyItem),
        MouseModifierSetting::alt_shift_behavior(
            "MM_CTX_ITEM",
            MediaItemLeftDragBehavior::CopyItemIgnoringSnap,
        ),
    ]
}

pub fn media_item_lower() -> Vec<MouseModifierSetting> {
    vec![
        MouseModifierSetting::default_behavior(
            "MM_CTX_ITEMLOWER",
            MediaItemLowerLeftDragBehavior::SelectRazorEditArea,
        ),
        MouseModifierSetting::cmd_behavior(
            "MM_CTX_ITEMLOWER",
            MediaItemLowerLeftDragBehavior::AddToRazorEditArea,
        ),
        MouseModifierSetting::shift_behavior(
            "MM_CTX_ITEMLOWER",
            MediaItemLowerLeftDragBehavior::SelectRazorEditAreaIgnoringSnap,
        ),
        MouseModifierSetting::shift_cmd_behavior(
            "MM_CTX_ITEMLOWER",
            MediaItemLowerLeftDragBehavior::AddToRazorEditAreaIgnoringSnap,
        ),
    ]
}

pub fn media_item_click() -> Vec<MouseModifierSetting> {
    vec![
        MouseModifierSetting::default_behavior(
            "MM_CTX_ITEM_CLK",
            MediaItemClickBehavior::SelectItemAndMoveEditCursor,
        ),
        MouseModifierSetting::shift_behavior(
            "MM_CTX_ITEM_CLK",
            MediaItemClickBehavior::AddARangeOfItemsToSelectionIfAlreadySelectedExtendTimeSelection,
        ),
        MouseModifierSetting::cmd_behavior(
            "MM_CTX_ITEM_CLK",
            MediaItemClickBehavior::ToggleItemSelection,
        ),
        MouseModifierSetting::shift_cmd_behavior(
            "MM_CTX_ITEM_CLK",
            MediaItemClickBehavior::SelectItemAndMoveEditCursorIgnoringSnap,
        ),
        MouseModifierSetting::alt_behavior(
            "MM_CTX_ITEM_CLK",
            MediaItemClickBehavior::SelectItemIgnoringGrouping,
        ),
        MouseModifierSetting::alt_shift_behavior(
            "MM_CTX_ITEM_CLK",
            MediaItemClickBehavior::ExtendRazorEditArea,
        ),
        MouseModifierSetting::cmd_opt_behavior(
            "MM_CTX_ITEM_CLK",
            MediaItemClickBehavior::AddStretchMarker,
        ),
    ]
}

pub fn track_left_drag() -> Vec<MouseModifierSetting> {
    vec![
        MouseModifierSetting::default_behavior(
            "MM_CTX_TRACK",
            TrackLeftDragBehavior::MarqueeSelectItems,
        ),
        MouseModifierSetting::shift_behavior(
            "MM_CTX_TRACK",
            TrackLeftDragBehavior::MarqueeAddToItemSelection,
        ),
        MouseModifierSetting::with_cmd("MM_CTX_TRACK", "25 m"),
        MouseModifierSetting::new(
            "MM_CTX_TRACK",
            MouseModifierFlag::new(true, true, false, false),
            "27 m",
        ),
        MouseModifierSetting::alt_behavior("MM_CTX_TRACK", TrackLeftDragBehavior::MarqueeZoom),
    ]
}

pub fn ruler_left_drag() -> Vec<MouseModifierSetting> {
    vec![
        MouseModifierSetting::default_behavior(
            "MM_CTX_RULER",
            RulerLeftDragBehavior::EditLoopPoint,
        ),
        MouseModifierSetting::shift_behavior("MM_CTX_RULER", RulerLeftDragBehavior::MoveLoopPoints),
        MouseModifierSetting::alt_behavior("MM_CTX_RULER", RulerLeftDragBehavior::NoAction),
        MouseModifierSetting::alt_shift_behavior(
            "MM_CTX_RULER",
            RulerLeftDragBehavior::MoveLoopPointsIgnoringSnap,
        ),
        MouseModifierSetting::cmd_behavior("MM_CTX_RULER", RulerLeftDragBehavior::NoAction),
        MouseModifierSetting::shift_cmd_behavior("MM_CTX_RULER", RulerLeftDragBehavior::NoAction),
        MouseModifierSetting::cmd_opt_behavior("MM_CTX_RULER", RulerLeftDragBehavior::NoAction),
        MouseModifierSetting::shift_cmd_opt_behavior(
            "MM_CTX_RULER",
            RulerLeftDragBehavior::NoAction,
        ),
    ]
}

pub fn track_left_click() -> Vec<MouseModifierSetting> {
    vec![
        MouseModifierSetting::default_behavior(
            "MM_CTX_TRACK_CLK",
            TrackClickBehavior::DeselectAllItems,
        ),
        MouseModifierSetting::shift_behavior(
            "MM_CTX_TRACK_CLK",
            TrackClickBehavior::DeselectAllItemsAndMoveEditCursor,
        ),
        MouseModifierSetting::alt_shift_behavior(
            "MM_CTX_TRACK_CLK",
            TrackClickBehavior::DeselectAllItemsAndMoveEditCursorIgnoringSnap,
        ),
    ]
}

pub fn all() -> Vec<MouseModifierSetting> {
    super::merge_sections([
        media_item_edge(),
        media_item_body(),
        media_item_lower(),
        media_item_click(),
        track_left_drag(),
        ruler_left_drag(),
        track_left_click(),
    ])
}
