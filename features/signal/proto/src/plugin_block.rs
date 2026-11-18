//! Plugin block definitions — maps a single DAW plugin's parameters
//! into virtual modules and blocks for UI organization.
//!
//! A [`PluginBlockDef`] is a self-contained JSON document that describes:
//! - The plugin identity (name, vendor, parameter count)
//! - Virtual modules grouping parameter subsets
//! - Virtual blocks within each module
//! - Parameter mappings from virtual block params to plugin param indices
//!
//! Plugin block defs are NOT stored in the database. They are embedded
//! inline in [`LayerSnapshot`](crate::layer::LayerSnapshot) as JSON, or
//! saved/loaded as standalone `.json` files.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::{Block, BlockParameter, BlockType, ModuleBlock, ModuleBlockSource, ModuleType};
use crate::{SignalChain, SignalNode};

// ─── ID ─────────────────────────────────────────────────────────

crate::typed_uuid_id!(
    /// Identifies a plugin block definition.
    PluginBlockDefId
);

// ─── ParamMapping ───────────────────────────────────────────────

/// Maps a virtual block parameter to a real plugin parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ParamMapping {
    /// Human-readable parameter name for UI display.
    pub name: String,
    /// Index into the real plugin's parameter array.
    pub plugin_param_index: u32,
    /// Default normalized value (0.0..1.0).
    pub default_value: f32,
    /// Which FX in a multi-FX block this param belongs to (0-based).
    ///
    /// For single-FX blocks this is always 0 (the default). For multi-FX
    /// blocks (e.g., an Amp Block backed by `amp_sim` + `cabinet_sim` in a
    /// REAPER Container), this indexes into the block's `linked_fx_indices`.
    #[serde(default)]
    pub fx_slot: u32,
}

impl ParamMapping {
    pub fn new(name: impl Into<String>, plugin_param_index: u32, default_value: f32) -> Self {
        Self {
            name: name.into(),
            plugin_param_index,
            default_value: default_value.clamp(0.0, 1.0),
            fx_slot: 0,
        }
    }

    /// Create a param mapping targeting a specific FX slot within a multi-FX block.
    pub fn new_multi_fx(
        name: impl Into<String>,
        plugin_param_index: u32,
        default_value: f32,
        fx_slot: u32,
    ) -> Self {
        Self {
            name: name.into(),
            plugin_param_index,
            default_value: default_value.clamp(0.0, 1.0),
            fx_slot,
        }
    }
}

// ─── VirtualBlock ───────────────────────────────────────────────

/// A virtual block within a virtual module — controls a subset of plugin params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct VirtualBlock {
    /// Unique ID within this plugin block def (e.g., "justa-boost").
    pub id: String,
    /// Display label (e.g., "Justa Boost").
    pub label: String,
    /// Block type for color/category in the grid.
    pub block_type: BlockType,
    /// Parameter mappings to the real plugin.
    pub params: Vec<ParamMapping>,
    /// Whether this block is currently enabled/active.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// FX indices this block spans in the DAW's FX chain.
    ///
    /// For a single-FX block, this is empty or contains one index.
    /// For a multi-FX block (e.g., an amp block backed by `amp_sim` + `cabinet_sim`
    /// inside a REAPER Container), this lists all linked FX indices.
    /// Each [`ParamMapping::fx_slot`] indexes into this array.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_fx_indices: Vec<u32>,
}

const fn default_true() -> bool {
    true
}

impl VirtualBlock {
    pub fn new(id: impl Into<String>, label: impl Into<String>, block_type: BlockType) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            block_type,
            params: Vec::new(),
            enabled: true,
            linked_fx_indices: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_param(mut self, mapping: ParamMapping) -> Self {
        self.params.push(mapping);
        self
    }

    #[must_use]
    pub fn with_params(mut self, mappings: Vec<ParamMapping>) -> Self {
        self.params.extend(mappings);
        self
    }

    /// Declare that this block spans multiple FX in the DAW chain.
    ///
    /// Each index is a position in the track's FX chain. Param mappings
    /// use [`ParamMapping::fx_slot`] to reference into this array.
    #[must_use]
    pub fn with_linked_fx(mut self, indices: Vec<u32>) -> Self {
        self.linked_fx_indices = indices;
        self
    }

    /// Whether this block spans more than one FX.
    #[must_use]
    pub const fn is_multi_fx(&self) -> bool {
        self.linked_fx_indices.len() > 1
    }

    /// Resolve a param's `fx_slot` to an actual FX chain index.
    ///
    /// Returns `None` if `linked_fx_indices` is empty (single-FX block —
    /// the caller should use the block's default FX index).
    #[must_use]
    pub fn resolve_fx_index(&self, fx_slot: u32) -> Option<u32> {
        self.linked_fx_indices.get(fx_slot as usize).copied()
    }
}

// ─── VirtualModule ──────────────────────────────────────────────

/// A virtual module grouping virtual blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct VirtualModule {
    /// Unique ID within this plugin block def (e.g., "pedals").
    pub id: String,
    /// Display label (e.g., "Pedals").
    pub label: String,
    /// Module type for color/grouping in the grid.
    pub module_type: ModuleType,
    /// Ordered list of virtual blocks in this module.
    pub blocks: Vec<VirtualBlock>,
}

impl VirtualModule {
    pub fn new(id: impl Into<String>, label: impl Into<String>, module_type: ModuleType) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            module_type,
            blocks: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_block(mut self, block: VirtualBlock) -> Self {
        self.blocks.push(block);
        self
    }
}

// ─── PluginBlockDef ─────────────────────────────────────────────

/// Complete definition of how a single plugin maps to virtual modules/blocks.
///
/// This is the top-level JSON-serializable document. It is NOT stored in the
/// database — it lives as a JSON file or is embedded inline in a `LayerSnapshot`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct PluginBlockDef {
    /// Unique identifier for this definition.
    pub id: PluginBlockDefId,
    /// The real plugin name as reported by the DAW.
    pub plugin_name: String,
    /// Plugin vendor (e.g., "Neural DSP").
    pub vendor: Option<String>,
    /// Total parameter count of the real plugin (for validation).
    pub param_count: u32,
    /// Virtual modules organizing this plugin's parameters.
    pub modules: Vec<VirtualModule>,
    /// Schema version for forward compatibility.
    #[serde(default = "default_version")]
    pub version: u32,
}

const fn default_version() -> u32 {
    1
}

impl PluginBlockDef {
    pub fn new(plugin_name: impl Into<String>, param_count: u32) -> Self {
        Self {
            id: PluginBlockDefId::new(),
            plugin_name: plugin_name.into(),
            vendor: None,
            param_count,
            modules: Vec::new(),
            version: 1,
        }
    }

    #[must_use]
    pub fn with_vendor(mut self, vendor: impl Into<String>) -> Self {
        self.vendor = Some(vendor.into());
        self
    }

    #[must_use]
    pub fn with_module(mut self, module: VirtualModule) -> Self {
        self.modules.push(module);
        self
    }

    /// All virtual blocks across all modules, in order.
    #[must_use]
    pub fn all_blocks(&self) -> Vec<&VirtualBlock> {
        self.modules.iter().flat_map(|m| &m.blocks).collect()
    }

    /// Validate that no parameter index exceeds `param_count` and no index is mapped twice
    /// within the same FX slot.
    ///
    /// For multi-FX blocks, the same `plugin_param_index` is allowed across
    /// different `fx_slot` values (they target different plugins).
    ///
    /// # Errors
    ///
    /// Returns an error if a parameter index exceeds the valid range or if
    /// an index is mapped multiple times within the same FX slot.
    pub fn validate(&self) -> Result<(), PluginBlockDefError> {
        // Key: (fx_slot, plugin_param_index) — duplicates within the same FX are errors.
        let mut seen = std::collections::HashSet::new();
        for block in self.all_blocks() {
            for param in &block.params {
                if param.plugin_param_index >= self.param_count {
                    return Err(PluginBlockDefError::IndexOutOfRange {
                        block_id: block.id.clone(),
                        param_name: param.name.clone(),
                        index: param.plugin_param_index,
                        max: self.param_count,
                    });
                }
                let key = (param.fx_slot, param.plugin_param_index);
                if !seen.insert(key) {
                    return Err(PluginBlockDefError::DuplicateIndex {
                        index: param.plugin_param_index,
                        block_id: block.id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Convert this definition into `(label, module_type, SignalChain)` tuples
    /// suitable for the grid rendering pipeline.
    ///
    /// Each `VirtualModule` becomes a tuple. Each `VirtualBlock` becomes a
    /// `SignalNode::Block(Box<ModuleBlock>)` with `ModuleBlockSource::Inline`,
    /// carrying the virtual block's parameters as `BlockParameter`s.
    #[must_use]
    pub fn to_module_chains(&self) -> Vec<(String, ModuleType, SignalChain)> {
        self.modules
            .iter()
            .map(|vm| {
                let blocks: Vec<ModuleBlock> = vm
                    .blocks
                    .iter()
                    .map(|vb| {
                        let block = Block::from_parameters(
                            vb.params
                                .iter()
                                .map(|p| {
                                    BlockParameter::new(
                                        format!("p{}", p.plugin_param_index),
                                        &p.name,
                                        p.default_value,
                                    )
                                })
                                .collect(),
                        );
                        ModuleBlock::new(
                            &vb.id,
                            &vb.label,
                            vb.block_type,
                            ModuleBlockSource::Inline { block },
                        )
                    })
                    .collect();

                let nodes: Vec<SignalNode> = blocks
                    .into_iter()
                    .map(|b| SignalNode::Block(Box::new(b)))
                    .collect();
                let chain = SignalChain::new(nodes);
                (vm.label.clone(), vm.module_type, chain)
            })
            .collect()
    }
}

// ─── Errors ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginBlockDefError {
    IndexOutOfRange {
        block_id: String,
        param_name: String,
        index: u32,
        max: u32,
    },
    DuplicateIndex {
        index: u32,
        block_id: String,
    },
}

impl std::fmt::Display for PluginBlockDefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexOutOfRange {
                block_id,
                param_name,
                index,
                max,
            } => {
                write!(
                    f,
                    "param '{param_name}' in block '{block_id}' has index {index} >= max {max}"
                )
            }
            Self::DuplicateIndex { index, block_id } => {
                write!(
                    f,
                    "param index {index} mapped multiple times (found in block '{block_id}')"
                )
            }
        }
    }
}

impl std::error::Error for PluginBlockDefError {}

// ─── FX Name Formatting ─────────────────────────────────────────

/// Controls which parts of an FX display name are visible.
///
/// An FX display name has up to 4 parts:
/// ```text
/// DRIVE Module: Simple Drive - Low
/// │     │       │              │
/// │     │       │              └─ variation
/// │     │       └─ preset
/// │     └─ collection_type ("Module" or "Block")
/// └─ type_name ("DRIVE", "AMP", etc.)
/// ```
///
/// Default format: `Type: Preset - Variation` (e.g., `DRIVE: Simple Drive - Low`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxDisplayOptions {
    /// Show the type prefix (e.g., "DRIVE", "AMP").
    pub show_type: bool,
    /// Show the collection keyword ("Module" or "Block").
    pub show_collection_type: bool,
    /// Show the preset/collection name (e.g., "Simple Drive").
    pub show_preset: bool,
    /// Show the variation name (e.g., "Low").
    pub show_variation: bool,
}

impl Default for FxDisplayOptions {
    /// Default: `Type: Preset - Variation` (collection type hidden).
    fn default() -> Self {
        Self {
            show_type: true,
            show_collection_type: false,
            show_preset: true,
            show_variation: true,
        }
    }
}

impl FxDisplayOptions {
    /// Show everything: `DRIVE Module: Simple Drive - Low`.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            show_type: true,
            show_collection_type: true,
            show_preset: true,
            show_variation: true,
        }
    }

    /// Show only preset and variation: `Simple Drive - Low`.
    #[must_use]
    pub const fn name_only() -> Self {
        Self {
            show_type: false,
            show_collection_type: false,
            show_preset: true,
            show_variation: true,
        }
    }
}

/// The decomposed parts of an FX name's content portion (after the type/keyword prefix).
///
/// Given `"Simple Drive - Low"`, this splits into:
/// - `preset`: `"Simple Drive"`
/// - `variation`: `Some("Low")`
///
/// If there's no ` - ` separator, the whole string is the preset with no variation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxNameParts {
    pub preset: String,
    pub variation: Option<String>,
}

impl FxNameParts {
    /// Split a combined name like `"Simple Drive - Low"` into preset + variation.
    #[must_use]
    pub fn split(combined: &str) -> Self {
        // Split on the last ` - ` to handle presets containing hyphens
        // e.g., "Spring-Box - Boing" → preset="Spring-Box", variation="Boing"
        if let Some(pos) = combined.rfind(" - ") {
            let preset = combined[..pos].trim().to_string();
            let variation = combined[pos + 3..].trim().to_string();
            if !preset.is_empty() && !variation.is_empty() {
                return Self {
                    preset,
                    variation: Some(variation),
                };
            }
        }
        Self {
            preset: combined.trim().to_string(),
            variation: None,
        }
    }
}

// ─── FX Name Parsing ────────────────────────────────────────────

/// Classification of an FX item based on its display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FxRole {
    /// A module container: `"<TYPE> Module: <name>"`.
    Module {
        module_type: ModuleType,
        name: String,
    },
    /// A block (either container or leaf FX): `"<Type> Block: <name>"`.
    Block { block_type: BlockType, name: String },
    /// A standalone module with no "Module"/"Block" keyword: `"Module: <name>"`.
    GenericModule { name: String },
    /// An FX that doesn't match the naming convention.
    Unknown { name: String },
}

impl FxRole {
    /// Parse an FX display name into a structured role.
    ///
    /// Recognizes these patterns:
    /// - `"<TYPE> Module: <name>"` → `Module { module_type, name }`
    /// - `"<Type> Block: <name>"` → `Block { block_type, name }`
    /// - `"Module: <name>"` → `GenericModule { name }`
    /// - Anything else → `Unknown { name }`
    #[must_use]
    pub fn parse(display_name: &str) -> Self {
        // Strip optional [M] / [B] prefix
        let display_name = display_name
            .strip_prefix("[M] ")
            .or_else(|| display_name.strip_prefix("[B] "))
            .unwrap_or(display_name);

        // Try "<Word> Module: <rest>" first
        if let Some(rest) = Self::strip_keyword(display_name, "Module:") {
            let prefix = display_name[..display_name.len() - rest.len() - "Module:".len()]
                .trim()
                .to_uppercase();
            if prefix.is_empty() {
                return Self::GenericModule {
                    name: rest.trim().to_string(),
                };
            }
            let module_type = Self::infer_module_type(&prefix);
            return Self::Module {
                module_type,
                name: rest.trim().to_string(),
            };
        }

        // Try "<Word> Block: <rest>"
        if let Some(rest) = Self::strip_keyword(display_name, "Block:") {
            let prefix = display_name[..display_name.len() - rest.len() - "Block:".len()].trim();
            let block_type = BlockType::from_str(&prefix.to_lowercase())
                .or_else(|| Self::infer_block_type(prefix))
                .unwrap_or(BlockType::Custom);
            return Self::Block {
                block_type,
                name: rest.trim().to_string(),
            };
        }

        Self::Unknown {
            name: display_name.to_string(),
        }
    }

    /// Find `keyword` in the string and return everything after it.
    fn strip_keyword<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
        s.find(keyword).map(|pos| &s[pos + keyword.len()..])
    }

    fn infer_module_type(prefix: &str) -> ModuleType {
        match prefix {
            "INPUT" | "SOURCE" => ModuleType::Input,
            "DRIVE" => ModuleType::Drive,
            "PRE-FX" | "PREFX" => ModuleType::PreFx,
            "AMP" => ModuleType::Amp,
            "EQ" => ModuleType::Eq,
            "DYNAMICS" => ModuleType::Dynamics,
            "MODULATION" | "MOD" => ModuleType::Modulation,
            "TIME" => ModuleType::Time,
            "MOTION" => ModuleType::Motion,
            "MASTER" => ModuleType::Master,
            "RESCUE" => ModuleType::Rescue,
            "CORRECTION" => ModuleType::Correction,
            "TONAL" => ModuleType::Tonal,
            "SENDS" => ModuleType::Sends,
            "SPECIAL" => ModuleType::Special,
            _ => ModuleType::Custom,
        }
    }

    fn infer_block_type(prefix: &str) -> Option<BlockType> {
        // Case-insensitive match against known block type names
        let lower = prefix.to_lowercase();
        match lower.as_str() {
            "eq" => Some(BlockType::Eq),
            "drive" => Some(BlockType::Drive),
            "verb" | "reverb" => Some(BlockType::Reverb),
            "amp" => Some(BlockType::Amp),
            "chorus" => Some(BlockType::Chorus),
            "flanger" => Some(BlockType::Flanger),
            "delay" => Some(BlockType::Delay),
            "trem" | "tremolo" => Some(BlockType::Trem),
            "vibrato" => Some(BlockType::Vibrato),
            "rotary" => Some(BlockType::Rotary),
            "limiter" => Some(BlockType::Limiter),
            "utility" => Some(BlockType::Volume),
            _ => None,
        }
    }

    /// Format this FX role as a display name using the given options.
    ///
    /// # Examples
    ///
    /// ```text
    /// // Default (show_type + show_preset + show_variation):
    /// "DRIVE: Simple Drive - Low"
    ///
    /// // Full (all flags on):
    /// "DRIVE Module: Simple Drive - Low"
    ///
    /// // Name only (type + collection_type off):
    /// "Simple Drive - Low"
    /// ```
    #[must_use]
    pub fn format(&self, opts: &FxDisplayOptions) -> String {
        match self {
            Self::Module { module_type, name } => {
                let base =
                    Self::format_parts(opts, &module_type.as_str().to_uppercase(), "Module", name);
                format!("[M] {base}")
            }
            Self::Block { block_type, name } => {
                let type_label = Self::block_type_label(*block_type);
                let base = Self::format_parts(opts, &type_label, "Block", name);
                format!("[B] {base}")
            }
            Self::GenericModule { name } => {
                // GenericModule has no type prefix, only "Module" keyword
                let parts = FxNameParts::split(name);
                let base = Self::format_name_portion(opts, &parts);
                format!("[M] {base}")
            }
            Self::Unknown { name } => name.clone(),
        }
    }

    /// Format with the default display options.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.format(&FxDisplayOptions::default())
    }

    /// Get the type label for a block type as used in FX naming.
    fn block_type_label(block_type: BlockType) -> String {
        match block_type {
            BlockType::Reverb => "Reverb".to_string(),
            BlockType::Trem => "Trem".to_string(),
            BlockType::Volume => "Utility".to_string(),
            _ => {
                // Capitalize first letter: "eq" → "EQ", "drive" → "Drive"
                let name = block_type.display_name();
                name.to_string()
            }
        }
    }

    /// Assemble the display string from type, collection keyword, and name parts.
    fn format_parts(
        opts: &FxDisplayOptions,
        type_name: &str,
        collection_kw: &str,
        combined_name: &str,
    ) -> String {
        let parts = FxNameParts::split(combined_name);
        let mut result = String::new();

        // Type prefix (e.g., "DRIVE")
        if opts.show_type {
            result.push_str(type_name);
        }

        // Collection keyword (e.g., "Module" or "Block")
        if opts.show_collection_type {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(collection_kw);
        }

        // Separator between prefix and name portion
        let name_portion = Self::format_name_portion(opts, &parts);
        if !name_portion.is_empty() {
            if !result.is_empty() {
                result.push_str(": ");
            }
            result.push_str(&name_portion);
        }

        result
    }

    /// Format just the preset + variation portion based on options.
    fn format_name_portion(opts: &FxDisplayOptions, parts: &FxNameParts) -> String {
        match (opts.show_preset, opts.show_variation) {
            (true, true) => {
                if let Some(ref var) = parts.variation {
                    format!("{} - {}", parts.preset, var)
                } else {
                    parts.preset.clone()
                }
            }
            (true, false) => parts.preset.clone(),
            (false, true) => parts
                .variation
                .as_deref()
                .unwrap_or(&parts.preset)
                .to_string(),
            (false, false) => String::new(),
        }
    }
}

// ─── Track Name Roles ───────────────────────────────────────────

/// Display options controlling which parts of a track name are shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackDisplayOptions {
    /// Show the bracket prefix (e.g., `[L]`, `[E]`, `[R]`).
    pub show_prefix: bool,
    /// Show the role keyword (e.g., "Layer", "Engine", "Rig").
    pub show_role: bool,
    /// Show the name portion.
    pub show_name: bool,
}

impl Default for TrackDisplayOptions {
    fn default() -> Self {
        Self {
            show_prefix: true,
            show_role: false,
            show_name: true,
        }
    }
}

/// Classification of a REAPER track based on its role in the Signal hierarchy.
///
/// Parallel to [`FxRole`] (which classifies FX items), this classifies tracks:
/// - `[R]` — Rig (top-level folder)
/// - `[E]` — Engine (sub-folder under a rig)
/// - `[L]` — Layer (leaf track, holds FX chain of modules + blocks)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackRole {
    /// A rig-level track (top-level folder): `"[R] <name>"`.
    Rig { name: String },
    /// An engine-level track (sub-folder): `"[E] <name>"`.
    Engine { name: String },
    /// A layer-level track (leaf track with FX): `"[L] <name>"`.
    Layer { name: String },
}

impl TrackRole {
    /// Format this track role as a display name using the given options.
    #[must_use]
    pub fn format(&self, opts: &TrackDisplayOptions) -> String {
        let (prefix, role, name) = match self {
            Self::Rig { name } => ("[R]", "Rig", name.as_str()),
            Self::Engine { name } => ("[E]", "Engine", name.as_str()),
            Self::Layer { name } => ("[L]", "Layer", name.as_str()),
        };

        let mut result = String::new();
        if opts.show_prefix {
            result.push_str(prefix);
        }
        if opts.show_role {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(role);
        }
        if opts.show_name {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(name);
        }
        result
    }

    /// Format with the default display options (`[X] <name>`).
    #[must_use]
    pub fn display_name(&self) -> String {
        self.format(&TrackDisplayOptions::default())
    }

    /// Parse a track display name into a structured role.
    ///
    /// Recognizes `[R] ...`, `[E] ...`, `[L] ...` prefixes.
    /// Returns `None` if the name doesn't match any known prefix.
    #[must_use]
    pub fn parse(display_name: &str) -> Option<Self> {
        if let Some(rest) = display_name.strip_prefix("[R] ") {
            Some(Self::Rig {
                name: rest.to_string(),
            })
        } else if let Some(rest) = display_name.strip_prefix("[E] ") {
            Some(Self::Engine {
                name: rest.to_string(),
            })
        } else {
            display_name.strip_prefix("[L] ").map(|rest| Self::Layer {
                name: rest.to_string(),
            })
        }
    }
}
