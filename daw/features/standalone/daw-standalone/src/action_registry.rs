//! `impl ActionRegistration for Standalone` — stub.

use daw_proto::{
    ActionExecutionResult, ActionListRequest, ActionListResponse, ActionRegistration, DawError,
    DawResult,
};

use crate::sync::Standalone;

fn unsupported<T>(what: &str) -> DawResult<T> {
    Err(DawError::operation_failed(format!(
        "standalone has no action registry: {what}"
    )))
}

impl ActionRegistration for Standalone {
    fn register_action(&self, _: &str, _: &str, _: bool, _: bool) -> u32 {
        0
    }
    fn register(&self, _: &str, _: &str) -> DawResult<u32> {
        unsupported("register")
    }
    fn register_in_menu(&self, _: &str, _: &str) -> DawResult<u32> {
        unsupported("register_in_menu")
    }
    fn register_toggle(&self, _: &str, _: &str) -> DawResult<u32> {
        unsupported("register_toggle")
    }
    fn register_toggle_in_menu(&self, _: &str, _: &str) -> DawResult<u32> {
        unsupported("register_toggle_in_menu")
    }
    fn unregister(&self, _: &str) -> DawResult<()> {
        unsupported("unregister")
    }
    fn is_registered(&self, _: &str) -> bool {
        false
    }
    fn lookup_command_id(&self, _: &str) -> Option<u32> {
        None
    }
    fn is_in_action_list(&self, _: &str) -> bool {
        false
    }
    fn list_actions(&self, _: ActionListRequest) -> ActionListResponse {
        ActionListResponse::default()
    }
    fn run_action(&self, _: u32) {}
    fn execute_named_action(&self, _: &str) -> bool {
        false
    }
    fn execute_action(&self, action_id: &str) -> ActionExecutionResult {
        ActionExecutionResult {
            requested_action: action_id.to_string(),
            executed: false,
            command_id: None,
            command_name: None,
            description: None,
            origin: None,
            provider: None,
            provider_tags: Vec::new(),
            registered_by_fts: false,
            toggle_state_before: None,
            toggle_state_after: None,
        }
    }
    fn set_toggle_state(&self, _: &str, _: bool) {}
    fn get_toggle_state(&self, _: &str) -> Option<bool> {
        None
    }
}
