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
    /// blocks (e.g., an Amp Block backed by amp_sim + cabinet_sim in a
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
    /// For a multi-FX block (e.g., an amp block backed by amp_sim + cabinet_sim
    /// inside a REAPER Container), this lists all linked FX indices.
    /// Each [`ParamMapping::fx_slot`] indexes into this array.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_fx_indices: Vec<u32>,
}

fn default_true() -> bool {
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
    pub fn is_multi_fx(&self) -> bool {
        self.linked_fx_indices.len() > 1
    }

    /// Resolve a param's `fx_slot` to an actual FX chain index.
    ///
    /// Returns `None` if `linked_fx_indices` is empty (single-FX block —
    /// the caller should use the block's default FX index).
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
/// database — it lives as a JSON file or is embedded inline in a LayerSnapshot.
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

fn default_version() -> u32 {
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
    pub fn all_blocks(&self) -> Vec<&VirtualBlock> {
        self.modules.iter().flat_map(|m| &m.blocks).collect()
    }

    /// Validate that no parameter index exceeds `param_count` and no index is mapped twice
    /// within the same FX slot.
    ///
    /// For multi-FX blocks, the same `plugin_param_index` is allowed across
    /// different `fx_slot` values (they target different plugins).
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
    /// `SignalNode::Block(ModuleBlock)` with `ModuleBlockSource::Inline`,
    /// carrying the virtual block's parameters as `BlockParameter`s.
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

                let nodes: Vec<SignalNode> = blocks.into_iter().map(SignalNode::Block).collect();
                let chain = SignalChain::new(nodes);
                (vm.label.clone(), vm.module_type, chain)
            })
            .collect()
    }
}

// ─── Errors ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
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

// ─── FX Name Parsing ────────────────────────────────────────────

/// Classification of an FX item based on its display name.
#[derive(Debug, Clone, PartialEq)]
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
    pub fn parse(display_name: &str) -> Self {
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
            "INPUT" | "SOURCE" => ModuleType::Source,
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
            "trem" | "tremolo" => Some(BlockType::Tremolo),
            "vibrato" => Some(BlockType::Vibrato),
            "rotary" => Some(BlockType::Rotary),
            "limiter" => Some(BlockType::Limiter),
            "utility" => Some(BlockType::Volume),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_fx_block_backwards_compatible() {
        let block = VirtualBlock::new("boost", "Boost", BlockType::Boost)
            .with_param(ParamMapping::new("gain", 0, 0.5))
            .with_param(ParamMapping::new("tone", 1, 0.7));

        assert!(!block.is_multi_fx());
        assert!(block.linked_fx_indices.is_empty());
        assert_eq!(block.params[0].fx_slot, 0);
        assert_eq!(block.resolve_fx_index(0), None); // no linked FX → caller uses default
    }

    #[test]
    fn multi_fx_block_with_linked_indices() {
        // Simulates the Guitar Rig's "Amp Block: JS Amp" which has
        // amp_sim (FX index 5) + cabinet_sim (FX index 6) in a container.
        let block = VirtualBlock::new("js-amp", "JS Amp", BlockType::Amp)
            .with_linked_fx(vec![5, 6])
            .with_param(ParamMapping::new_multi_fx("amp-gain", 0, 0.5, 0))
            .with_param(ParamMapping::new_multi_fx("amp-tone", 1, 0.6, 0))
            .with_param(ParamMapping::new_multi_fx("cab-type", 0, 0.3, 1))
            .with_param(ParamMapping::new_multi_fx("cab-mic", 1, 0.5, 1));

        assert!(block.is_multi_fx());
        assert_eq!(block.linked_fx_indices, vec![5, 6]);
        assert_eq!(block.resolve_fx_index(0), Some(5)); // amp_sim
        assert_eq!(block.resolve_fx_index(1), Some(6)); // cabinet_sim
        assert_eq!(block.resolve_fx_index(2), None); // out of range

        // Params targeting FX slot 0 (amp_sim)
        let amp_params: Vec<_> = block.params.iter().filter(|p| p.fx_slot == 0).collect();
        assert_eq!(amp_params.len(), 2);
        assert_eq!(amp_params[0].name, "amp-gain");

        // Params targeting FX slot 1 (cabinet_sim)
        let cab_params: Vec<_> = block.params.iter().filter(|p| p.fx_slot == 1).collect();
        assert_eq!(cab_params.len(), 2);
        assert_eq!(cab_params[0].name, "cab-type");
    }

    #[test]
    fn validate_allows_same_index_across_fx_slots() {
        // Both FX slots use param index 0 — this should be valid.
        let def = PluginBlockDef::new("TestPlugin", 10).with_module(
            VirtualModule::new("amp-module", "Amp Module", ModuleType::Amp).with_block(
                VirtualBlock::new("amp", "Amp", BlockType::Amp)
                    .with_linked_fx(vec![0, 1])
                    .with_param(ParamMapping::new_multi_fx("amp-gain", 0, 0.5, 0))
                    .with_param(ParamMapping::new_multi_fx("cab-gain", 0, 0.5, 1)),
            ),
        );

        assert!(def.validate().is_ok());
    }

    #[test]
    fn validate_rejects_duplicate_within_same_fx_slot() {
        let def = PluginBlockDef::new("TestPlugin", 10).with_module(
            VirtualModule::new("mod", "Module", ModuleType::Amp).with_block(
                VirtualBlock::new("block", "Block", BlockType::Amp)
                    .with_param(ParamMapping::new("gain", 0, 0.5))
                    .with_param(ParamMapping::new("also-gain", 0, 0.7)), // duplicate index in same slot
            ),
        );

        let err = def.validate().unwrap_err();
        match err {
            PluginBlockDefError::DuplicateIndex { index, .. } => assert_eq!(index, 0),
            _ => panic!("expected DuplicateIndex, got {:?}", err),
        }
    }

    #[test]
    fn serde_round_trip_multi_fx() {
        let block = VirtualBlock::new("js-amp", "JS Amp", BlockType::Amp)
            .with_linked_fx(vec![5, 6])
            .with_param(ParamMapping::new_multi_fx("gain", 0, 0.5, 0))
            .with_param(ParamMapping::new_multi_fx("cab", 0, 0.3, 1));

        let json = serde_json::to_string(&block).unwrap();
        let restored: VirtualBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.linked_fx_indices, vec![5, 6]);
        assert_eq!(restored.params[0].fx_slot, 0);
        assert_eq!(restored.params[1].fx_slot, 1);
    }

    #[test]
    fn serde_backwards_compat_missing_fields() {
        // Simulate JSON from before multi-FX support was added
        let json = r#"{
            "id": "boost",
            "label": "Boost",
            "block_type": "Boost",
            "params": [{"name": "gain", "plugin_param_index": 0, "default_value": 0.5}],
            "enabled": true
        }"#;
        let block: VirtualBlock = serde_json::from_str(json).unwrap();
        assert!(block.linked_fx_indices.is_empty());
        assert_eq!(block.params[0].fx_slot, 0);
    }

    // ─── FxRole parsing tests ───────────────────────────────────

    #[test]
    fn parse_module_name() {
        let role = FxRole::parse("INPUT Module: Guitar Input - Relaxed");
        assert_eq!(
            role,
            FxRole::Module {
                module_type: ModuleType::Source,
                name: "Guitar Input - Relaxed".to_string(),
            }
        );
    }

    #[test]
    fn parse_drive_module() {
        let role = FxRole::parse("DRIVE Module: Simple Drive - Low");
        assert_eq!(
            role,
            FxRole::Module {
                module_type: ModuleType::Drive,
                name: "Simple Drive - Low".to_string(),
            }
        );
    }

    #[test]
    fn parse_amp_module() {
        let role = FxRole::parse("AMP Module: JS / Tukan Combo - Clean");
        assert_eq!(
            role,
            FxRole::Module {
                module_type: ModuleType::Amp,
                name: "JS / Tukan Combo - Clean".to_string(),
            }
        );
    }

    #[test]
    fn parse_block_name() {
        let role = FxRole::parse("EQ Block: Reagate - Relaxed");
        assert_eq!(
            role,
            FxRole::Block {
                block_type: BlockType::Eq,
                name: "Reagate - Relaxed".to_string(),
            }
        );
    }

    #[test]
    fn parse_reverb_block() {
        let role = FxRole::parse("Reverb Block: ReaVerbate - Ambient Room (Reaverbate)");
        assert_eq!(
            role,
            FxRole::Block {
                block_type: BlockType::Reverb,
                name: "ReaVerbate - Ambient Room (Reaverbate)".to_string(),
            }
        );
    }

    #[test]
    fn parse_verb_block() {
        // "Verb" is a shorthand alias for Reverb
        let role = FxRole::parse("Verb Block: Spring-Box - Boing (Spring-Box Delay-Reverb)");
        assert_eq!(
            role,
            FxRole::Block {
                block_type: BlockType::Reverb,
                name: "Spring-Box - Boing (Spring-Box Delay-Reverb)".to_string(),
            }
        );
    }

    #[test]
    fn parse_amp_block() {
        let role = FxRole::parse("Amp Block: JS Amp - Clean");
        assert_eq!(
            role,
            FxRole::Block {
                block_type: BlockType::Amp,
                name: "JS Amp - Clean".to_string(),
            }
        );
    }

    #[test]
    fn parse_generic_module() {
        let role = FxRole::parse("Module: Room");
        assert_eq!(
            role,
            FxRole::GenericModule {
                name: "Room".to_string(),
            }
        );
    }

    #[test]
    fn parse_unknown_fx() {
        let role = FxRole::parse("ReaComp (Cockos)");
        assert_eq!(
            role,
            FxRole::Unknown {
                name: "ReaComp (Cockos)".to_string(),
            }
        );
    }

    #[test]
    fn parse_utility_block() {
        let role = FxRole::parse("Utility Block: Trim");
        assert_eq!(
            role,
            FxRole::Block {
                block_type: BlockType::Volume,
                name: "Trim".to_string(),
            }
        );
    }

    #[test]
    fn parse_all_guitar_rig_containers() {
        // Every top-level container from the Guitar Rig in signal-stock-testing.RPP
        let names = [
            ("INPUT Module: Guitar Input - Relaxed", "Source"),
            ("DRIVE Module: Simple Drive - Low", "Drive"),
            ("PRE-FX Module: Spring Box - Boing", "Pre-FX"),
            ("AMP Module: JS / Tukan Combo - Clean", "Amp"),
            ("MODULATION Module: Stock - Heavy", "Modulation"),
            ("TIME Module: StockJS - Heavy", "Time"),
            ("MOTION Module: StockJS - Wonky", "Motion"),
            ("MASTER Module: Polish - Light Mud Removal", "Master"),
        ];
        for (name, expected_type) in &names {
            match FxRole::parse(name) {
                FxRole::Module { module_type, .. } => {
                    assert_eq!(
                        module_type.display_name(),
                        *expected_type,
                        "wrong type for '{name}'"
                    );
                }
                other => panic!("expected Module for '{name}', got {:?}", other),
            }
        }
    }

    #[test]
    fn parse_all_guitar_rig_blocks() {
        // Every block FX from the Guitar Rig
        let names = [
            ("EQ Block: Reagate - Relaxed", BlockType::Eq),
            (
                "Drive Block: Eric Distortion - Light (Eris Distortion)",
                BlockType::Drive,
            ),
            (
                "Verb Block: Spring-Box - Boing (Spring-Box Delay-Reverb)",
                BlockType::Reverb,
            ),
            ("Amp Block: JS Amp - Clean", BlockType::Amp),
            ("Amp Block: Tukan - Crunch", BlockType::Amp),
            (
                "Reverb Block: ReaVerbate - Ambient Room (Reaverbate)",
                BlockType::Reverb,
            ),
            (
                "Chorus Block: Tukan S2 - Heavy (Chorus S2)",
                BlockType::Chorus,
            ),
            (
                "Flanger Block: Kawa XY - Crazy (kawa_XY_Flanger)",
                BlockType::Flanger,
            ),
            (
                "Delay Block: ReaDelay - Filtered 1/8 (ReaDelay)",
                BlockType::Delay,
            ),
            (
                "Delay Block: Delay Machine 2 - Simple Taps (Delay Machine 2)",
                BlockType::Delay,
            ),
            ("Reverb Block: ReaVerb - Empty (Reaverb)", BlockType::Reverb),
            (
                "Reverb Block: Abyss (Saike) - Blackhole (Abyss Reverb)",
                BlockType::Reverb,
            ),
            ("Trem Block: Tukan AC - Light (AC Trem)", BlockType::Tremolo),
            (
                "Vibrato Block: Garaint Luff - Fast (Vibrarto by Geraint Luff)",
                BlockType::Vibrato,
            ),
            (
                "Rotary Block: Tukan - Leslie (Rotary (Tukan))",
                BlockType::Rotary,
            ),
            ("EQ Block: ReaEQ - Mud Removal (ReaEQ)", BlockType::Eq),
            (
                "Limiter Block: ReaLimit - No Latency -6db (ReaLimit)",
                BlockType::Limiter,
            ),
            ("Utility Block: Trim", BlockType::Volume),
        ];
        for (name, expected_type) in &names {
            match FxRole::parse(name) {
                FxRole::Block { block_type, .. } => {
                    assert_eq!(block_type, *expected_type, "wrong type for '{name}'");
                }
                other => panic!("expected Block for '{name}', got {:?}", other),
            }
        }
    }

    // ─── Full Guitar Rig hierarchy mapping ──────────────────────

    #[test]
    fn build_guitar_rig_hierarchy() {
        // Simulate the flat FX list as REAPER would report it.
        // Each tuple: (fx_index, display_name, param_count)
        // Containers have 1 param (bypass), leaf FX have real param counts.
        let fx_chain: Vec<(u32, &str, u32)> = vec![
            // INPUT Module
            (0, "INPUT Module: Guitar Input - Relaxed", 1),
            (1, "EQ Block: Reagate - Relaxed", 14),
            // DRIVE Module
            (2, "DRIVE Module: Simple Drive - Low", 1),
            (
                3,
                "Drive Block: Eric Distortion - Light (Eris Distortion)",
                8,
            ),
            // PRE-FX Module
            (4, "PRE-FX Module: Spring Box - Boing", 1),
            (
                5,
                "Verb Block: Spring-Box - Boing (Spring-Box Delay-Reverb)",
                12,
            ),
            // AMP Module (nested containers)
            (6, "AMP Module: JS / Tukan Combo - Clean", 1),
            (7, "Amp Block: JS Amp - Clean", 1),  // sub-container
            (8, "amp_sim", 20),                   // leaf FX in Amp Block
            (9, "cabinet_sim", 10),               // leaf FX in Amp Block
            (10, "Amp Block: Tukan - Crunch", 1), // sub-container
            (11, "Guitar Amp (Tukan)", 15),       // leaf FX
            (12, "amp-model", 8),                 // leaf FX
            (13, "Module: Room", 1),              // sub-container
            (
                14,
                "Reverb Block: ReaVerbate - Ambient Room (Reaverbate)",
                16,
            ),
            // MODULATION Module
            (15, "MODULATION Module: Stock - Heavy", 1),
            (16, "Chorus Block: Tukan S2 - Heavy (Chorus S2)", 10),
            (17, "Flanger Block: Kawa XY - Crazy (kawa_XY_Flanger)", 12),
            // TIME Module
            (18, "TIME Module: StockJS - Heavy", 1),
            (19, "Delay Block: ReaDelay - Filtered 1/8 (ReaDelay)", 20),
            (
                20,
                "Delay Block: Delay Machine 2 - Simple Taps (Delay Machine 2)",
                14,
            ),
            (21, "Passthrough", 2),
            (22, "Reverb Block: ReaVerb - Empty (Reaverb)", 24),
            (
                23,
                "Reverb Block: Abyss (Saike) - Blackhole (Abyss Reverb)",
                16,
            ),
            (24, "Passhrough", 2),
            // MOTION Module
            (25, "MOTION Module: StockJS - Wonky", 1),
            (26, "Trem Block: Tukan AC - Light (AC Trem)", 8),
            (
                27,
                "Vibrato Block: Garaint Luff - Fast (Vibrarto by Geraint Luff)",
                6,
            ),
            (28, "Rotary Block: Tukan - Leslie (Rotary (Tukan))", 10),
            // MASTER Module
            (29, "MASTER Module: Polish - Light Mud Removal", 1),
            (30, "EQ Block: ReaEQ - Mud Removal (ReaEQ)", 36),
            (
                31,
                "Limiter Block: ReaLimit - No Latency -6db (ReaLimit)",
                8,
            ),
            (32, "Utility Block: Trim", 4),
        ];

        // Parse every FX name and collect modules with their blocks
        let mut modules: Vec<(ModuleType, String, Vec<(BlockType, String, Vec<u32>)>)> = Vec::new();
        let mut current_module: Option<(ModuleType, String, Vec<(BlockType, String, Vec<u32>)>)> =
            None;
        let mut current_block_fx: Option<(BlockType, String, Vec<u32>)> = None;

        for (idx, name, param_count) in &fx_chain {
            let role = FxRole::parse(name);
            match role {
                FxRole::Module { module_type, name } => {
                    // Flush previous block into previous module
                    if let Some(block) = current_block_fx.take() {
                        if let Some(ref mut m) = current_module {
                            m.2.push(block);
                        }
                    }
                    // Flush previous module
                    if let Some(m) = current_module.take() {
                        modules.push(m);
                    }
                    current_module = Some((module_type, name, Vec::new()));
                }
                FxRole::Block { block_type, name } => {
                    // Flush previous block
                    if let Some(block) = current_block_fx.take() {
                        if let Some(ref mut m) = current_module {
                            m.2.push(block);
                        }
                    }
                    if *param_count <= 1 {
                        // This is a container block (multi-FX) — will collect leaf FX
                        current_block_fx = Some((block_type, name, Vec::new()));
                    } else {
                        // Leaf block FX — single FX
                        if let Some(ref mut m) = current_module {
                            m.2.push((block_type, name, vec![*idx]));
                        }
                    }
                }
                FxRole::GenericModule { name } => {
                    // Sub-module within a parent module (e.g., "Module: Room")
                    if let Some(block) = current_block_fx.take() {
                        if let Some(ref mut m) = current_module {
                            m.2.push(block);
                        }
                    }
                    // Treat as a nested module context — blocks will follow
                }
                FxRole::Unknown { .. } => {
                    // Leaf FX — belongs to current block or current module
                    if let Some(ref mut block) = current_block_fx {
                        block.2.push(*idx); // Add to multi-FX block
                    }
                    // Otherwise it's a standalone FX (like Passthrough)
                }
            }
        }
        // Flush final state
        if let Some(block) = current_block_fx.take() {
            if let Some(ref mut m) = current_module {
                m.2.push(block);
            }
        }
        if let Some(m) = current_module.take() {
            modules.push(m);
        }

        // Verify we got all 8 modules
        assert_eq!(modules.len(), 8, "expected 8 modules");
        let module_types: Vec<ModuleType> = modules.iter().map(|m| m.0).collect();
        assert_eq!(
            module_types,
            vec![
                ModuleType::Source,
                ModuleType::Drive,
                ModuleType::PreFx,
                ModuleType::Amp,
                ModuleType::Modulation,
                ModuleType::Time,
                ModuleType::Motion,
                ModuleType::Master,
            ]
        );

        // INPUT has 1 block (EQ)
        assert_eq!(modules[0].2.len(), 1);
        assert_eq!(modules[0].2[0].0, BlockType::Eq);

        // DRIVE has 1 block (Drive)
        assert_eq!(modules[1].2.len(), 1);
        assert_eq!(modules[1].2[0].0, BlockType::Drive);

        // PRE-FX has 1 block (Reverb)
        assert_eq!(modules[2].2.len(), 1);
        assert_eq!(modules[2].2[0].0, BlockType::Reverb);

        // AMP has 3 entries: JS Amp (multi-FX), Tukan (multi-FX), Room Reverb
        assert_eq!(
            modules[3].2.len(),
            3,
            "AMP module blocks: {:?}",
            modules[3].2
        );
        // JS Amp is multi-FX with indices [8, 9]
        assert_eq!(modules[3].2[0].0, BlockType::Amp);
        assert_eq!(modules[3].2[0].2, vec![8, 9], "JS Amp should link FX 8+9");
        // Tukan is multi-FX with indices [11, 12]
        assert_eq!(modules[3].2[1].0, BlockType::Amp);
        assert_eq!(
            modules[3].2[1].2,
            vec![11, 12],
            "Tukan should link FX 11+12"
        );
        // Room Reverb is single-FX
        assert_eq!(modules[3].2[2].0, BlockType::Reverb);
        assert_eq!(modules[3].2[2].2, vec![14]);

        // MODULATION has 2 blocks
        assert_eq!(modules[4].2.len(), 2);
        assert_eq!(modules[4].2[0].0, BlockType::Chorus);
        assert_eq!(modules[4].2[1].0, BlockType::Flanger);

        // TIME has 4 blocks (2 delays, 2 reverbs — passthrough FX are skipped as Unknown)
        assert_eq!(
            modules[5].2.len(),
            4,
            "TIME module blocks: {:?}",
            modules[5].2
        );

        // MOTION has 3 blocks
        assert_eq!(modules[6].2.len(), 3);
        assert_eq!(modules[6].2[0].0, BlockType::Tremolo);
        assert_eq!(modules[6].2[1].0, BlockType::Vibrato);
        assert_eq!(modules[6].2[2].0, BlockType::Rotary);

        // MASTER has 3 blocks
        assert_eq!(modules[7].2.len(), 3);
        assert_eq!(modules[7].2[0].0, BlockType::Eq);
        assert_eq!(modules[7].2[1].0, BlockType::Limiter);
        assert_eq!(modules[7].2[2].0, BlockType::Volume); // Utility maps to Volume

        // Now build the actual PluginBlockDef from parsed data
        let mut def = PluginBlockDef::new("Guitar Rig (Stock)", 100);
        for (module_type, module_name, blocks) in &modules {
            let mut vm = VirtualModule::new(module_type.as_str(), module_name, *module_type);
            for (block_type, block_name, fx_indices) in blocks {
                let mut vb = VirtualBlock::new(block_type.as_str(), block_name, *block_type);
                if fx_indices.len() > 1 {
                    vb = vb.with_linked_fx(fx_indices.clone());
                }
                vm = vm.with_block(vb);
            }
            def = def.with_module(vm);
        }

        assert_eq!(def.modules.len(), 8);
        assert_eq!(def.all_blocks().len(), 18);

        // Verify multi-FX blocks
        let amp_module = &def.modules[3];
        assert!(amp_module.blocks[0].is_multi_fx());
        assert!(amp_module.blocks[1].is_multi_fx());
        assert!(!amp_module.blocks[2].is_multi_fx());

        // Verify resolve
        assert_eq!(amp_module.blocks[0].resolve_fx_index(0), Some(8));
        assert_eq!(amp_module.blocks[0].resolve_fx_index(1), Some(9));
        assert_eq!(amp_module.blocks[1].resolve_fx_index(0), Some(11));
        assert_eq!(amp_module.blocks[1].resolve_fx_index(1), Some(12));
    }
}
