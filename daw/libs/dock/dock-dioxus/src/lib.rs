// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Dock Dioxus — modular docking layout UI components.
//!
//! Provides reactive components for rendering and manipulating dock layouts
//! defined by [`dock_proto`].

pub mod components;
pub mod context;
pub mod drag_ghost;
pub mod hooks;
pub mod panel_registry;
pub mod prelude;
pub mod signals;
pub mod transition;

// Re-export public API
pub use components::{DockRoot, DockTabBar, PanelHeader, PresetBar, SplitPane, TilePane};
pub use context::{DockContext, DockProvider, PanelRenderer};
pub use hooks::{init_dock_presets, init_rig_dock, load_rig_preset, use_dock_actions, DockActions};
pub use panel_registry::PanelRendererRegistry;
pub use signals::*;
pub use transition::{
    compute_transition, LayoutTransition, PanelTransition, TransitionKind, TransitionRect,
};
