//! Armed Click Action System
//!
//! Implements custom "armed" behavior without using REAPER's native ArmCommand.
//! REAPER's native arming consumes the next click entirely, which doesn't work
//! for our workflow pattern where we want to intercept clicks and run our action.
//!
//! Instead, we intercept WM_LBUTTONDOWN in the wheel hook and check if the
//! active workflow has an armed click action that matches the current mouse context.

use reaper_high::Reaper;
use reaper_low::Swell;
use tracing::debug;

// Re-export context detection types and functions from the context_detection module
pub use super::context_detection::{
    ItemHitInfo, MouseContextResult, MouseModifierContext, detect_context_at_point,
    detect_mouse_modifier_context, is_debug_mouse_context_enabled, toggle_debug_mouse_context,
};

// region: --- Armed Click Action

/// Defines what contexts an armed click action responds to
#[derive(Debug, Clone)]
pub struct ArmedClickAction {
    /// Action command ID to execute on click
    pub action: String,
    /// Mouse contexts that trigger this action
    pub target_contexts: Vec<ArmedContext>,
    /// Whether to pass the click through to REAPER after executing (default: false)
    pub pass_through: bool,
    /// After executing, FTS drives a slip-edit drag on the item under the mouse
    /// itself (instead of passing the click through). See [`super::slip_drag`].
    pub slip_drag: bool,
}

impl ArmedClickAction {
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            target_contexts: Vec::new(),
            pass_through: false,
            slip_drag: false,
        }
    }

    /// Add a target context
    pub fn with_context(mut self, context: ArmedContext) -> Self {
        self.target_contexts.push(context);
        self
    }

    /// Add multiple target contexts
    pub fn with_contexts(mut self, contexts: impl IntoIterator<Item = ArmedContext>) -> Self {
        self.target_contexts.extend(contexts);
        self
    }

    /// Set whether to pass the click through after executing
    pub fn with_pass_through(mut self, pass_through: bool) -> Self {
        self.pass_through = pass_through;
        self
    }

    /// Set whether FTS drives a slip-edit drag after executing (instead of
    /// passing the click through). Implies `pass_through = false`.
    pub fn with_slip_drag(mut self, slip_drag: bool) -> Self {
        self.slip_drag = slip_drag;
        if slip_drag {
            self.pass_through = false;
        }
        self
    }

    /// Check if the given mouse position matches any of our target contexts
    pub fn matches_position(&self, mouse_x: i32, mouse_y: i32) -> bool {
        for context in &self.target_contexts {
            if context.matches_position(mouse_x, mouse_y) {
                return true;
            }
        }

        false
    }

    /// Execute the armed action
    pub fn execute(&self) {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();

        // First, try using the action registry's stored command IDs
        if let Some(cmd_id) = crate::infrastructure::action_registry::get_command_id(&self.action) {
            debug!(action = %self.action, cmd_id = cmd_id.get(), "Executing armed click action via registry");
            medium.low().Main_OnCommand(cmd_id.get() as i32, 0);
            return;
        }

        // Try as named command
        if let Some(cmd_id) = medium.named_command_lookup(self.action.as_str()) {
            debug!(action = %self.action, cmd_id = cmd_id.get(), "Executing armed click action via named lookup");
            medium.low().Main_OnCommand(cmd_id.get() as i32, 0);
            return;
        }

        // Try parsing as numeric
        if let Ok(cmd_id) = self.action.parse::<i32>() {
            debug!(action = %self.action, cmd_id = cmd_id, "Executing armed click action as numeric");
            medium.low().Main_OnCommand(cmd_id, 0);
            return;
        }

        tracing::warn!(action = %self.action, "Could not resolve armed action command ID");
    }
}

/// Helper to create common armed click configurations
impl ArmedClickAction {
    /// Create an armed action that triggers on any item click
    pub fn on_item(action: impl Into<String>) -> Self {
        Self::new(action).with_context(ArmedContext::Item)
    }

    /// Create an armed action that triggers anywhere in arrange view
    pub fn in_arrange(action: impl Into<String>) -> Self {
        Self::new(action).with_context(ArmedContext::Arrange)
    }

    /// Create an armed action that triggers on item lower half (slip edit zone)
    pub fn on_item_lower(action: impl Into<String>) -> Self {
        Self::new(action).with_context(ArmedContext::ItemLower)
    }
}

// endregion: --- Armed Click Action

// region: --- Armed Context

/// Context where armed click action can be triggered
#[derive(Debug, Clone)]
pub enum ArmedContext {
    /// Any click in arrange view
    Arrange,
    /// Click on a media item (anywhere on the item)
    Item,
    /// Click on item edge (for trimming)
    ItemEdge,
    /// Click on item lower half
    ItemLower,
    /// Click in empty track area (no item)
    Track,
    /// Click on ruler
    Ruler,
    /// Click on envelope
    Envelope,
}

impl ArmedContext {
    /// Check if the given mouse position matches this context
    fn matches_position(&self, mouse_x: i32, mouse_y: i32) -> bool {
        match self {
            ArmedContext::Arrange => {
                // Check if mouse is in arrange view
                is_in_arrange_view(mouse_x, mouse_y)
            }
            ArmedContext::Item => {
                // Check if there's an item at this position
                is_over_item(mouse_x, mouse_y)
            }
            ArmedContext::ItemEdge => {
                // TODO: Detect item edge specifically
                // For now, check if over item (could be refined with position relative to item bounds)
                is_over_item(mouse_x, mouse_y)
            }
            ArmedContext::ItemLower => {
                // TODO: Detect lower half of item
                // For now, check if over item
                is_over_item(mouse_x, mouse_y)
            }
            ArmedContext::Track => {
                // In arrange but NOT over an item
                is_in_arrange_view(mouse_x, mouse_y) && !is_over_item(mouse_x, mouse_y)
            }
            ArmedContext::Ruler => {
                // TODO: Implement ruler detection
                false
            }
            ArmedContext::Envelope => {
                // TODO: Implement envelope detection
                false
            }
        }
    }
}

// endregion: --- Armed Context

// region: --- Helper Functions

/// Check if the mouse is in the arrange view
fn is_in_arrange_view(mouse_x: i32, mouse_y: i32) -> bool {
    use crate::input::reaper_windows;

    let reaper = Reaper::get();
    let medium = reaper.medium_reaper();

    // Get arrange window using the existing helper
    if let Some(arrange_hwnd) = reaper_windows::get_arrange_wnd(medium) {
        // Get arrange window rect
        let mut rect = reaper_low::raw::RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };

        let swell = Swell::get();
        unsafe {
            swell.GetWindowRect(arrange_hwnd, &mut rect);
        }

        // Check if mouse is within arrange bounds
        mouse_x >= rect.left && mouse_x < rect.right && mouse_y >= rect.top && mouse_y < rect.bottom
    } else {
        false
    }
}

/// Check if the mouse is over a media item
fn is_over_item(mouse_x: i32, mouse_y: i32) -> bool {
    get_item_at_point(mouse_x, mouse_y).is_some()
}

/// Get the item at a screen position (if any)
pub fn get_item_at_point(mouse_x: i32, mouse_y: i32) -> Option<*mut reaper_low::raw::MediaItem> {
    use std::ptr;

    let reaper = Reaper::get();
    let medium = reaper.medium_reaper();

    let mut take_out: *mut reaper_low::raw::MediaItem_Take = ptr::null_mut();

    let item = unsafe {
        medium.low().GetItemFromPoint(
            mouse_x,
            mouse_y,
            true, // allow_locked
            &mut take_out,
        )
    };

    if item.is_null() { None } else { Some(item) }
}

// endregion: --- Helper Functions
