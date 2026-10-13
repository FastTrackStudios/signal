//! Shared types and utilities for mouse modifier behaviors

pub mod behavior;
pub mod conversion;
pub mod macros;
pub mod traits;

// Re-export everything for convenience
pub use behavior::MouseModifierBehavior;
pub use conversion::{get_behavior, get_mouse_modifier_name};
pub use traits::{BehaviorDisplay, BehaviorId, MouseBehavior};
