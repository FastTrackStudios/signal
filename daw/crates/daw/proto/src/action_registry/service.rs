//! Action registration service — register/execute REAPER actions.
//!
//! Subscriber stream (`subscribe_actions`) retires with the
//! architect::rpc port; sibling-trait when revived.

use super::{ActionExecutionResult, ActionListRequest, ActionListResponse};
use crate::DawResult;

#[architect::rpc]
pub trait ActionRegistration {
    /// Register a new REAPER action. Returns the assigned command ID
    /// (0 on failure).
    ///
    /// - `command_name`: Unique identifier (e.g. `"FTS_SIGNAL_ARM"`).
    /// - `description`: Label shown in REAPER's action list.
    /// - `show_in_menu`: If true, appears in REAPER's Extensions menu.
    /// - `toggleable`: If true, REAPER shows an on/off indicator.
    fn register_action(
        &self,
        command_name: &str,
        description: &str,
        show_in_menu: bool,
        toggleable: bool,
    ) -> u32;

    /// Convenience: register a plain action (not toggle, no menu).
    fn register(&self, cmd_name: &str, description: &str) -> DawResult<u32>;

    /// Register an action that also appears in the host's main menu.
    fn register_in_menu(&self, cmd_name: &str, description: &str) -> DawResult<u32>;

    /// Register a toggle (on/off) action.
    fn register_toggle(&self, cmd_name: &str, description: &str) -> DawResult<u32>;

    /// Register a toggle action that also appears in the main menu.
    fn register_toggle_in_menu(&self, cmd_name: &str, description: &str) -> DawResult<u32>;

    fn unregister(&self, cmd_name: &str) -> DawResult<()>;

    /// Check if an action is registered (by us or any other extension).
    fn is_registered(&self, command_name: &str) -> bool;

    /// Look up the numeric command ID for a named action.
    fn lookup_command_id(&self, command_name: &str) -> Option<u32>;

    /// Check if an action is actually present in REAPER's action list
    /// (verifies the gaccel entry exists).
    fn is_in_action_list(&self, command_name: &str) -> bool;

    /// Enumerate actions in REAPER's main action list.
    fn list_actions(&self, request: ActionListRequest) -> ActionListResponse;

    /// Run a native DAW action by numeric command ID. Maps to
    /// `Main_OnCommandEx(command_id, 0, current_project)`.
    fn run_action(&self, command_id: u32);

    /// Execute a named action (custom or native).
    fn execute_named_action(&self, command_name: &str) -> bool;

    /// Execute any REAPER action — accepts numeric ID or named
    /// identifier. Returns resolved metadata.
    fn execute_action(&self, action_id: &str) -> ActionExecutionResult;

    /// Set the toggle state for a toggleable action.
    fn set_toggle_state(&self, command_name: &str, is_on: bool);

    /// `None` if not registered or not toggleable.
    fn get_toggle_state(&self, command_name: &str) -> Option<bool>;
}
