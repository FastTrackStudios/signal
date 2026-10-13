//! Logic Pro Keybind Preset
//!
//! Mimics Logic Pro's keybindings for users transitioning from Logic.
//! Key differences from REAPER defaults:
//! - No fade hotspots in item corners (cleaner edge behavior)
//! - Option+wheel for zoom
//! - Logic-style transport and navigation
//! - Numpad for transport and markers
//! - Logic-style MIDI Editor shortcuts

use crate::input::keybinds::sections::{
    LogicEditing, LogicMidiEditor, LogicMouseModifiers, LogicNavigation, LogicScrolling,
    LogicTransport, LogicViews,
};
use crate::input::keybinds::{KeybindPreset, PresetBuilder};

/// Create the Logic Pro preset using composable sections
pub fn logic_preset() -> KeybindPreset {
    PresetBuilder::new("logic", "Logic Pro-style keybindings")
        .version("1.0.0")
        // Compose from Logic-style sections
        // NOTE: LogicMouseModifiers removes fade corners from item edges
        .with_section(LogicNavigation)
        .with_section(LogicTransport)
        .with_section(LogicEditing)
        .with_section(LogicScrolling)
        .with_section(LogicViews)
        .with_section(LogicMidiEditor)
        .with_section(LogicMouseModifiers) // No fade corners!
        .build()
}
