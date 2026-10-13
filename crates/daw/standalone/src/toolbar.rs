//! `impl Toolbar for Standalone` — stub. No host toolbar on this backend.

use daw_proto::toolbar::{
    Toolbar, ToolbarButton, ToolbarIcon, ToolbarResult, ToolbarTarget, TrackedButton,
};

use crate::sync::Standalone;

impl Toolbar for Standalone {
    fn add_button(&self, _button: ToolbarButton, _workflow_id: &str) -> ToolbarResult {
        ToolbarResult::error("toolbar unsupported on standalone backend")
    }

    fn update_button(&self, _button: ToolbarButton, _workflow_id: &str) -> ToolbarResult {
        ToolbarResult::error("toolbar unsupported on standalone backend")
    }

    fn remove_button(&self, _target: ToolbarTarget, _command_name: &str) -> ToolbarResult {
        ToolbarResult::ok(0)
    }

    fn move_button(
        &self,
        _target: ToolbarTarget,
        _command_name: &str,
        _position: u32,
    ) -> ToolbarResult {
        ToolbarResult::error("toolbar unsupported on standalone backend")
    }

    fn set_button_icon(
        &self,
        _target: ToolbarTarget,
        _command_name: &str,
        _icon: Option<ToolbarIcon>,
    ) -> ToolbarResult {
        ToolbarResult::error("toolbar unsupported on standalone backend")
    }

    fn remove_workflow_buttons(&self, _workflow_id: &str) -> ToolbarResult {
        ToolbarResult::ok(0)
    }

    fn is_available(&self) -> bool {
        false
    }

    fn tracked_buttons(&self) -> Vec<TrackedButton> {
        Vec::new()
    }
}
