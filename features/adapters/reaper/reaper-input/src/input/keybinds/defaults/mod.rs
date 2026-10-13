//! Default Keybind Presets and Override Layers
//!
//! Built-in keybind configurations that ship with the extension.
//!
//! Presets are composed from sections in [`super::sections`].
//!
//! Note: The `fasttrackstudio` preset is config-driven and loaded from
//! `config/fasttrackstudio/` at runtime. Only Reaper and Logic have
//! compiled-in presets here.

mod fast_slip_edit;
mod logic;
mod reaper;
mod tempo_map;

pub use fast_slip_edit::fast_slip_edit_override;
pub use logic::logic_preset;
pub use reaper::reaper_preset;
pub use tempo_map::tempo_map_override;

use super::{KeybindOverride, KeybindPreset};

/// All built-in presets
pub static ALL_PRESETS: &[fn() -> KeybindPreset] = &[reaper_preset, logic_preset];

/// All built-in override layers
pub static ALL_OVERRIDES: &[fn() -> KeybindOverride] =
    &[tempo_map_override, fast_slip_edit_override];
