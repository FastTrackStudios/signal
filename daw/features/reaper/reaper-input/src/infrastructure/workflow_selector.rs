//! Workflow and Profile Selector Stubs
//!
//! Minimal stubs for the popup menu selectors. The actual UI implementation
//! lives in `reaper-extension/src/local_actions.rs` via `define_actions!`.

use crate::input::handler::InputHandler;
use crate::input::workflows;

/// Action handler for profile selector popup menu
pub fn profile_selector_handler() {
    // Profile selector is handled by reaper-extension's local_actions INPUT_MENU
    tracing::debug!("profile_selector_handler stub called");
}

/// Get toggle state for profile selector (true if FTS-Input is enabled)
pub fn get_profile_selector_state() -> bool {
    InputHandler::is_enabled()
}

/// Action handler for workflow selector popup menu
pub fn workflow_selector_handler() {
    tracing::debug!("workflow_selector_handler stub called");
}

/// Get toggle state for workflow selector (true if a workflow is active)
pub fn get_workflow_selector_state() -> bool {
    workflows::get_active_workflow_id().is_some()
}
