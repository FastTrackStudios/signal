//! FTS-Input: Key Sequence System
//!
//! Provides a vim-like key sequence system for REAPER, similar to reaper-keys.
//! Supports:
//! - Key sequence building (e.g., "tL" = play and loop next measure)
//! - Action composition (timeline_operator + timeline_motion)
//! - Multiple modes (normal, visual_timeline, visual_track)
//! - Context awareness (main, midi, global)
//! - Macro recording and playback

// region: --- Modules

pub mod actions;
pub mod bindings;
pub mod constants;
pub mod error;
pub mod event_parser;
pub mod executor;
pub mod handler;
pub mod keybinds;
pub mod matcher;
pub mod midi_utils;
pub mod mouse_context;
pub mod mouse_modifiers;
pub mod mouse_util;
pub mod processor;
pub mod reaper_windows;
pub mod state;
pub mod status_panel;
pub mod utils;
pub mod wheel_hook;
pub mod which_key_component;
pub mod which_key_overlay;
pub mod window_detection;
pub mod workflows;

// endregion: --- Modules

pub use handler::InputHandler;
pub use keybinds::{
    ActionSet, BindingConflict, ConflictType, Keybind, KeybindContext, KeybindOverride,
    KeybindPreset, MouseModifier, MouseModifierContext, PresetBuilder, WheelBind, WheelDirection,
};
pub use state::{CommandState, Context, Mode};
