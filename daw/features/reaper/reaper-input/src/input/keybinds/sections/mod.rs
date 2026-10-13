//! Composable Action Set Sections
//!
//! Each section defines a group of related keybindings that can be mixed and matched
//! to create custom presets. Sections implement the [`ActionSet`] trait.
//!
//! ## Available Sections
//!
//! - [`scrolling`] - Scroll and zoom behaviors (mouse wheel, keyboard zoom)
//! - [`transport`] - Play, stop, record, navigation controls
//! - [`navigation`] - Cursor and track movement
//! - [`mouse_modifiers`] - Click+drag behaviors (edge resize, fades, etc.)
//! - [`editing`] - Cut, copy, paste, split, delete
//! - [`markers_regions`] - Marker and region editing shortcuts
//! - [`views`] - Window management, views, automation controls
//! - [`midi_editor`] - MIDI Editor specific keybindings

pub mod editing;
pub mod markers_regions;
pub mod midi_editor;
pub mod mouse_modifiers;
pub mod navigation;
pub mod scrolling;
pub mod transport;
pub mod views;

pub use editing::*;
pub use midi_editor::*;
pub use mouse_modifiers::*;
pub use navigation::*;
pub use scrolling::*;
pub use transport::*;
pub use views::*;
