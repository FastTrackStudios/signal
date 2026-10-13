//! Fast Slip Edit Override Layer
//!
//! Keybindings active during fast slip editing workflow.
//! Enable with `FTS_INPUT_WORKFLOW_FAST_SLIP_EDIT` action.

use crate::input::keybinds::{Keybind, KeybindOverride};

/// Create the Fast Slip Edit override layer
pub fn fast_slip_edit_override() -> KeybindOverride {
    KeybindOverride::new(
        "fast-slip-edit",
        "Fast slip editing workflow keybindings - quick item manipulation",
    )
    .with_priority(50) // Medium priority
    .with_bindings(vec![
        // === Split with Crossfade ===
        // Press 's' to split selected items at cursor with crossfade on left
        Keybind::new("s", "FTS_SPLIT_ITEMS_CROSSFADE_LEFT")
            .with_description("Split at cursor with crossfade on left"),
        // Press 'x' as an alternative
        Keybind::new("x", "FTS_SPLIT_ITEMS_CROSSFADE_LEFT")
            .with_description("Split at cursor with crossfade on left (alt)"),
    ])
}
