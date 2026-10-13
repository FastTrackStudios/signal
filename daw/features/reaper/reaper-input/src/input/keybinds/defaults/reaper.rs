//! REAPER Default Keybind Preset
//!
//! Matches REAPER's default keybindings for users who want familiar controls.
//! Includes all standard REAPER behaviors:
//! - Fade hotspots in item corners
//! - Standard Ctrl/Cmd+wheel zoom
//! - Default keyboard shortcuts
//! - Full track/item/view controls
//! - MIDI Editor shortcuts

use crate::input::keybinds::sections::{
    ReaperEditing, ReaperMidiEditor, ReaperMouseModifiers, ReaperNavigation, ReaperScrolling,
    ReaperTransport, ReaperViews,
};
use crate::input::keybinds::{KeybindPreset, PresetBuilder};

/// Create the REAPER default preset using composable sections
pub fn reaper_preset() -> KeybindPreset {
    PresetBuilder::new(
        "reaper",
        "REAPER default keybindings with all standard behaviors",
    )
    .version("1.0.0")
    // Compose from REAPER-style sections
    .with_section(ReaperNavigation)
    .with_section(ReaperTransport)
    .with_section(ReaperEditing)
    .with_section(ReaperScrolling)
    .with_section(ReaperViews)
    .with_section(ReaperMidiEditor)
    .with_section(ReaperMouseModifiers) // Full fade corners and edge behaviors
    .build()
}
