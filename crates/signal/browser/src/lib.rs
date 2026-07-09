//! Headless data layer for the collection browser.
//!
//! This crate contains the type definitions, async data fetching, and
//! grid-conversion logic for browsing the signal domain (presets, engines,
//! layers, modules, blocks). It has zero Dioxus dependency, so the same
//! logic can drive the desktop Dioxus UI, a TUI, the CLI, or any future
//! frontend.
//!
//! Rendering (the `RigGridPanel` component, etc.) lives in `signal-ui`
//! and consumes the headless types defined here.

pub mod data_fetching;
pub mod grid_conversion;
pub mod pack_registry;
pub mod types;

// Public surface used by signal-ui (mirrors what the old
// `views/collection_browser/mod.rs` re-exported).
pub use data_fetching::rig_type_to_engine_type;
pub use grid_conversion::{
    ParamLookup, engines_to_grid_slots, module_chains_to_grid_slots, signal_chain_to_grid_slots,
};
pub use types::{
    ColumnItem, DetailData, DetailParam, EngineFlowData, FILTER_CATEGORIES, GridSlot,
    LayerFlowData, ModuleChainData, NavCategory, RIG_TYPES, SortMode,
};

/// Re-export of the project-wide tagging schema used throughout this crate.
pub use signal::tagging::{StructuredTag, TagCategory, TagSet};

/// Alias matching the legacy `EngineParamLookup` name some consumers use.
pub use grid_conversion::ParamLookup as EngineParamLookup;
