//! Signal2 protocol types — domain model for rig control.
//!
//! ## Hierarchy
//!
//! **Physical**: Block → Module → Layer → Engine → Rig
//!
//! **Performance**: Profile (Patches) → Song (Sections)
//!
//! **Templates**: Structural blueprints with [`Assignment::Unassigned`](template::Assignment)
//! placeholders at every level.

use facet::Facet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Domain modules ─────────────────────────────────────────────
pub mod actions;
pub mod automation;
pub mod block;
pub mod builder;
pub mod catalog;
pub mod defaults;
pub mod engine;
pub mod fx_send;
pub mod layer;
pub mod metadata;
pub mod midi;
pub mod midi_actions;
pub mod module_type;
pub mod override_policy;
pub mod overrides;
pub mod plugin_block;
pub mod profile;
pub mod rack;
pub mod resolve;
pub mod rig;
pub mod rig_template;
pub mod routing;
pub mod scene_template;
pub mod setlist;
pub mod signal_chain;
pub mod song;
pub mod tagging;
pub mod template;
pub mod traits;
pub mod versioning;

// ─── Re-exported from macromod ──────────────────────────────────
pub use macromod::easing;
pub use macromod::macro_bank;
pub use macromod::curation as param_curation;
pub use macromod::runtime;
pub use macromod::{BlockParameter, MacroBinding, ParameterValue, ParamTarget, ResponseCurve};

/// Backward-compatible `modulation` module path.
pub mod modulation {
    pub use macromod::sources::*;
    pub use macromod::routing::*;
}

pub use block::*;
pub use module_type::*;
pub use signal_chain::*;

/// Shared contract for generating globally unique IDs at runtime.
pub trait IdFactory: Send + Sync {
    fn new_uuid(&self) -> Uuid;
}

/// Default runtime ID factory. Uses UUIDv7 for sortable, globally unique IDs.
#[derive(Debug, Default, Clone, Copy)]
pub struct RuntimeIdFactory;

impl IdFactory for RuntimeIdFactory {
    fn new_uuid(&self) -> Uuid {
        Uuid::now_v7()
    }
}

/// Creates a branded string ID type with Display, From, and AsRef impls.
/// Used for categorical IDs like `RigTypeId` that remain human-readable strings.
#[macro_export]
macro_rules! typed_string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, facet::Facet)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

/// Creates a branded UUID ID type backed by String for Facet compatibility.
/// Generates v7 UUIDs on `new()`, validates UUID format on `From<&str>`.
/// Used for all entity IDs that need global uniqueness for online sharing.
#[macro_export]
macro_rules! typed_uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, facet::Facet)]
        pub struct $name(String);

        impl $name {
            /// Generate a new random v7 UUID.
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7().to_string())
            }

            /// Wrap an existing UUID.
            pub fn from_uuid(uuid: uuid::Uuid) -> Self {
                Self(uuid.to_string())
            }

            /// Get the string representation.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Parse back to a UUID value.
            ///
            /// # Panics
            /// Panics if the inner string is not a valid UUID. Prefer
            /// [`try_to_uuid`](Self::try_to_uuid) at untrusted boundaries.
            pub fn to_uuid(&self) -> uuid::Uuid {
                self.0.parse().expect(concat!(
                    "corrupted UUID in ",
                    stringify!($name)
                ))
            }

            /// Try to parse back to a UUID value, returning an error on invalid data.
            pub fn try_to_uuid(&self) -> Result<uuid::Uuid, uuid::Error> {
                self.0.parse()
            }

            /// Parse a string into this ID type, returning an error if it is not a valid UUID.
            ///
            /// Use this at untrusted boundaries (deserialization, user input) instead of
            /// `From<String>` which panics on invalid input.
            pub fn try_parse(value: impl Into<String>) -> Result<Self, uuid::Error> {
                let s = value.into();
                let _: uuid::Uuid = s.parse()?;
                Ok(Self(s))
            }

            /// Consume and return the inner string.
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<uuid::Uuid> for $name {
            fn from(value: uuid::Uuid) -> Self {
                Self(value.to_string())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                // Validate UUID format
                let _: uuid::Uuid = value.parse().expect(concat!(
                    "invalid UUID string for ",
                    stringify!($name)
                ));
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                // Validate UUID format
                let _: uuid::Uuid = value.parse().expect(concat!(
                    "invalid UUID string for ",
                    stringify!($name)
                ));
                Self(value.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

/// Namespace UUID for deterministic seed data IDs (v5).
pub const SEED_UUID_NS: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x51, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

/// Generate a deterministic UUID from a human-readable name.
/// Same name always produces same UUID — used for seed data and tests.
pub fn seed_id(name: &str) -> Uuid {
    Uuid::new_v5(&SEED_UUID_NS, name.as_bytes())
}

typed_uuid_id!(
    /// Branded type for preset identifiers.
    PresetId
);
typed_uuid_id!(
    /// Branded type for snapshot identifiers.
    SnapshotId
);
typed_uuid_id!(
    /// Branded type for module preset identifiers.
    ModulePresetId
);
typed_uuid_id!(
    /// Branded type for module snapshot identifiers.
    ModuleSnapshotId
);

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Block {
    parameters: Vec<BlockParameter>,
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
            BlockParameter::new("param_1", "Parameter 1", param_1),
            BlockParameter::new("param_2", "Parameter 2", param_2),
            BlockParameter::new("param_3", "Parameter 3", param_3),
        ])
    }

    pub fn from_parameters(parameters: Vec<BlockParameter>) -> Self {
        let parameters = if parameters.is_empty() {
            vec![BlockParameter::new("value", "Value", 0.5)]
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

    pub fn parameters(&self) -> &[BlockParameter] {
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

fn default_version() -> u32 {
    1
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

#[roam::service]
pub trait BlockService {
    async fn get_block(&self, block_type: BlockType) -> Result<Block, String>;
    async fn set_block(&self, block_type: BlockType, block: Block) -> Result<Block, String>;
    async fn list_block_presets(&self, block_type: BlockType) -> Result<Vec<Preset>, String>;
    async fn load_block_preset(
        &self,
        block_type: BlockType,
        preset_id: PresetId,
    ) -> Result<Option<Snapshot>, String>;
    async fn load_block_preset_snapshot(
        &self,
        block_type: BlockType,
        preset_id: PresetId,
        snapshot_id: SnapshotId,
    ) -> Result<Option<Snapshot>, String>;
    async fn save_block_preset(&self, preset: Preset) -> Result<(), String>;
    async fn delete_block_preset(
        &self,
        block_type: BlockType,
        preset_id: PresetId,
    ) -> Result<(), String>;
    async fn list_module_presets(&self) -> Result<Vec<ModulePreset>, String>;
    async fn load_module_preset(
        &self,
        preset_id: ModulePresetId,
    ) -> Result<Option<ModuleSnapshot>, String>;
    async fn load_module_preset_snapshot(
        &self,
        preset_id: ModulePresetId,
        snapshot_id: ModuleSnapshotId,
    ) -> Result<Option<ModuleSnapshot>, String>;
    async fn save_module_collection(&self, preset: ModulePreset) -> Result<(), String>;
    async fn delete_module_collection(&self, id: ModulePresetId) -> Result<(), String>;
}

#[roam::service]
pub trait LayerService {
    async fn list_layers(&self) -> Result<Vec<layer::Layer>, String>;
    async fn load_layer(&self, id: layer::LayerId) -> Result<Option<layer::Layer>, String>;
    async fn save_layer(&self, layer: layer::Layer) -> Result<(), String>;
    async fn delete_layer(&self, id: layer::LayerId) -> Result<(), String>;
    async fn load_layer_variant(
        &self,
        layer_id: layer::LayerId,
        variant_id: layer::LayerSnapshotId,
    ) -> Result<Option<layer::LayerSnapshot>, String>;
}

#[roam::service]
pub trait EngineService {
    async fn list_engines(&self) -> Result<Vec<engine::Engine>, String>;
    async fn load_engine(&self, id: engine::EngineId) -> Result<Option<engine::Engine>, String>;
    async fn save_engine(&self, engine: engine::Engine) -> Result<(), String>;
    async fn delete_engine(&self, id: engine::EngineId) -> Result<(), String>;
    async fn load_engine_variant(
        &self,
        engine_id: engine::EngineId,
        variant_id: engine::EngineSceneId,
    ) -> Result<Option<engine::EngineScene>, String>;
}

#[roam::service]
pub trait RigService {
    async fn list_rigs(&self) -> Result<Vec<rig::Rig>, String>;
    async fn load_rig(&self, id: rig::RigId) -> Result<Option<rig::Rig>, String>;
    async fn save_rig(&self, rig: rig::Rig) -> Result<(), String>;
    async fn delete_rig(&self, id: rig::RigId) -> Result<(), String>;
    async fn load_rig_variant(
        &self,
        rig_id: rig::RigId,
        variant_id: rig::RigSceneId,
    ) -> Result<Option<rig::RigScene>, String>;
}

#[roam::service]
pub trait ProfileService {
    async fn list_profiles(&self) -> Result<Vec<profile::Profile>, String>;
    async fn load_profile(
        &self,
        id: profile::ProfileId,
    ) -> Result<Option<profile::Profile>, String>;
    async fn save_profile(&self, profile: profile::Profile) -> Result<(), String>;
    async fn delete_profile(&self, id: profile::ProfileId) -> Result<(), String>;
    async fn load_profile_variant(
        &self,
        profile_id: profile::ProfileId,
        variant_id: profile::PatchId,
    ) -> Result<Option<profile::Patch>, String>;
}

#[roam::service]
pub trait SongService {
    async fn list_songs(&self) -> Result<Vec<song::Song>, String>;
    async fn load_song(&self, id: song::SongId) -> Result<Option<song::Song>, String>;
    async fn save_song(&self, song: song::Song) -> Result<(), String>;
    async fn delete_song(&self, id: song::SongId) -> Result<(), String>;
    async fn load_song_variant(
        &self,
        song_id: song::SongId,
        variant_id: song::SectionId,
    ) -> Result<Option<song::Section>, String>;
}

#[roam::service]
pub trait SetlistService {
    async fn list_setlists(&self) -> Result<Vec<setlist::Setlist>, String>;
    async fn load_setlist(
        &self,
        id: setlist::SetlistId,
    ) -> Result<Option<setlist::Setlist>, String>;
    async fn save_setlist(&self, setlist: setlist::Setlist) -> Result<(), String>;
    async fn delete_setlist(&self, id: setlist::SetlistId) -> Result<(), String>;
    async fn load_setlist_entry(
        &self,
        setlist_id: setlist::SetlistId,
        entry_id: setlist::SetlistEntryId,
    ) -> Result<Option<setlist::SetlistEntry>, String>;
}

#[roam::service]
pub trait BrowserService {
    async fn browser_index(&self) -> Result<tagging::BrowserIndex, String>;
    async fn browse(
        &self,
        query: tagging::BrowserQuery,
    ) -> Result<Vec<tagging::BrowserHit>, String>;
}

#[roam::service]
pub trait ResolveService {
    async fn resolve_target(
        &self,
        target: resolve::ResolveTarget,
    ) -> Result<resolve::ResolvedGraph, resolve::ResolveError>;
}

#[roam::service]
pub trait SceneTemplateService {
    async fn list_scene_templates(&self) -> Result<Vec<scene_template::SceneTemplate>, String>;
    async fn load_scene_template(
        &self,
        id: scene_template::SceneTemplateId,
    ) -> Result<Option<scene_template::SceneTemplate>, String>;
    async fn save_scene_template(
        &self,
        template: scene_template::SceneTemplate,
    ) -> Result<(), String>;
    async fn delete_scene_template(
        &self,
        id: scene_template::SceneTemplateId,
    ) -> Result<(), String>;
    async fn reorder_scene_templates(
        &self,
        ordered_ids: Vec<scene_template::SceneTemplateId>,
    ) -> Result<(), String>;
}

#[roam::service]
pub trait RackService {
    async fn list_racks(&self) -> Result<Vec<rack::Rack>, String>;
    async fn load_rack(&self, id: rack::RackId) -> Result<Option<rack::Rack>, String>;
    async fn save_rack(&self, rack: rack::Rack) -> Result<(), String>;
    async fn delete_rack(&self, id: rack::RackId) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn seed_id_is_deterministic() {
        let a = seed_id("test-name");
        let b = seed_id("test-name");
        assert_eq!(a, b);

        let c = seed_id("different-name");
        assert_ne!(a, c);
    }

    #[test]
    fn uuid_id_from_string_round_trip() {
        let id = PresetId::new();
        let s = id.to_string();
        let parsed = PresetId::from(s);
        assert_eq!(id, parsed);
    }
}
