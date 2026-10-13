//! Dock Protocol — domain types for modular layout/docking system.
//!
//! Pure domain types with facet serialization. Zero UI dependencies.
//!
//! - Layout tree: binary splits + tile leaves with tab groups
//! - Panel registry: known panel types
//! - Presets: named layout snapshots (screensets)
//! - Drop zones: drag-and-drop panel rearrangement
//! - Builder: fluent API for constructing layouts
//! - Defaults: built-in screenset presets
//! - Persistence: JSON save/load for presets and layouts

pub mod builder;
pub mod context;
pub mod defaults;
pub mod diff;
pub mod drop_zone;
pub mod history;
pub mod id;
pub mod layout;
pub mod panel;
pub mod persistence;
pub mod preset;
pub mod registry;
pub mod tab_group;
pub mod tree;
pub mod workspace;

// Re-export core types at crate root
pub use builder::DockLayoutBuilder;
pub use context::{ActionContext, WhenExpr};
pub use defaults::{default_presets, rig_presets};
pub use diff::{LayoutDiff, LayoutOp};
pub use drop_zone::DropZone;
pub use history::LayoutHistory;
pub use id::*;
pub use layout::{DockLayout, DockZoneMode, FlatNode, LayoutRegion};
pub use panel::PanelId;
pub use persistence::{
    load_presets_from_file, save_presets_to_file, workspace_from_json, workspace_to_json,
};
pub use preset::{DockPreset, PresetCollection};
pub use registry::{DockPosition, PanelConstraints, PanelDescriptor, PanelRegistry};
pub use tab_group::TabGroup;
pub use tree::{DockNode, SplitDirection};
pub use workspace::{DockWindow, DockWorkspace, DockWorkspaceState, WindowBounds};
