//! Mouse modifier behavior wrapper

use super::traits::MouseBehavior;
use crate::input::mouse_modifiers::types::{MouseButtonInput, MouseModifierContext};

/// Wrapper for mouse modifier behaviors using trait objects
///
/// This allows us to work with any behavior type generically without needing
/// to enumerate every single variant in a massive enum. New behaviors can be
/// added by simply implementing the `MouseBehavior` trait.
pub enum MouseModifierBehavior {
    /// A known behavior that implements MouseBehavior
    Known(Box<dyn MouseBehavior>),

    /// Unknown behavior for contexts not yet converted
    Unknown {
        context: MouseModifierContext,
        button_input: MouseButtonInput,
        behavior_id: u32,
    },
}

impl Clone for MouseModifierBehavior {
    fn clone(&self) -> Self {
        match self {
            MouseModifierBehavior::Known(behavior) => {
                // We can't clone trait objects directly, but we can store the behavior_id
                // and recreate when needed. For now, we'll store it as Unknown with the behavior_id.
                // This loses the concrete type but preserves the behavior_id for display purposes.
                MouseModifierBehavior::Unknown {
                    context: MouseModifierContext::ArrangeView(
                        crate::input::mouse_modifiers::types::ArrangeViewInteraction::Middle(
                            MouseButtonInput::MiddleDrag,
                        ),
                    ),
                    button_input: MouseButtonInput::MiddleDrag,
                    behavior_id: behavior.behavior_id(),
                }
            }
            MouseModifierBehavior::Unknown {
                context,
                button_input,
                behavior_id,
            } => MouseModifierBehavior::Unknown {
                context: *context,
                button_input: *button_input,
                behavior_id: *behavior_id,
            },
        }
    }
}

impl std::fmt::Debug for MouseModifierBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MouseModifierBehavior::Known(behavior) => f
                .debug_struct("MouseModifierBehavior::Known")
                .field("behavior_id", &behavior.behavior_id())
                .field("display_name", &behavior.display_name())
                .finish(),
            MouseModifierBehavior::Unknown {
                context,
                button_input,
                behavior_id,
            } => f
                .debug_struct("MouseModifierBehavior::Unknown")
                .field("context", context)
                .field("button_input", button_input)
                .field("behavior_id", behavior_id)
                .finish(),
        }
    }
}

impl MouseModifierBehavior {
    /// Get the display name for this behavior
    pub fn display_name(&self) -> String {
        match self {
            MouseModifierBehavior::Known(behavior) => behavior.display_name().to_string(),
            MouseModifierBehavior::Unknown { behavior_id, .. } => {
                format!("Action {} m", behavior_id)
            }
        }
    }

    /// Get the behavior ID
    pub fn behavior_id(&self) -> u32 {
        match self {
            MouseModifierBehavior::Known(behavior) => behavior.behavior_id(),
            MouseModifierBehavior::Unknown { behavior_id, .. } => *behavior_id,
        }
    }

    /// Get the behavior ID string format (e.g., "1 m", "13 m")
    pub fn behavior_id_string(&self) -> String {
        match self {
            MouseModifierBehavior::Known(behavior) => behavior.behavior_id_string(),
            MouseModifierBehavior::Unknown { behavior_id, .. } => {
                format!("{} m", behavior_id)
            }
        }
    }
}
