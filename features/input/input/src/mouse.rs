//! Mouse pattern matching and per-mode binding tables.

use crate::command::ActionId;
use crate::context::{ActionContext, WhenExpr};
use crate::event::MouseEvent;
use crate::key::Modifiers;

/// A mouse gesture pattern used for binding lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MousePattern {
    pub button: crate::event::MouseButton,
    pub action: crate::event::MouseAction,
    pub modifiers: Modifiers,
}

impl MousePattern {
    pub fn new(
        button: crate::event::MouseButton,
        action: crate::event::MouseAction,
        modifiers: Modifiers,
    ) -> Self {
        Self {
            button,
            action,
            modifiers,
        }
    }

    pub fn matches(&self, event: &MouseEvent) -> bool {
        self.button == event.button
            && self.action == event.action
            && self.modifiers == event.modifiers
    }
}

/// Ordered set of mouse bindings for a mode.
///
/// First matching entry wins.
#[derive(Debug, Clone, Default)]
pub struct MouseBindingTable {
    bindings: Vec<(MousePattern, WhenExpr, ActionId)>,
}

impl MouseBindingTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, pattern: MousePattern, when: WhenExpr, action: ActionId) {
        self.bindings.push((pattern, when, action));
    }

    /// Read-only access to the raw binding entries.
    pub fn bindings(&self) -> &[(MousePattern, WhenExpr, ActionId)] {
        &self.bindings
    }

    pub fn match_event(&self, event: &MouseEvent, ctx: &ActionContext) -> Option<ActionId> {
        for (pattern, when, action) in &self.bindings {
            if pattern.matches(event) && when.evaluate(ctx) {
                return Some(action.clone());
            }
        }
        None
    }
}
