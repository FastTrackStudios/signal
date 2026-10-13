//! `StandaloneActionRegistry` — sync action-registry sub-handle.
//!
//! Stores registered actions in-memory keyed by command name. Each call to
//! `register*` allocates a fresh command id from a monotonically increasing
//! counter on the parent `Standalone` state.

use daw_proto::{DawError, DawResult, sync::ActionRegistry};

use super::daw::Standalone;

/// Stored entry for a registered action.
#[derive(Clone, Debug)]
pub struct ActionEntry {
    pub id: u32,
    pub description: String,
    /// Bitmask of action flags. Bit 0 = in-menu, bit 1 = toggle.
    pub flags: u32,
}

impl ActionEntry {
    pub const FLAG_IN_MENU: u32 = 1 << 0;
    pub const FLAG_TOGGLE: u32 = 1 << 1;
}

pub struct StandaloneActionRegistry<'a> {
    daw: &'a Standalone,
}

impl<'a> StandaloneActionRegistry<'a> {
    pub(crate) fn new(daw: &'a Standalone) -> Self {
        Self { daw }
    }

    fn register_with_flags(&self, cmd_name: &str, description: &str, flags: u32) -> DawResult<u32> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        let id = s.next_action_id;
        s.next_action_id = s
            .next_action_id
            .checked_add(1)
            .ok_or_else(|| DawError::internal("standalone action registry id space exhausted"))?;
        s.actions.insert(
            cmd_name.to_string(),
            ActionEntry {
                id,
                description: description.to_string(),
                flags,
            },
        );
        Ok(id)
    }
}

impl<'a> ActionRegistry for StandaloneActionRegistry<'a> {
    fn register(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        self.register_with_flags(cmd_name, description, 0)
    }

    fn register_in_menu(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        self.register_with_flags(cmd_name, description, ActionEntry::FLAG_IN_MENU)
    }

    fn register_toggle(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        self.register_with_flags(cmd_name, description, ActionEntry::FLAG_TOGGLE)
    }

    fn register_toggle_in_menu(&self, cmd_name: &str, description: &str) -> DawResult<u32> {
        self.register_with_flags(
            cmd_name,
            description,
            ActionEntry::FLAG_IN_MENU | ActionEntry::FLAG_TOGGLE,
        )
    }

    fn unregister(&self, cmd_name: &str) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        s.actions
            .remove(cmd_name)
            .ok_or_else(|| DawError::not_found("Action", cmd_name))?;
        Ok(())
    }
}
