//! Type definitions for the collection browser.

use signal::metadata::Metadata as MetadataModel;
use signal::rig::RigType;
use signal::tagging::{TagCategory, TagSet};
use signal::SignalChain;

// region: --- Navigation & Sort

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavCategory {
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

    pub fn accent(self) -> &'static str {
        match self {
            Self::Presets => "from-amber-500 via-orange-400 to-red-500",
            Self::Engines => "from-rose-500 via-pink-400 to-fuchsia-500",
            Self::Layers => "from-emerald-500 via-teal-400 to-cyan-500",
            Self::Modules => "from-blue-500 via-indigo-400 to-violet-500",
            Self::Blocks => "from-orange-500 via-amber-400 to-yellow-500",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SortMode {
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
pub(super) struct ColumnItem {
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
pub(super) struct DetailParam {
    pub name: String,
    pub value: f32,
}

/// A module's signal chain data for grid rendering.
#[derive(Clone, PartialEq)]
pub struct ModuleChainData {
    pub name: String,
    pub color_bg: String,
    pub color_fg: String,
    pub color_border: String,
    pub chain: SignalChain,
    pub module_type: Option<signal::ModuleType>,
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

/// Nested detail data for the detail panel.
#[derive(Clone, PartialEq, Default)]
pub(super) struct DetailData {
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

pub(super) const RIG_TYPES: &[RigType] = &[
    RigType::Guitar,
    RigType::Bass,
    RigType::Keys,
    RigType::Drums,
    RigType::DrumEnhancement,
    RigType::Vocals,
];

/// The filterable tag categories shown as chip filters in the toolbar.
pub(super) const FILTER_CATEGORIES: &[TagCategory] = &[
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
