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
    /// Full REAPER plugin identifier, e.g. `"CLAP: Pro-Q 4 (FabFilter)"`.
    /// Used for `source:` tags, "Add to FX Chain", and dedup matching.
    /// Note: CLAP names in REAPER use the short display name (no vendor prefix),
    /// e.g. `"CLAP: Pro-Q 4 (FabFilter)"` not `"CLAP: FabFilter Pro-Q 4 (FabFilter)"`.
    pub reaper_name: &'static str,
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
        reaper_name: "CLAP: Pro-Q 4 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-C 3",
        block_type: BlockType::Compressor,
        format: FfpFormat::Text,
        signature: "FFPC",
        reaper_name: "CLAP: Pro-C 3 (FabFilter)",
    },
    // ─── Binary-format plugins ───────────────────────────────────
    FabFilterPluginEntry {
        name: "Pro-Q 3",
        block_type: BlockType::Eq,
        format: FfpFormat::Binary,
        signature: "FQ3p",
        reaper_name: "CLAP: Pro-Q 3 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-Q 2",
        block_type: BlockType::Eq,
        format: FfpFormat::Binary,
        signature: "FQ2p",
        reaper_name: "CLAP: Pro-Q 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-C 2",
        block_type: BlockType::Compressor,
        format: FfpFormat::Binary,
        signature: "FC2p",
        reaper_name: "CLAP: Pro-C 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-R 2",
        block_type: BlockType::Reverb,
        format: FfpFormat::Binary,
        signature: "FR2p",
        reaper_name: "CLAP: Pro-R 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-R",
        block_type: BlockType::Reverb,
        format: FfpFormat::Binary,
        signature: "FRvb",
        reaper_name: "CLAP: Pro-R (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-L 2",
        block_type: BlockType::Limiter,
        format: FfpFormat::Binary,
        signature: "FL2p",
        reaper_name: "CLAP: Pro-L 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-L",
        block_type: BlockType::Limiter,
        format: FfpFormat::Binary,
        signature: "FLim",
        reaper_name: "CLAP: Pro-L (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-G",
        block_type: BlockType::Gate,
        format: FfpFormat::Binary,
        signature: "FGat",
        reaper_name: "CLAP: Pro-G (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-DS",
        block_type: BlockType::DeEsser,
        format: FfpFormat::Binary,
        signature: "FDSp",
        reaper_name: "CLAP: Pro-DS (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Pro-MB",
        block_type: BlockType::Compressor,
        format: FfpFormat::Binary,
        signature: "FMBp",
        reaper_name: "CLAP: Pro-MB (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Saturn 2",
        block_type: BlockType::Saturator,
        format: FfpFormat::Binary,
        signature: "FS2p",
        reaper_name: "CLAP: Saturn 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Saturn",
        block_type: BlockType::Saturator,
        format: FfpFormat::Binary,
        signature: "FSat",
        reaper_name: "CLAP: Saturn (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Timeless 3",
        block_type: BlockType::Delay,
        format: FfpFormat::Binary,
        signature: "FT3p",
        reaper_name: "CLAP: Timeless 3 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Timeless 2",
        block_type: BlockType::Delay,
        format: FfpFormat::Binary,
        signature: "FT2p",
        reaper_name: "CLAP: Timeless 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Volcano 3",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FV3p",
        reaper_name: "CLAP: Volcano 3 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Volcano 2",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FV2p",
        reaper_name: "CLAP: Volcano 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Twin 3",
        block_type: BlockType::Custom,
        format: FfpFormat::Binary,
        signature: "FW3p",
        reaper_name: "CLAP: Twin 3 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Twin 2",
        block_type: BlockType::Custom,
        format: FfpFormat::Binary,
        signature: "FW2p",
        reaper_name: "CLAP: Twin 2 (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Micro",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FMic",
        reaper_name: "CLAP: Micro (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "Simplon",
        block_type: BlockType::Filter,
        format: FfpFormat::Binary,
        signature: "FSim",
        reaper_name: "CLAP: Simplon (FabFilter)",
    },
    FabFilterPluginEntry {
        name: "One",
        block_type: BlockType::Custom,
        format: FfpFormat::Binary,
        signature: "FOne",
        reaper_name: "CLAP: One (FabFilter)",
    },
];

/// Look up a plugin by name (case-insensitive).
pub fn lookup_plugin(name: &str) -> Option<&'static FabFilterPluginEntry> {
    FABFILTER_PLUGINS
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

/// Look up a plugin by its full REAPER identifier (case-insensitive).
///
/// Matches against the `reaper_name` field, e.g. `"CLAP: Pro-Q 4 (FabFilter)"`.
pub fn lookup_by_reaper_name(reaper_name: &str) -> Option<&'static FabFilterPluginEntry> {
    FABFILTER_PLUGINS
        .iter()
        .find(|p| p.reaper_name.eq_ignore_ascii_case(reaper_name))
}
