//! Action registry events.

use facet::Facet;

/// Events pushed to subscribers when their registered actions are triggered.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum ActionEvent {
    /// A registered action was triggered by the user (hotkey, toolbar,
    /// script).
    Triggered {
        /// The command name that was registered.
        command_name: String,
    },
}

// SelfRef compatibility: ActionEvent has no lifetime parameters,
// so Ref<'a> = Self.
#[allow(unsafe_code)]
unsafe impl vox_types::Reborrow for ActionEvent {
    type Ref<'a> = ActionEvent;
}
