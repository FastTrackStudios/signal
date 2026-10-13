//! Toolbar service — register/update/move/remove host toolbar buttons.

use super::{ToolbarButton, ToolbarIcon, ToolbarResult, ToolbarTarget, TrackedButton};

#[architect::rpc]
pub trait Toolbar {
    /// Add a toolbar button. Returns the resolved command ID via
    /// `ToolbarResult`. Idempotent — if the button already exists,
    /// returns the existing ID. `workflow_id` groups buttons for
    /// batch removal.
    fn add_button(&self, button: ToolbarButton, workflow_id: &str) -> ToolbarResult;

    /// Update an existing toolbar button (or add if not present).
    fn update_button(&self, button: ToolbarButton, workflow_id: &str) -> ToolbarResult;

    fn remove_button(&self, target: ToolbarTarget, command_name: &str) -> ToolbarResult;

    /// Move a toolbar button to a zero-based position.
    fn move_button(
        &self,
        target: ToolbarTarget,
        command_name: &str,
        position: u32,
    ) -> ToolbarResult;

    /// Set or clear a button's icon while preserving label + flags.
    fn set_button_icon(
        &self,
        target: ToolbarTarget,
        command_name: &str,
        icon: Option<ToolbarIcon>,
    ) -> ToolbarResult;

    /// Remove all buttons belonging to a workflow.
    fn remove_workflow_buttons(&self, workflow_id: &str) -> ToolbarResult;

    fn is_available(&self) -> bool;
    fn tracked_buttons(&self) -> Vec<TrackedButton>;
}
