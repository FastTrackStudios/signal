use crate::input::mouse_modifiers::behaviors::media_item::MediaItemEdgeLeftDragBehavior;
use crate::input::mouse_modifiers::behaviors::track::{TrackClickBehavior, TrackLeftDragBehavior};
use crate::input::mouse_modifiers::core::MouseModifierFlag;
use crate::input::mouse_modifiers::manager::MouseModifierSetting;

pub fn media_item_edge() -> Vec<MouseModifierSetting> {
    vec![MouseModifierSetting::default_behavior(
        "MM_CTX_ITEMEDGE",
        MediaItemEdgeLeftDragBehavior::MoveEdge,
    )]
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
    super::merge_sections([media_item_edge(), track_left_drag(), track_left_click()])
}
