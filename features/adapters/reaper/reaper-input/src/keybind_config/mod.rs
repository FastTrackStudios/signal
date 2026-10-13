//! Config-file-driven keybind system.
//!
//! Loads keybind profiles from `.styx` files on disk and converts them into
//! the runtime [`KeybindPreset`](crate::input::keybinds::KeybindPreset) format.
//!
//! # Directory layout (installed at `~/.config/REAPER/FTS/keybinds/`)
//!
//! ```text
//! keybinds/
//! ├── fasttrackstudio/
//! │   ├── profile.styx
//! │   ├── transport.styx
//! │   └── ...
//! ├── logic/
//! │   └── ...
//! ├── reaper/
//! │   └── ...
//! └── overlays/
//!     └── tempo-map.styx
//! ```

pub mod composer;
pub mod editor;
pub mod host;
pub mod loader;
pub mod types;
pub mod undo;
pub mod workflow_editor;

pub use loader::{load_mouse_overlay, load_overlay, load_profile_preset, load_workflow_config};
pub use types::{
    KeybindDef, MouseBindDef, MouseModifierSettingDef, MouseProfileConfig, OverlayConfig,
    ProfileConfig, ReaperSettingDef, SectionConfig, WheelBindDef, WhichKeyEntryDef,
    WhichKeyTreeDef, WorkflowConfig,
};
