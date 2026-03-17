//! Core domain model types — blocks, snapshots, presets, and modules.
//!
//! This module contains the foundational data structures that form the
//! building blocks of the signal hierarchy:
//!
//! - **Block-level**: [`Block`], [`Snapshot`], [`Preset`] — FX parameter state
//! - **Module-level**: [`Module`], [`ModuleSnapshot`], [`ModulePreset`] — FX chain containers
//! - **Composition**: [`ModuleBlock`], [`ModuleBlockSource`], [`BlockParameterOverride`]
//! - **Classification**: [`EngineType`]
//! - **Traits**: [`SnapshotLike`], [`PresetLike`] — generic snapshot/preset access

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ids::{ModulePresetId, ModuleSnapshotId, PresetId, SnapshotId};
use crate::metadata;
use crate::module_type::ModuleType;
use crate::signal_chain::SignalChain;
use crate::traits;
use crate::BlockType;
use macromod::ParameterValue;

// ─── EngineType ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet, Default)]
#[repr(C)]
pub enum EngineType {
    #[default]
    Guitar,
    Bass,
    Vocal,
    Keys,
    Synth,
    Organ,
    Pad,
}

impl EngineType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Guitar => "guitar",
            Self::Bass => "bass",
            Self::Vocal => "vocal",
            Self::Keys => "keys",
            Self::Synth => "synth",
            Self::Organ => "organ",
            Self::Pad => "pad",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "guitar" => Some(Self::Guitar),
            "bass" => Some(Self::Bass),
            "vocal" => Some(Self::Vocal),
            "keys" => Some(Self::Keys),
            "synth" => Some(Self::Synth),
            "organ" => Some(Self::Organ),
            "pad" => Some(Self::Pad),
            _ => None,
        }
    }
}

// ─── Block ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Block {
    parameters: Vec<macromod::BlockParameter>,
    /// Optional macro knob bank for this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macro_bank: Option<macromod::MacroBank>,
    /// Optional parameter curation for the custom GUI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_curation: Option<macromod::ParamCuration>,
    /// Optional modulation routing for this block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modulation: Option<macromod::ModulationRouteSet>,
}

impl Block {
    pub fn new(param_1: f32, param_2: f32, param_3: f32) -> Self {
        Self::from_parameters(vec![
            macromod::BlockParameter::new("param_1", "Parameter 1", param_1),
            macromod::BlockParameter::new("param_2", "Parameter 2", param_2),
            macromod::BlockParameter::new("param_3", "Parameter 3", param_3),
        ])
    }

    pub fn from_parameters(parameters: Vec<macromod::BlockParameter>) -> Self {
        let parameters = if parameters.is_empty() {
            vec![macromod::BlockParameter::new("value", "Value", 0.5)]
        } else {
            parameters
        };

        Self {
            parameters,
            macro_bank: None,
            param_curation: None,
            modulation: None,
        }
    }

    pub fn parameters(&self) -> &[macromod::BlockParameter] {
        &self.parameters
    }

    pub fn set_parameter_value(&mut self, index: usize, value: f32) {
        if let Some(parameter) = self.parameters.get_mut(index) {
            parameter.set_value(value);
        }
    }

    pub fn first_value(&self) -> Option<f32> {
        self.parameters.first().map(|p| p.value().get())
    }

    pub fn set_first_value(&mut self, value: f32) {
        self.set_parameter_value(0, value);
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::new(0.5, 0.5, 0.5)
    }
}

// ─── Snapshot ──────────────────────────────────────────────────

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Snapshot {
    id: SnapshotId,
    name: String,
    block: Block,
    #[serde(default)]
    metadata: metadata::Metadata,
    #[serde(default = "default_version")]
    version: u32,
    /// Optional binary plugin state (e.g. JUCE preset `.bin` data).
    ///
    /// When present, the DAW bridge loads this via `set_state_chunk` instead
    /// of setting parameters one-by-one. This is required for plugins like
    /// Neural DSP where fingerprint param names don't match REAPER-exposed names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    state_data: Option<Vec<u8>>,
}

impl Snapshot {
    pub fn new(id: impl Into<SnapshotId>, name: impl Into<String>, block: Block) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            block,
            metadata: metadata::Metadata::new(),
            version: 1,
            state_data: None,
        }
    }

    /// Create a snapshot with an explicit version (used by storage layer on load).
    pub fn with_version(
        id: impl Into<SnapshotId>,
        name: impl Into<String>,
        block: Block,
        version: u32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            block,
            metadata: metadata::Metadata::new(),
            version,
            state_data: None,
        }
    }

    /// Create a snapshot with explicit version + metadata (used by storage layer on load).
    pub fn with_version_and_metadata(
        id: impl Into<SnapshotId>,
        name: impl Into<String>,
        block: Block,
        version: u32,
        metadata: metadata::Metadata,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            block,
            metadata,
            version,
            state_data: None,
        }
    }

    pub fn id(&self) -> &SnapshotId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn block(&self) -> Block {
        self.block.clone()
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: metadata::Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Attach binary plugin state data (e.g. JUCE `.bin` preset file).
    #[must_use]
    pub fn with_state_data(mut self, data: Vec<u8>) -> Self {
        self.state_data = Some(data);
        self
    }

    /// Binary plugin state data, if available.
    pub fn state_data(&self) -> Option<&[u8]> {
        self.state_data.as_deref()
    }

    /// Replace the block state. Used when saving parameter changes.
    pub fn set_block(&mut self, block: Block) {
        self.block = block;
    }

    /// Replace the binary plugin state data.
    pub fn set_state_data(&mut self, data: Vec<u8>) {
        self.state_data = Some(data);
    }

    /// Bump the version counter. Called by the storage layer when parameter values change.
    pub fn increment_version(&mut self) {
        self.version += 1;
    }
}

// ─── SnapshotLike / PresetLike traits ──────────────────────────

pub trait SnapshotLike {
    type Id;
    type State;

    fn id(&self) -> &Self::Id;
    fn name(&self) -> &str;
    fn state(&self) -> &Self::State;
}

impl SnapshotLike for Snapshot {
    type Id = SnapshotId;
    type State = Block;

    fn id(&self) -> &Self::Id {
        self.id()
    }

    fn name(&self) -> &str {
        self.name()
    }

    fn state(&self) -> &Self::State {
        &self.block
    }
}

impl traits::Variant for Snapshot {
    type Id = SnapshotId;
    type BaseRef = ();
    type Override = ();
    fn id(&self) -> &SnapshotId {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
}

impl traits::DefaultVariant for Snapshot {
    fn default_named(name: impl Into<String>) -> Self {
        Self::new(SnapshotId::new(), name, Block::default())
    }
}

// ─── Preset ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Preset {
    id: PresetId,
    name: String,
    block_type: BlockType,
    default_snapshot: Snapshot,
    snapshots: Vec<Snapshot>,
    #[serde(default)]
    metadata: metadata::Metadata,
}

impl Preset {
    pub fn new(
        id: impl Into<PresetId>,
        name: impl Into<String>,
        block_type: BlockType,
        default_snapshot: Snapshot,
        additional_snapshots: Vec<Snapshot>,
    ) -> Self {
        let mut snapshots = Vec::with_capacity(additional_snapshots.len() + 1);
        snapshots.push(default_snapshot.clone());
        snapshots.extend(
            additional_snapshots
                .into_iter()
                .filter(|s| s.id() != default_snapshot.id()),
        );

        Self {
            id: id.into(),
            name: name.into(),
            block_type,
            default_snapshot,
            snapshots,
            metadata: metadata::Metadata::new(),
        }
    }

    pub fn with_default_snapshot(
        id: impl Into<PresetId>,
        name: impl Into<String>,
        block_type: BlockType,
        default_snapshot: Snapshot,
    ) -> Self {
        Self::new(id, name, block_type, default_snapshot, Vec::new())
    }

    pub fn id(&self) -> &PresetId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn block_type(&self) -> BlockType {
        self.block_type
    }

    pub fn default_snapshot(&self) -> Snapshot {
        self.default_snapshot.clone()
    }

    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    pub fn snapshot(&self, snapshot_id: &SnapshotId) -> Option<Snapshot> {
        self.snapshots
            .iter()
            .find(|s| s.id() == snapshot_id)
            .cloned()
    }

    /// Add a snapshot to this preset's variant list.
    pub fn add_snapshot(&mut self, snapshot: Snapshot) {
        if !self.snapshots.iter().any(|s| s.id == snapshot.id) {
            self.snapshots.push(snapshot);
        }
    }

    pub fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: metadata::Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

pub trait PresetLike {
    type Id;
    type SnapshotId;
    type Snapshot: SnapshotLike<Id = Self::SnapshotId>;

    fn id(&self) -> &Self::Id;
    fn name(&self) -> &str;
    fn snapshots(&self) -> &[Self::Snapshot];
    fn default_snapshot_id(&self) -> &Self::SnapshotId;

    fn default_snapshot(&self) -> Option<&Self::Snapshot>
    where
        Self::SnapshotId: PartialEq,
    {
        self.snapshots()
            .iter()
            .find(|snapshot| snapshot.id() == self.default_snapshot_id())
    }
}

impl PresetLike for Preset {
    type Id = PresetId;
    type SnapshotId = SnapshotId;
    type Snapshot = Snapshot;

    fn id(&self) -> &Self::Id {
        self.id()
    }

    fn name(&self) -> &str {
        self.name()
    }

    fn snapshots(&self) -> &[Self::Snapshot] {
        self.snapshots()
    }

    fn default_snapshot_id(&self) -> &Self::SnapshotId {
        self.default_snapshot.id()
    }
}

impl traits::Collection for Preset {
    type Variant = Snapshot;

    fn variants(&self) -> &[Snapshot] {
        &self.snapshots
    }
    fn variants_mut(&mut self) -> &mut Vec<Snapshot> {
        &mut self.snapshots
    }
    fn default_variant_id(&self) -> &SnapshotId {
        self.default_snapshot.id()
    }
    fn set_default_variant_id(&mut self, id: SnapshotId) {
        if let Some(snap) = self.snapshots.iter().find(|s| s.id == id) {
            self.default_snapshot = snap.clone();
        }
    }
}

impl traits::HasMetadata for Snapshot {
    fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut metadata::Metadata {
        &mut self.metadata
    }
}

impl traits::HasMetadata for Preset {
    fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut metadata::Metadata {
        &mut self.metadata
    }
}

// ─── BlockParameterOverride ────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct BlockParameterOverride {
    parameter_id: String,
    value: ParameterValue,
}

impl BlockParameterOverride {
    pub fn new(parameter_id: impl Into<String>, value: f32) -> Self {
        Self {
            parameter_id: parameter_id.into(),
            value: ParameterValue::new(value),
        }
    }

    pub fn parameter_id(&self) -> &str {
        &self.parameter_id
    }

    pub fn value(&self) -> ParameterValue {
        self.value
    }
}

// ─── ModuleBlockSource ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum ModuleBlockSource {
    PresetDefault {
        preset_id: PresetId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        saved_at_version: Option<u32>,
    },
    PresetSnapshot {
        preset_id: PresetId,
        snapshot_id: SnapshotId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        saved_at_version: Option<u32>,
    },
    Inline {
        block: Block,
    },
}

impl ModuleBlockSource {
    /// The snapshot version this source was saved against, if known.
    ///
    /// Returns `None` for legacy data or inline blocks.
    pub fn saved_at_version(&self) -> Option<u32> {
        match self {
            Self::PresetDefault {
                saved_at_version, ..
            } => *saved_at_version,
            Self::PresetSnapshot {
                saved_at_version, ..
            } => *saved_at_version,
            Self::Inline { .. } => None,
        }
    }

    /// Return a new source with the saved version stamped.
    pub fn with_saved_version(self, version: u32) -> Self {
        match self {
            Self::PresetDefault { preset_id, .. } => Self::PresetDefault {
                preset_id,
                saved_at_version: Some(version),
            },
            Self::PresetSnapshot {
                preset_id,
                snapshot_id,
                ..
            } => Self::PresetSnapshot {
                preset_id,
                snapshot_id,
                saved_at_version: Some(version),
            },
            inline @ Self::Inline { .. } => inline,
        }
    }
}

// ─── ModuleBlock ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModuleBlock {
    id: String,
    label: String,
    block_type: BlockType,
    source: ModuleBlockSource,
    overrides: Vec<BlockParameterOverride>,
}

impl ModuleBlock {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        block_type: BlockType,
        source: ModuleBlockSource,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            block_type,
            source,
            overrides: Vec::new(),
        }
    }

    pub fn with_overrides(mut self, overrides: Vec<BlockParameterOverride>) -> Self {
        self.overrides = overrides;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn block_type(&self) -> BlockType {
        self.block_type
    }

    pub fn source(&self) -> &ModuleBlockSource {
        &self.source
    }

    pub fn overrides(&self) -> &[BlockParameterOverride] {
        &self.overrides
    }
}

// ─── Module ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Module {
    chain: SignalChain,
}

impl Module {
    /// Create a module from a signal chain (supports parallel routing).
    pub fn from_chain(chain: SignalChain) -> Self {
        Self { chain }
    }

    /// Create a module from a flat list of blocks (pure series chain).
    ///
    /// Backward-compatible constructor — wraps blocks in [`SignalChain::serial`].
    pub fn from_blocks(blocks: Vec<ModuleBlock>) -> Self {
        Self {
            chain: SignalChain::serial(blocks),
        }
    }

    /// The full signal chain topology.
    pub fn chain(&self) -> &SignalChain {
        &self.chain
    }

    /// All blocks in depth-first order (flattening any parallel splits).
    ///
    /// Use this when you need a flat view regardless of topology — e.g.,
    /// counting blocks, iterating parameters, or building a template.
    pub fn blocks(&self) -> Vec<&ModuleBlock> {
        self.chain.blocks()
    }
}

// ─── ModuleSnapshot ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModuleSnapshot {
    id: ModuleSnapshotId,
    name: String,
    module: Module,
    #[serde(default)]
    metadata: metadata::Metadata,
    #[serde(default = "default_version")]
    version: u32,
}

impl ModuleSnapshot {
    pub fn new(id: impl Into<ModuleSnapshotId>, name: impl Into<String>, module: Module) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            module,
            metadata: metadata::Metadata::new(),
            version: 1,
        }
    }

    /// Create a module snapshot with an explicit version (used by storage layer on load).
    pub fn with_version(
        id: impl Into<ModuleSnapshotId>,
        name: impl Into<String>,
        module: Module,
        version: u32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            module,
            metadata: metadata::Metadata::new(),
            version,
        }
    }

    /// Create a module snapshot with explicit version + metadata (used by storage layer on load).
    pub fn with_version_and_metadata(
        id: impl Into<ModuleSnapshotId>,
        name: impl Into<String>,
        module: Module,
        version: u32,
        metadata: metadata::Metadata,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            module,
            metadata,
            version,
        }
    }

    pub fn id(&self) -> &ModuleSnapshotId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: metadata::Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Replace the module state. Used when saving module chain changes.
    pub fn set_module(&mut self, module: Module) {
        self.module = module;
    }

    /// Bump the version counter. Called by the storage layer when parameter values change.
    pub fn increment_version(&mut self) {
        self.version += 1;
    }
}

impl SnapshotLike for ModuleSnapshot {
    type Id = ModuleSnapshotId;
    type State = Module;

    fn id(&self) -> &Self::Id {
        self.id()
    }

    fn name(&self) -> &str {
        self.name()
    }

    fn state(&self) -> &Self::State {
        self.module()
    }
}

impl traits::Variant for ModuleSnapshot {
    type Id = ModuleSnapshotId;
    type BaseRef = ();
    type Override = ();
    fn id(&self) -> &ModuleSnapshotId {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
}

impl traits::DefaultVariant for ModuleSnapshot {
    fn default_named(name: impl Into<String>) -> Self {
        Self::new(
            ModuleSnapshotId::new(),
            name,
            Module::from_blocks(Vec::new()),
        )
    }
}

// ─── ModulePreset ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModulePreset {
    id: ModulePresetId,
    name: String,
    module_type: ModuleType,
    default_snapshot: ModuleSnapshot,
    snapshots: Vec<ModuleSnapshot>,
    #[serde(default)]
    metadata: metadata::Metadata,
}

impl ModulePreset {
    pub fn new(
        id: impl Into<ModulePresetId>,
        name: impl Into<String>,
        module_type: ModuleType,
        default_snapshot: ModuleSnapshot,
        additional_snapshots: Vec<ModuleSnapshot>,
    ) -> Self {
        let mut snapshots = Vec::with_capacity(additional_snapshots.len() + 1);
        snapshots.push(default_snapshot.clone());
        snapshots.extend(
            additional_snapshots
                .into_iter()
                .filter(|snapshot| snapshot.id() != default_snapshot.id()),
        );

        Self {
            id: id.into(),
            name: name.into(),
            module_type,
            default_snapshot,
            snapshots,
            metadata: metadata::Metadata::new(),
        }
    }

    pub fn id(&self) -> &ModulePresetId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn module_type(&self) -> ModuleType {
        self.module_type
    }

    pub fn snapshots(&self) -> &[ModuleSnapshot] {
        &self.snapshots
    }

    pub fn default_snapshot(&self) -> &ModuleSnapshot {
        &self.default_snapshot
    }

    pub fn snapshot(&self, snapshot_id: &ModuleSnapshotId) -> Option<ModuleSnapshot> {
        self.snapshots
            .iter()
            .find(|snapshot| snapshot.id() == snapshot_id)
            .cloned()
    }

    /// Add a snapshot to this module preset's variant list.
    pub fn add_snapshot(&mut self, snapshot: ModuleSnapshot) {
        if !self.snapshots.iter().any(|s| s.id == snapshot.id) {
            self.snapshots.push(snapshot);
        }
    }

    pub fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: metadata::Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

impl PresetLike for ModulePreset {
    type Id = ModulePresetId;
    type SnapshotId = ModuleSnapshotId;
    type Snapshot = ModuleSnapshot;

    fn id(&self) -> &Self::Id {
        self.id()
    }

    fn name(&self) -> &str {
        self.name()
    }

    fn snapshots(&self) -> &[Self::Snapshot] {
        self.snapshots()
    }

    fn default_snapshot_id(&self) -> &Self::SnapshotId {
        self.default_snapshot.id()
    }
}

impl traits::Collection for ModulePreset {
    type Variant = ModuleSnapshot;

    fn variants(&self) -> &[ModuleSnapshot] {
        &self.snapshots
    }
    fn variants_mut(&mut self) -> &mut Vec<ModuleSnapshot> {
        &mut self.snapshots
    }
    fn default_variant_id(&self) -> &ModuleSnapshotId {
        self.default_snapshot.id()
    }
    fn set_default_variant_id(&mut self, id: ModuleSnapshotId) {
        if let Some(snap) = self.snapshots.iter().find(|s| s.id == id) {
            self.default_snapshot = snap.clone();
        }
    }
}

impl traits::HasMetadata for ModuleSnapshot {
    fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut metadata::Metadata {
        &mut self.metadata
    }
}

impl traits::HasMetadata for ModulePreset {
    fn metadata(&self) -> &metadata::Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut metadata::Metadata {
        &mut self.metadata
    }
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::seed_id;

    #[test]
    fn preset_always_contains_default_snapshot() {
        let snap_default_id = SnapshotId::from_uuid(seed_id("snap-default"));
        let snap_extra_id = SnapshotId::from_uuid(seed_id("snap-extra"));
        let default = Snapshot::new(snap_default_id.clone(), "Default", Block::default());
        let duplicate = Snapshot::new(
            snap_default_id.clone(),
            "Duplicate",
            Block::new(0.1, 0.2, 0.3),
        );
        let extra = Snapshot::new(snap_extra_id.clone(), "Extra", Block::new(0.8, 0.1, 0.6));

        let preset = Preset::new(
            PresetId::from_uuid(seed_id("preset-a")),
            "Preset A",
            BlockType::Amp,
            default.clone(),
            vec![duplicate, extra],
        );

        assert_eq!(preset.default_snapshot(), default);
        assert_eq!(preset.block_type(), BlockType::Amp);
        assert_eq!(preset.snapshots().len(), 2);
        assert_eq!(preset.snapshots()[0].id(), &snap_default_id);
        assert_eq!(preset.snapshots()[1].id(), &snap_extra_id);
    }

    // -- Version tracking tests

    #[test]
    fn snapshot_starts_at_version_1() {
        let snap = Snapshot::new(SnapshotId::new(), "Test", Block::default());
        assert_eq!(snap.version(), 1);
    }

    #[test]
    fn snapshot_version_increments() {
        let mut snap = Snapshot::new(SnapshotId::new(), "Test", Block::default());
        assert_eq!(snap.version(), 1);
        snap.increment_version();
        assert_eq!(snap.version(), 2);
        snap.increment_version();
        assert_eq!(snap.version(), 3);
    }

    #[test]
    fn snapshot_with_version_sets_explicit_version() {
        let snap = Snapshot::with_version(SnapshotId::new(), "Test", Block::default(), 42);
        assert_eq!(snap.version(), 42);
    }

    #[test]
    fn module_snapshot_starts_at_version_1() {
        let ms = ModuleSnapshot::new(ModuleSnapshotId::new(), "Test", Module::from_blocks(vec![]));
        assert_eq!(ms.version(), 1);
    }

    #[test]
    fn module_snapshot_version_increments() {
        let mut ms =
            ModuleSnapshot::new(ModuleSnapshotId::new(), "Test", Module::from_blocks(vec![]));
        ms.increment_version();
        assert_eq!(ms.version(), 2);
    }

    #[test]
    fn module_block_source_saved_version() {
        let source = ModuleBlockSource::PresetDefault {
            preset_id: PresetId::new(),
            saved_at_version: None,
        };
        assert_eq!(source.saved_at_version(), None);

        let stamped = source.with_saved_version(3);
        assert_eq!(stamped.saved_at_version(), Some(3));
    }

    #[test]
    fn module_block_source_snapshot_saved_version() {
        let source = ModuleBlockSource::PresetSnapshot {
            preset_id: PresetId::new(),
            snapshot_id: SnapshotId::new(),
            saved_at_version: None,
        };
        let stamped = source.with_saved_version(5);
        assert_eq!(stamped.saved_at_version(), Some(5));

        // Inline always returns None
        let inline = ModuleBlockSource::Inline {
            block: Block::default(),
        };
        assert_eq!(inline.saved_at_version(), None);
        let still_inline = inline.with_saved_version(10);
        assert_eq!(still_inline.saved_at_version(), None);
    }

    #[test]
    fn snapshot_serde_round_trip() {
        let snap = Snapshot::new(SnapshotId::new(), "Test", Block::default());
        let json = serde_json::to_string(&snap).unwrap();
        let roundtrip: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, roundtrip);
    }

    #[test]
    fn module_block_source_serde_with_version() {
        let source = ModuleBlockSource::PresetSnapshot {
            preset_id: PresetId::new(),
            snapshot_id: SnapshotId::new(),
            saved_at_version: Some(7),
        };
        let json = serde_json::to_string(&source).unwrap();
        let roundtrip: ModuleBlockSource = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.saved_at_version(), Some(7));
    }
}
