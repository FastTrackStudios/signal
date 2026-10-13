//! `StandaloneToolbar` — sync toolbar sub-handle.
//!
//! Stores buttons keyed by `(target, command_name)` plus the workflow id that
//! registered them. `is_available` always returns `true` for the in-memory
//! backend.

use daw_proto::{DawError, DawResult, ToolbarButton, ToolbarTarget, sync::Toolbar};

use super::daw::Standalone;

/// Hashable form of `ToolbarTarget` for use as a HashMap key.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum ToolbarTargetKey {
    Main,
    Floating(u8),
    Midi(u8),
}

impl From<&ToolbarTarget> for ToolbarTargetKey {
    fn from(t: &ToolbarTarget) -> Self {
        match t {
            ToolbarTarget::Main => ToolbarTargetKey::Main,
            ToolbarTarget::Floating(n) => ToolbarTargetKey::Floating(*n),
            ToolbarTarget::Midi(n) => ToolbarTargetKey::Midi(*n),
        }
    }
}

/// Stored toolbar entry tracking button state plus owning workflow.
#[derive(Clone, Debug)]
pub struct ToolbarEntry {
    pub button: ToolbarButton,
    pub workflow_id: String,
}

pub struct StandaloneToolbar<'a> {
    daw: &'a Standalone,
}

impl<'a> StandaloneToolbar<'a> {
    pub(crate) fn new(daw: &'a Standalone) -> Self {
        Self { daw }
    }
}

impl<'a> Toolbar for StandaloneToolbar<'a> {
    fn is_available(&self) -> bool {
        true
    }

    fn add_button(&self, button: ToolbarButton, workflow_id: &str) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        let key = (
            ToolbarTargetKey::from(&button.target),
            button.command_name.clone(),
        );
        s.toolbar_buttons.insert(
            key,
            ToolbarEntry {
                button,
                workflow_id: workflow_id.to_string(),
            },
        );
        Ok(())
    }

    fn update_button(&self, button: ToolbarButton, workflow_id: &str) -> DawResult<()> {
        // Idempotent w.r.t. add_button — same insert semantics.
        self.add_button(button, workflow_id)
    }

    fn remove_button(&self, target: ToolbarTarget, cmd_name: &str) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        let key = (ToolbarTargetKey::from(&target), cmd_name.to_string());
        s.toolbar_buttons
            .remove(&key)
            .ok_or_else(|| DawError::not_found("ToolbarButton", cmd_name))?;
        Ok(())
    }
}
