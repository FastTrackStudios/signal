//! Commands produced by the input processor.

use crate::event::InputEvent;
use crate::mode::ModeId;

/// Unique identifier for an action.
///
/// Kept local for now so `input` remains buildable while upstream `actions-proto`
/// is temporarily incompatible in this workspace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionId(pub String);

impl ActionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A command produced by the input processor in response to input events.
#[derive(Debug, Clone)]
pub enum InputCommand {
    /// Execute a simple action.
    Action(ActionId),
    /// Execute an action with composed arguments (count, operator, motion, etc.).
    ActionWithArgs { action: ActionId, args: InputArgs },
    /// Switch the base editing mode.
    SwitchMode(ModeId),
    /// Push a transient sub-mode onto the stack.
    PushMode(ModeId),
    /// Pop the current sub-mode from the stack.
    PopMode,
    /// Insert literal text (used in insert mode).
    InsertText(String),
    /// The event was not handled by any binding.
    Unhandled(InputEvent),
    /// Keys are buffered, waiting for more input.
    Pending { display: String },
}

/// Composed arguments for an action (count, operator, motion, etc.).
#[derive(Debug, Clone, Default)]
pub struct InputArgs {
    pub count: Option<u32>,
    pub operator: Option<String>,
    pub motion: Option<String>,
    pub text_object: Option<String>,
    pub register: Option<char>,
}
