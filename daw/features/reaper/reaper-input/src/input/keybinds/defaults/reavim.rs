//! ReaVim Keybind Preset
//!
//! Vim-style modal keybindings for REAPER.
//! Placeholder for future full modal implementation.
//!
//! Future plans:
//! - Normal mode: Navigation and commands
//! - Visual mode: Selection operations
//! - Command mode: Extended commands via `:` prefix

use crate::input::keybinds::{Keybind, KeybindPreset};

/// Create the ReaVim preset (placeholder)
pub fn reavim_preset() -> KeybindPreset {
    KeybindPreset::new(
        "reavim",
        "Vim-style modal keybindings (placeholder - full implementation coming)",
    )
    .with_version("0.1.0")
    .with_bindings(vec![
        // Basic vim-style navigation (non-modal for now)
        Keybind::new("h", "40104").with_description("Move cursor left"),
        Keybind::new("j", "40285").with_description("Select next track"),
        Keybind::new("k", "40286").with_description("Select previous track"),
        Keybind::new("l", "40105").with_description("Move cursor right"),
        // Vim-style commands
        Keybind::new("gg", "40042").with_description("Go to start of project"),
        Keybind::new("<S-g>", "40043").with_description("Go to end of project"),
        Keybind::new("w", "40105").with_description("Move to next word/item"),
        Keybind::new("b", "40104").with_description("Move to previous word/item"),
        // TODO: Full modal system
        // - Track current mode (Normal, Visual, Insert)
        // - Mode-specific bindings
        // - Status line mode indicator
        // - Motion + operator composition (d$, y$, etc.)
    ])
}
