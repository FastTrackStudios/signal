//! Headless grid view-model — `GridSlot` + friends, pure data.
//!
//! Extracted from `signal-browser` so the grid UI (`signal-grid-ui`) can
//! run on wasm: this crate depends only on `signal-proto`.

use signal_proto::module_type::ModuleType;
use signal_proto::signal_chain::SignalChain;

pub mod conversion;

/// A module's signal chain data for grid rendering.
#[derive(Clone, PartialEq)]
pub struct ModuleChainData {
    pub name: String,
    pub color_bg: String,
    pub color_fg: String,
    pub color_border: String,
    pub chain: SignalChain,
    pub module_type: Option<ModuleType>,
}

/// A layer's resolved module chains for rig-level display.
#[derive(Clone, PartialEq)]
pub struct LayerFlowData {
    pub name: String,
    pub module_chains: Vec<ModuleChainData>,
}

/// An engine's resolved layer data for rig-level display.
#[derive(Clone, PartialEq)]
pub struct EngineFlowData {
    pub name: String,
    pub layers: Vec<LayerFlowData>,
}

// region: --- GridSlot (headless data type)

/// A single cell in the grid. Pure data — rendered by signal-ui's
/// `DynamicGridView`, but built up by the headless conversion functions
/// in `grid_conversion`.
#[derive(Debug, Clone, PartialEq)]
pub struct GridSlot {
    pub id: uuid::Uuid,
    pub block_type: signal_proto::block::BlockType,
    pub block_preset_name: Option<String>,
    pub plugin_name: Option<String>,
    /// Grid column position (0-indexed).
    pub col: usize,
    /// Grid row position (0-indexed).
    pub row: usize,
    /// Module group key — slots with the same key are grouped visually.
    pub module_group: Option<String>,
    /// Module type for coloring the group container.
    pub module_type: Option<ModuleType>,
    /// Layer group key — modules within the same layer share this key.
    pub layer_group: Option<String>,
    /// Engine group key — layers within the same engine share this key.
    pub engine_group: Option<String>,
    /// True when the block has no plugin loaded yet (template placeholder).
    pub is_template: bool,
    /// True when the block is bypassed (signal passes through unprocessed).
    pub bypassed: bool,
    /// Phantom slot — participates in layout (group bounds, grid sizing)
    /// but does not render a visible cell. Used for dry pass-through lanes.
    pub is_phantom: bool,
    /// Resolved block parameters (name, value 0..1) for the inspector panel.
    pub parameters: Vec<(String, f32)>,
    /// Preset ID this block was loaded from (for save-back). `None` for inline/template blocks.
    pub preset_id: Option<String>,
    /// Snapshot ID this block was loaded from. `None` for default snapshots or inline blocks.
    pub snapshot_id: Option<String>,
}

// endregion: --- GridSlot
