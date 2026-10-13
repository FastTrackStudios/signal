//! input-config-proto — wire contract for keybind/workflow configuration.
//!
//! The facet/styx config data model (profiles, sections, overlays,
//! workflows) plus the `InputConfigService` vox trait for editing it over
//! RPC. wasm-clean: web editors, the fasttrackstudio.app site, and the
//! community hub depend on this crate; only hosts link reaper-input.

pub mod context;
pub mod service;
pub mod types;

pub use context::{KeybindContext, MouseModifierContext};
pub use service::{InputConfigService, OverlayInfo, ProfileInfo, WorkflowInfo, WriteResult};
pub use types::*;
