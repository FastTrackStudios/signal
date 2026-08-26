//! The zoomable/pannable rig graph — `DynamicGridView` + `RigGridPanel` —
//! extracted from signal-ui as a wasm-clean crate (see docs/detachable-gui.md).
//!
//! Depends only on the proto layer (`signal-proto`, `signal-grid`) and
//! Dioxus component libraries; renders identically in the desktop shell and
//! the browser remote. Timing uses `architect::platform::sleep` (tokio
//! native / browser timers on wasm).

pub mod dynamic_grid;
pub mod icons;
mod inspector;
mod knob;
mod panel;

pub use icons::{block_icon, module_icon};
pub use inspector::BlockInspectorPanel;
pub use knob::{Knob, KnobSize};
pub use panel::RigGridPanel;
