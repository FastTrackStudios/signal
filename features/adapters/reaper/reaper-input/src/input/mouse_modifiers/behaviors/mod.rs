//! Typesafe mouse modifier behaviors
//!
//! This module provides typesafe enums for all mouse modifier behaviors/actions
//! that can be assigned in REAPER. Each context has its own set of behaviors,
//! allowing for compile-time type safety and pattern matching.
//!
//! # Structure
//!
//! The module is organized by context, with each context in its own file:
//! - `arrange_view.rs` - Arrange View behaviors
//! - `automation_item.rs` - Automation Item behaviors
//! - `cursor_handle.rs` - Cursor Handle behaviors
//! - `envelope.rs` - Envelope behaviors
//! - `fixed_lane.rs` - Fixed Lane behaviors
//! - `media_item/` - Media Item behaviors (organized by interaction type)
//! - `midi/` - MIDI Editor behaviors (organized by interaction type)
//! - `mixer.rs` - Mixer behaviors
//! - `project.rs` - Project behaviors
//! - `razor_edit.rs` - Razor Edit behaviors
//! - `ruler.rs` - Ruler behaviors
//! - `track.rs` - Track behaviors
//! - `shared.rs` - Shared macro and wrapper enum

pub mod shared {
    pub mod behavior;
    pub mod conversion;
    pub mod macros;
    pub mod traits;
}
pub mod arrange_view;
pub mod automation_item;
pub mod cursor_handle;
pub mod envelope;
pub mod fixed_lane;
pub mod media_item;
pub mod midi;
pub mod mixer;
pub mod project;
pub mod razor_edit;
pub mod ruler;
pub mod track;

// Re-export the main types
pub use shared::behavior::MouseModifierBehavior;
pub use shared::conversion::{get_behavior, get_mouse_modifier_name};
pub use shared::traits::{BehaviorDisplay, BehaviorId, MouseBehavior};

// Re-export all behavior enums for convenience
pub use arrange_view::*;
pub use automation_item::*;
pub use cursor_handle::*;
pub use envelope::*;
pub use fixed_lane::*;
pub use media_item::*;
pub use midi::*;
pub use mixer::*;
pub use project::*;
pub use razor_edit::*;
pub use ruler::*;
pub use track::*;
