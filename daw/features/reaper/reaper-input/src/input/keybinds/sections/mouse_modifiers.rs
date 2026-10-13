//! Mouse Modifier Action Sets
//!
//! Defines click+drag behaviors for different contexts (edges, fades, etc.)
//! These control behaviors like "what happens when I click the top-right corner of an item".

use crate::input::keybinds::{ActionSet, MouseModifier, MouseModifierContext};

/// Standard REAPER mouse modifiers (full fade controls in corners)
pub struct ReaperMouseModifiers;

impl ActionSet for ReaperMouseModifiers {
    fn name(&self) -> &'static str {
        "ReaperMouseModifiers"
    }

    fn mouse_modifiers(&self) -> Vec<MouseModifier> {
        vec![
            // REAPER default: corners have fade controls
            MouseModifier::new(MouseModifierContext::MediaItemFade, "", "fade_adjust")
                .with_description("Adjust fade curve (REAPER default)"),
            MouseModifier::new(MouseModifierContext::MediaItemLeftEdge, "", "edge_resize")
                .with_description("Resize item from left edge"),
            MouseModifier::new(MouseModifierContext::MediaItemRightEdge, "", "edge_resize")
                .with_description("Resize item from right edge"),
            // Shift+drag on edge = slip edit
            MouseModifier::new(MouseModifierContext::MediaItemLeftEdge, "<S->", "slip_edit")
                .with_description("Slip edit from left edge"),
            MouseModifier::new(
                MouseModifierContext::MediaItemRightEdge,
                "<S->",
                "slip_edit",
            )
            .with_description("Slip edit from right edge"),
            // Alt+drag on item = copy
            MouseModifier::new(
                MouseModifierContext::MediaItemBottomHalf,
                "<A->",
                "copy_item",
            )
            .with_description("Copy item on drag"),
        ]
    }
}

/// Logic Pro style mouse modifiers (no fade corners - cleaner edge behavior)
pub struct LogicMouseModifiers;

impl ActionSet for LogicMouseModifiers {
    fn name(&self) -> &'static str {
        "LogicMouseModifiers"
    }

    fn mouse_modifiers(&self) -> Vec<MouseModifier> {
        vec![
            // Logic style: simpler edge behavior, no fade hotspots in corners
            MouseModifier::new(MouseModifierContext::MediaItemLeftEdge, "", "edge_resize")
                .with_description("Resize item from left edge"),
            MouseModifier::new(MouseModifierContext::MediaItemRightEdge, "", "edge_resize")
                .with_description("Resize item from right edge"),
            // Option+drag = copy (Logic style)
            MouseModifier::new(
                MouseModifierContext::MediaItemBottomHalf,
                "<A->",
                "copy_item",
            )
            .with_description("Copy item on drag (Option+drag)"),
            // Shift+drag on edge = slip edit
            MouseModifier::new(MouseModifierContext::MediaItemLeftEdge, "<S->", "slip_edit")
                .with_description("Slip edit from left edge"),
            MouseModifier::new(
                MouseModifierContext::MediaItemRightEdge,
                "<S->",
                "slip_edit",
            )
            .with_description("Slip edit from right edge"),
            // NOTE: No fade controls in corners - this is the key Logic difference
        ]
    }
}

/// No mouse modifier overrides - use REAPER defaults
pub struct NoMouseModifiers;

impl ActionSet for NoMouseModifiers {
    fn name(&self) -> &'static str {
        "NoMouseModifiers"
    }

    fn mouse_modifiers(&self) -> Vec<MouseModifier> {
        vec![]
    }
}
