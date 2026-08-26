//! Type definitions for the collection browser.

use signal_proto::metadata::Metadata as MetadataModel;
use signal_proto::rig::RigType;
use signal_proto::tagging::{TagCategory, TagSet};
use signal_proto::SignalChain;

// region: --- Navigation & Sort

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavCategory {
    Presets,
    Engines,
    Layers,
    Modules,
    Blocks,
}

impl NavCategory {
    pub const ALL: &[NavCategory] = &[
        Self::Presets,
        Self::Engines,
        Self::Layers,
        Self::Modules,
        Self::Blocks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Presets => "Presets",
            Self::Engines => "Engines",
            Self::Layers => "Layers",
            Self::Modules => "Modules",
            Self::Blocks => "Blocks",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Name,
    NameDesc,
    Variants,
    BlockType,
}

impl SortMode {
    pub const ALL: &[SortMode] = &[Self::Name, Self::NameDesc, Self::Variants, Self::BlockType];

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "A \u{2192} Z",
            Self::NameDesc => "Z \u{2192} A",
            Self::Variants => "Most Variants",
            Self::BlockType => "Type",
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::NameDesc => "name_desc",
            Self::Variants => "variants",
            Self::BlockType => "block_type",
        }
    }

    pub fn from_value(s: &str) -> Self {
        match s {
            "name_desc" => Self::NameDesc,
            "variants" => Self::Variants,
            "block_type" => Self::BlockType,
            _ => Self::Name,
        }
    }
}

// endregion: --- Navigation & Sort

// region: --- Column & Detail

#[derive(Clone, PartialEq)]
pub struct ColumnItem {
    pub id: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub badge: Option<String>,
    pub metadata: Option<MetadataModel>,
    /// Structured tags for filtering/sorting.
    pub structured_tags: TagSet,
    /// Nested detail data (params, blocks, modules) for the detail panel.
    pub detail: DetailData,
    /// Extra data for context (e.g. block type index for Blocks nav).
    pub tag: Option<usize>,
    /// Folder path for grouping (e.g. "Guitar", "Drums/Kick").
    pub folder: Option<String>,
}

#[derive(Clone, PartialEq)]
pub struct DetailParam {
    pub name: String,
    pub value: f32,
}

#[derive(Clone, PartialEq, Default)]
pub struct DetailData {
    /// Standalone parameters (block snapshots).
    pub params: Vec<DetailParam>,
    /// Raw signal chain for grid rendering (module snapshots).
    pub chain: Option<SignalChain>,
    /// Module chains for layer/engine detail.
    pub module_chains: Vec<ModuleChainData>,
    /// Full rig hierarchy (engines → layers → modules) for preset detail.
    pub engines: Vec<EngineFlowData>,
}

// endregion: --- Column & Detail

// region: --- Constants

pub const RIG_TYPES: &[RigType] = &[
    RigType::Guitar,
    RigType::Bass,
    RigType::Keys,
    RigType::Drums,
    RigType::DrumEnhancement,
    RigType::Vocals,
];

/// The filterable tag categories shown as chip filters in the toolbar.
pub const FILTER_CATEGORIES: &[TagCategory] = &[
    TagCategory::Tone,
    TagCategory::Character,
    TagCategory::Genre,
    TagCategory::Vendor,
    TagCategory::Plugin,
    TagCategory::Context,
    TagCategory::Instrument,
    TagCategory::Block,
    TagCategory::Module,
    TagCategory::Workflow,
];

// endregion: --- Constants

// GridSlot + flow-data types moved to the wasm-clean `signal-grid` crate.
pub use signal_grid::{EngineFlowData, GridSlot, LayerFlowData, ModuleChainData};
