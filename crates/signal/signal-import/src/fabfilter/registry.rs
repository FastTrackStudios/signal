//! Registry of all known FabFilter plugins with their block type mappings.
//!
//! Each entry maps a plugin name to its Signal `BlockType` and whether its
//! preset files use the text-parseable INI format or binary format.

use signal_proto::block::BlockType;

/// Whether a plugin's `.ffp` files are text (INI) or binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfpFormat {
    Text,
    Binary,
}

/// A known FabFilter plugin.
#[derive(Debug, Clone)]
pub struct FabFilterPluginEntry {
    /// Plugin display name as it appears in the preset directory.
    pub name: &'static str,
    /// Signal block type this plugin maps to.
    pub block_type: BlockType,
    /// Preset file format.
    pub format: FfpFormat,
    /// 4-character signature found in text presets (informational for binary).
    pub signature: &'static str,
}

/// All known FabFilter plugins. Directory names under `~/Documents/FabFilter/Presets/`
/// match these names exactly.
pub const FABFILTER_PLUGINS: &[FabFilterPluginEntry] = &[
    // ─── Text-format plugins (INI .ffp) ──────────────────────────
    FabFilterPluginEntry {
        name: "Pro-Q 4",
        block_type: BlockType::Eq,
        format: FfpFormat::Text,
        signature: "FFPQ",
    },
    FabFilterPluginEntry {
        name: "Pro-C 3",
        block_type: BlockType::Compressor,
        format: FfpFormat::Text,
        signature: "FFPC",
    },
    // ─── Binary-format plugins ───────────────────────────────────
    FabFilterPluginEntry {
        name: "Pro-Q 3",
        block_type: BlockType::Eq,
        format: FfpFormat::Binary,
        signature: "FQ3p",
    },
    FabFilterPluginEntry {
        name: "Pro-Q 2",
        block_type: BlockType::Eq,
        format: FfpFormat::Binary,
        signature: "FQ2p",
    },
    FabFilterPluginEntry {
        name: "Pro-C 2",
        block_type: BlockType::Compressor,
        format: FfpFormat::Binary,
        signature: "FC2p",
    },
    FabFilterPluginEntry {
        name: "Pro-R 2",
        block_type: BlockType::Reverb,
        format: FfpFormat::Binary,
        signature: "FR2p",
    },
    FabFilterPluginEntry {
        name: "Pro-R",
        block_type: BlockType::Reverb,
        format: FfpFormat::Binary,
        signature: "FRvb",
    },
    FabFilterPluginEntry {
        name: "Pro-L 2",
        block_type: BlockType::Limiter,
        format: FfpFormat::Binary,
        signature: "FL2p",
    },
    FabFilterPluginEntry {
        name: "Pro-L",
        block_type: BlockType::Limiter,
        format: FfpFormat::Binary,
        signature: "FLim",
    },
    FabFilterPluginEntry {
        name: "Pro-G",
        block_type: BlockType::Gate,
        format: FfpFormat::Binary,
        signature: "FGat",
    },
    FabFilterPluginEntry {
        name: "Pro-DS",
        block_type: BlockType::DeEsser,
        format: FfpFormat::Binary,
        signature: "FDSp",
    },
    FabFilterPluginEntry {
        name: "Pro-MB",
        block_type: BlockType::Compressor,
        format: FfpFormat::Binary,
        signature: "FMBp",
    },
    FabFilterPluginEntry {
        name: "Saturn 2",
        block_type: BlockType::Saturator,
        format: FfpFormat::Binary,
        signature: "FS2p",
    },
    FabFilterPluginEntry {
        name: "Saturn",
        block_type: BlockType::Saturator,
        format: FfpFormat::Binary,
        signature: "FSat",
    },
    FabFilterPluginEntry {
        name: "Timeless 3",
        block_type: BlockType::Delay,
        format: FfpFormat::Binary,
        signature: "FT3p",
    },
    FabFilterPluginEntry {
        name: "Timeless 2",
        block_type: BlockType::Delay,
        format: FfpFormat::Binary,
        signature: "FT2p",
    },
    FabFilterPluginEntry {
        name: "Volcano 3",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FV3p",
    },
    FabFilterPluginEntry {
        name: "Volcano 2",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FV2p",
    },
    FabFilterPluginEntry {
        name: "Twin 3",
        block_type: BlockType::Custom,
        format: FfpFormat::Binary,
        signature: "FW3p",
    },
    FabFilterPluginEntry {
        name: "Twin 2",
        block_type: BlockType::Custom,
        format: FfpFormat::Binary,
        signature: "FW2p",
    },
    FabFilterPluginEntry {
        name: "Micro",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FMic",
    },
    FabFilterPluginEntry {
        name: "Simplon",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FSim",
    },
    FabFilterPluginEntry {
        name: "One",
        block_type: BlockType::Custom,
        format: FfpFormat::Binary,
        signature: "FOne",
    },
];

/// Look up a plugin by name (case-insensitive).
pub fn lookup_plugin(name: &str) -> Option<&'static FabFilterPluginEntry> {
    FABFILTER_PLUGINS
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}
