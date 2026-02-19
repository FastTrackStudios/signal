//! Template system — structural blueprints at every level (Block → Song).
//!
//! Templates capture the **shape** of a domain entity without binding to
//! specific plugin assignments, parameter state, or variant references.
//!
//! ## Key concepts
//!
//! - [`Assignment`] — wraps a slot that may or may not be filled.
//!   `Assignment::Unassigned` marks template placeholders awaiting user input.
//!
//! - [`Templateable`] — trait for types that can produce and consume templates.
//!   `to_template()` strips bindings; `instantiate()` fails if any required
//!   assignments are still `Unassigned`.
//!
//! - [`TemplateMetadata`] — tags, description, and notes for templates.
//!
//! - [`InstantiateError`] — typed error listing which assignments are missing.
//!
//! ## Hierarchy
//!
//! Templates exist at every level: Block, Module, Layer, Engine, Rig,
//! Profile, and Song.

use std::fmt;

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::engine::EngineId;
use crate::layer::LayerId;
use crate::metadata::{Metadata, Tags};
use crate::profile::ProfileId;
use crate::rig::{RigId, RigTypeId};
use crate::song::SongId;
use crate::{BlockType, PresetId};

// ─── Assignment ─────────────────────────────────────────────────

/// A slot that may or may not be filled. Template placeholders use
/// `Unassigned` until the user picks a concrete value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum Assignment<T> {
    /// Not yet assigned — a placeholder slot.
    Unassigned,
    /// A concrete assignment.
    Assigned(T),
}

impl<T> Assignment<T> {
    pub fn is_unassigned(&self) -> bool {
        matches!(self, Self::Unassigned)
    }

    pub fn is_assigned(&self) -> bool {
        matches!(self, Self::Assigned(_))
    }

    pub fn as_ref(&self) -> Assignment<&T> {
        match self {
            Self::Unassigned => Assignment::Unassigned,
            Self::Assigned(v) => Assignment::Assigned(v),
        }
    }

    pub fn assigned(&self) -> Option<&T> {
        match self {
            Self::Unassigned => None,
            Self::Assigned(v) => Some(v),
        }
    }
}

impl<T: fmt::Display> fmt::Display for Assignment<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unassigned => write!(f, "<unassigned>"),
            Self::Assigned(v) => write!(f, "{v}"),
        }
    }
}

// ─── Instantiation errors ───────────────────────────────────────

/// Which level a missing assignment belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentLevel {
    Block,
    Module,
    Layer,
    Engine,
    Rig,
    Profile,
    Song,
}

impl fmt::Display for AssignmentLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Block => write!(f, "block"),
            Self::Module => write!(f, "module"),
            Self::Layer => write!(f, "layer"),
            Self::Engine => write!(f, "engine"),
            Self::Rig => write!(f, "rig"),
            Self::Profile => write!(f, "profile"),
            Self::Song => write!(f, "song"),
        }
    }
}

/// A single missing assignment within a template.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingAssignment {
    /// Which hierarchy level the missing assignment belongs to.
    pub level: AssignmentLevel,
    /// Human-readable description of what needs to be assigned.
    pub slot: String,
}

impl fmt::Display for MissingAssignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.level, self.slot)
    }
}

/// Error returned when `instantiate()` fails because required assignments
/// are still `Unassigned`.
#[derive(Debug, Clone, PartialEq)]
pub struct InstantiateError {
    pub missing: Vec<MissingAssignment>,
}

impl fmt::Display for InstantiateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "missing assignments: ")?;
        for (i, m) in self.missing.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{m}")?;
        }
        Ok(())
    }
}

impl std::error::Error for InstantiateError {}

impl InstantiateError {
    pub fn new(missing: Vec<MissingAssignment>) -> Self {
        Self { missing }
    }
}

// ─── Template metadata ──────────────────────────────────────────

/// Shared metadata for templates — tags, description, notes.
///
/// Separate from [`Metadata`] to allow templates to carry independent
/// metadata that doesn't transfer to instantiated instances.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Facet)]
pub struct TemplateMetadata {
    pub tags: Tags,
    pub description: Option<String>,
    pub notes: Option<String>,
}

impl TemplateMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.add(tag);
        self
    }
}

impl From<Metadata> for TemplateMetadata {
    fn from(m: Metadata) -> Self {
        Self {
            tags: m.tags,
            description: m.description,
            notes: m.notes,
        }
    }
}

// ─── Templateable trait ─────────────────────────────────────────

/// Types that can produce and consume structural templates.
///
/// `to_template()` extracts the structural blueprint.
/// `instantiate()` validates all assignments are filled, then creates an instance.
pub trait Templateable: Sized {
    type Template;

    /// Extract a structural template — strips bindings and state.
    fn to_template(&self) -> Self::Template;

    /// Attempt to create an instance from a template. Fails if any
    /// required [`Assignment`] slots are still `Unassigned`.
    fn instantiate(template: &Self::Template) -> Result<Self, InstantiateError>;
}

// ─── BlockTemplate ──────────────────────────────────────────────

/// A block slot — knows what type of DSP it needs but not which plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct BlockTemplate {
    pub block_type: BlockType,
    pub name: String,
    pub preset_id: Assignment<PresetId>,
    pub metadata: TemplateMetadata,
}

impl BlockTemplate {
    pub fn new(name: impl Into<String>, block_type: BlockType) -> Self {
        Self {
            block_type,
            name: name.into(),
            preset_id: Assignment::Unassigned,
            metadata: TemplateMetadata::new(),
        }
    }

    /// Create a block template with a preset already assigned.
    pub fn assigned(
        name: impl Into<String>,
        block_type: BlockType,
        preset_id: impl Into<PresetId>,
    ) -> Self {
        Self {
            block_type,
            name: name.into(),
            preset_id: Assignment::Assigned(preset_id.into()),
            metadata: TemplateMetadata::new(),
        }
    }

    #[must_use]
    pub fn with_preset(mut self, preset_id: impl Into<PresetId>) -> Self {
        self.preset_id = Assignment::Assigned(preset_id.into());
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: TemplateMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

// ─── Signal chain templates ─────────────────────────────────────

/// Template for a signal processing node — either a block or a parallel split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum SignalNodeTemplate {
    /// A single block slot.
    Block(BlockTemplate),
    /// Parallel split into independent lanes.
    Split { lanes: Vec<SignalChainTemplate> },
}

/// Template for an ordered signal chain — mirrors [`SignalChain`](crate::SignalChain).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SignalChainTemplate {
    pub nodes: Vec<SignalNodeTemplate>,
}

impl SignalChainTemplate {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Create a serial chain template from block templates.
    pub fn serial(blocks: Vec<BlockTemplate>) -> Self {
        Self {
            nodes: blocks.into_iter().map(SignalNodeTemplate::Block).collect(),
        }
    }

    #[must_use]
    pub fn with_node(mut self, node: SignalNodeTemplate) -> Self {
        self.nodes.push(node);
        self
    }

    #[must_use]
    pub fn with_block(mut self, block: BlockTemplate) -> Self {
        self.nodes.push(SignalNodeTemplate::Block(block));
        self
    }

    /// Collect missing assignments recursively.
    pub fn missing_assignments(&self) -> Vec<MissingAssignment> {
        let mut missing = Vec::new();
        for node in &self.nodes {
            match node {
                SignalNodeTemplate::Block(b) => {
                    if b.preset_id.is_unassigned() {
                        missing.push(MissingAssignment {
                            level: AssignmentLevel::Block,
                            slot: format!("{} ({})", b.name, b.block_type.as_str()),
                        });
                    }
                }
                SignalNodeTemplate::Split { lanes } => {
                    for lane in lanes {
                        missing.extend(lane.missing_assignments());
                    }
                }
            }
        }
        missing
    }
}

impl Default for SignalChainTemplate {
    fn default() -> Self {
        Self::new()
    }
}

// ─── ModuleTemplate ─────────────────────────────────────────────

/// A module slot — knows its signal chain structure but not specific presets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ModuleTemplate {
    pub name: String,
    pub module_type: crate::ModuleType,
    pub chain: SignalChainTemplate,
    pub metadata: TemplateMetadata,
}

impl ModuleTemplate {
    pub fn new(name: impl Into<String>, module_type: crate::ModuleType) -> Self {
        Self {
            name: name.into(),
            module_type,
            chain: SignalChainTemplate::new(),
            metadata: TemplateMetadata::new(),
        }
    }

    /// Append a block to the chain (convenience for serial templates).
    #[must_use]
    pub fn with_block(mut self, block: BlockTemplate) -> Self {
        self.chain.nodes.push(SignalNodeTemplate::Block(block));
        self
    }

    /// Append an arbitrary signal node to the chain.
    #[must_use]
    pub fn with_node(mut self, node: SignalNodeTemplate) -> Self {
        self.chain.nodes.push(node);
        self
    }

    /// Set the full signal chain template.
    #[must_use]
    pub fn with_chain(mut self, chain: SignalChainTemplate) -> Self {
        self.chain = chain;
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: TemplateMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Collect missing assignments from all blocks in the chain.
    pub fn missing_assignments(&self) -> Vec<MissingAssignment> {
        self.chain.missing_assignments()
    }
}

// ─── LayerTemplate ──────────────────────────────────────────────

/// A layer slot — knows its module structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct LayerTemplate {
    pub name: String,
    pub layer_id: Assignment<LayerId>,
    pub modules: Vec<ModuleTemplate>,
    pub metadata: TemplateMetadata,
}

impl LayerTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            layer_id: Assignment::Unassigned,
            modules: Vec::new(),
            metadata: TemplateMetadata::new(),
        }
    }

    #[must_use]
    pub fn with_layer_id(mut self, id: impl Into<LayerId>) -> Self {
        self.layer_id = Assignment::Assigned(id.into());
        self
    }

    #[must_use]
    pub fn with_module(mut self, module: ModuleTemplate) -> Self {
        self.modules.push(module);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: TemplateMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn missing_assignments(&self) -> Vec<MissingAssignment> {
        let mut missing = Vec::new();
        if self.layer_id.is_unassigned() {
            missing.push(MissingAssignment {
                level: AssignmentLevel::Layer,
                slot: format!("layer_id for '{}'", self.name),
            });
        }
        for module in &self.modules {
            missing.extend(module.missing_assignments());
        }
        missing
    }
}

// ─── EngineTemplate ─────────────────────────────────────────────

/// An engine slot — knows its layer structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct EngineTemplate {
    pub name: String,
    pub engine_id: Assignment<EngineId>,
    pub layers: Vec<LayerTemplate>,
    pub metadata: TemplateMetadata,
}

impl EngineTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            engine_id: Assignment::Unassigned,
            layers: Vec::new(),
            metadata: TemplateMetadata::new(),
        }
    }

    #[must_use]
    pub fn with_engine_id(mut self, id: impl Into<EngineId>) -> Self {
        self.engine_id = Assignment::Assigned(id.into());
        self
    }

    #[must_use]
    pub fn with_layer(mut self, layer: LayerTemplate) -> Self {
        self.layers.push(layer);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: TemplateMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn missing_assignments(&self) -> Vec<MissingAssignment> {
        let mut missing = Vec::new();
        if self.engine_id.is_unassigned() {
            missing.push(MissingAssignment {
                level: AssignmentLevel::Engine,
                slot: format!("engine_id for '{}'", self.name),
            });
        }
        for layer in &self.layers {
            missing.extend(layer.missing_assignments());
        }
        missing
    }
}

// ─── RigTemplate ────────────────────────────────────────────────

/// A rig slot — knows its engine structure and optional rig type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct RigTemplate {
    pub name: String,
    pub rig_id: Assignment<RigId>,
    pub rig_type_id: Option<RigTypeId>,
    pub engines: Vec<EngineTemplate>,
    pub metadata: TemplateMetadata,
}

impl RigTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rig_id: Assignment::Unassigned,
            rig_type_id: None,
            engines: Vec::new(),
            metadata: TemplateMetadata::new(),
        }
    }

    #[must_use]
    pub fn with_rig_id(mut self, id: impl Into<RigId>) -> Self {
        self.rig_id = Assignment::Assigned(id.into());
        self
    }

    #[must_use]
    pub fn with_rig_type(mut self, rig_type_id: impl Into<RigTypeId>) -> Self {
        self.rig_type_id = Some(rig_type_id.into());
        self
    }

    #[must_use]
    pub fn with_engine(mut self, engine: EngineTemplate) -> Self {
        self.engines.push(engine);
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: TemplateMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn missing_assignments(&self) -> Vec<MissingAssignment> {
        let mut missing = Vec::new();
        if self.rig_id.is_unassigned() {
            missing.push(MissingAssignment {
                level: AssignmentLevel::Rig,
                slot: format!("rig_id for '{}'", self.name),
            });
        }
        for engine in &self.engines {
            missing.extend(engine.missing_assignments());
        }
        missing
    }
}

// ─── ProfileTemplate ────────────────────────────────────────────

/// A profile template — structure for a named configuration set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct ProfileTemplate {
    pub name: String,
    pub profile_id: Assignment<ProfileId>,
    pub rig_template: Option<RigTemplate>,
    pub patch_names: Vec<String>,
    pub metadata: TemplateMetadata,
}

impl ProfileTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            profile_id: Assignment::Unassigned,
            rig_template: None,
            patch_names: Vec::new(),
            metadata: TemplateMetadata::new(),
        }
    }

    #[must_use]
    pub fn with_profile_id(mut self, id: impl Into<ProfileId>) -> Self {
        self.profile_id = Assignment::Assigned(id.into());
        self
    }

    #[must_use]
    pub fn with_rig_template(mut self, rig: RigTemplate) -> Self {
        self.rig_template = Some(rig);
        self
    }

    #[must_use]
    pub fn with_patch_name(mut self, name: impl Into<String>) -> Self {
        self.patch_names.push(name.into());
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: TemplateMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn missing_assignments(&self) -> Vec<MissingAssignment> {
        let mut missing = Vec::new();
        if self.profile_id.is_unassigned() {
            missing.push(MissingAssignment {
                level: AssignmentLevel::Profile,
                slot: format!("profile_id for '{}'", self.name),
            });
        }
        if let Some(rig) = &self.rig_template {
            missing.extend(rig.missing_assignments());
        }
        missing
    }
}

// ─── SongTemplate ───────────────────────────────────────────────

/// A song template — structure for a performance with named sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct SongTemplate {
    pub name: String,
    pub song_id: Assignment<SongId>,
    pub artist: Option<String>,
    pub section_names: Vec<String>,
    pub metadata: TemplateMetadata,
}

impl SongTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            song_id: Assignment::Unassigned,
            artist: None,
            section_names: Vec::new(),
            metadata: TemplateMetadata::new(),
        }
    }

    #[must_use]
    pub fn with_song_id(mut self, id: impl Into<SongId>) -> Self {
        self.song_id = Assignment::Assigned(id.into());
        self
    }

    #[must_use]
    pub fn with_artist(mut self, artist: impl Into<String>) -> Self {
        self.artist = Some(artist.into());
        self
    }

    #[must_use]
    pub fn with_section(mut self, name: impl Into<String>) -> Self {
        self.section_names.push(name.into());
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, metadata: TemplateMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn missing_assignments(&self) -> Vec<MissingAssignment> {
        let mut missing = Vec::new();
        if self.song_id.is_unassigned() {
            missing.push(MissingAssignment {
                level: AssignmentLevel::Song,
                slot: format!("song_id for '{}'", self.name),
            });
        }
        missing
    }
}

// ─── Templateable implementations ───────────────────────────────

// Block → BlockTemplate (strips preset binding, keeps type and name)
impl Templateable for crate::Block {
    type Template = BlockTemplate;

    fn to_template(&self) -> BlockTemplate {
        BlockTemplate {
            block_type: BlockType::Amp, // default — actual block type is parameter-implied
            name: "Block".to_string(),
            preset_id: Assignment::Unassigned,
            metadata: TemplateMetadata::new(),
        }
    }

    fn instantiate(template: &BlockTemplate) -> Result<Self, InstantiateError> {
        if template.preset_id.is_unassigned() {
            return Err(InstantiateError::new(vec![MissingAssignment {
                level: AssignmentLevel::Block,
                slot: format!("{} ({})", template.name, template.block_type.as_str()),
            }]));
        }
        // With an assigned preset, create a default block
        Ok(crate::Block::default())
    }
}

// Module → ModuleTemplate (strips block preset bindings, keeps chain structure)
impl Templateable for crate::Module {
    type Template = ModuleTemplate;

    fn to_template(&self) -> ModuleTemplate {
        fn chain_to_template(chain: &crate::SignalChain) -> SignalChainTemplate {
            SignalChainTemplate {
                nodes: chain
                    .nodes()
                    .iter()
                    .map(|node| match node {
                        crate::SignalNode::Block(mb) => SignalNodeTemplate::Block(BlockTemplate {
                            block_type: mb.block_type(),
                            name: mb.label().to_string(),
                            preset_id: match mb.source() {
                                crate::ModuleBlockSource::PresetDefault { preset_id, .. } => {
                                    Assignment::Assigned(preset_id.clone())
                                }
                                crate::ModuleBlockSource::PresetSnapshot { preset_id, .. } => {
                                    Assignment::Assigned(preset_id.clone())
                                }
                                crate::ModuleBlockSource::Inline { .. } => Assignment::Unassigned,
                            },
                            metadata: TemplateMetadata::new(),
                        }),
                        crate::SignalNode::Split { lanes } => SignalNodeTemplate::Split {
                            lanes: lanes.iter().map(chain_to_template).collect(),
                        },
                    })
                    .collect(),
            }
        }

        ModuleTemplate {
            name: "Module".to_string(),
            module_type: crate::ModuleType::default(),
            chain: chain_to_template(self.chain()),
            metadata: TemplateMetadata::new(),
        }
    }

    fn instantiate(template: &ModuleTemplate) -> Result<Self, InstantiateError> {
        let missing = template.missing_assignments();
        if !missing.is_empty() {
            return Err(InstantiateError::new(missing));
        }

        fn instantiate_chain(
            chain: &SignalChainTemplate,
            counter: &mut usize,
        ) -> crate::SignalChain {
            let nodes = chain
                .nodes
                .iter()
                .map(|node| match node {
                    SignalNodeTemplate::Block(bt) => {
                        let preset_id = bt.preset_id.assigned().unwrap();
                        let idx = *counter;
                        *counter += 1;
                        crate::SignalNode::Block(crate::ModuleBlock::new(
                            format!("block-{idx}"),
                            &bt.name,
                            bt.block_type,
                            crate::ModuleBlockSource::PresetDefault {
                                preset_id: preset_id.clone(),
                                saved_at_version: None,
                            },
                        ))
                    }
                    SignalNodeTemplate::Split { lanes } => crate::SignalNode::Split {
                        lanes: lanes
                            .iter()
                            .map(|lane| instantiate_chain(lane, counter))
                            .collect(),
                    },
                })
                .collect();
            crate::SignalChain::new(nodes)
        }

        let mut counter = 0;
        let chain = instantiate_chain(&template.chain, &mut counter);
        Ok(crate::Module::from_chain(chain))
    }
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{seed_id, ModuleType};

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    // -- Assignment tests

    #[test]
    fn test_assignment_unassigned() {
        let a: Assignment<String> = Assignment::Unassigned;
        assert!(a.is_unassigned());
        assert!(!a.is_assigned());
        assert!(a.assigned().is_none());
    }

    #[test]
    fn test_assignment_assigned() {
        let a = Assignment::Assigned("hello".to_string());
        assert!(!a.is_unassigned());
        assert!(a.is_assigned());
        assert_eq!(a.assigned(), Some(&"hello".to_string()));
    }

    #[test]
    fn test_assignment_display() {
        let unassigned: Assignment<String> = Assignment::Unassigned;
        assert_eq!(format!("{unassigned}"), "<unassigned>");

        let assigned = Assignment::Assigned("my-preset".to_string());
        assert_eq!(format!("{assigned}"), "my-preset");
    }

    // -- Template metadata tests

    #[test]
    fn test_template_metadata_builder() {
        let meta = TemplateMetadata::new()
            .with_description("A guitar template")
            .with_notes("Start with this for rock tones")
            .with_tag("guitar")
            .with_tag("rock");

        assert_eq!(meta.description.as_deref(), Some("A guitar template"));
        assert_eq!(
            meta.notes.as_deref(),
            Some("Start with this for rock tones")
        );
        assert_eq!(meta.tags.len(), 2);
        assert!(meta.tags.contains("guitar"));
    }

    // -- BlockTemplate tests

    #[test]
    fn test_block_template_unassigned() {
        let template = BlockTemplate::new("Drive", BlockType::Drive);
        assert!(template.preset_id.is_unassigned());
    }

    #[test]
    fn test_block_template_assigned() {
        let pid = PresetId::from_uuid(seed_id("preset-od1"));
        let template = BlockTemplate::assigned("Drive", BlockType::Drive, pid.clone());
        assert!(template.preset_id.is_assigned());
        assert_eq!(template.preset_id.assigned().unwrap(), &pid);
    }

    #[test]
    fn test_block_instantiate_fails_when_unassigned() {
        let template = BlockTemplate::new("Drive", BlockType::Drive);
        let result = crate::Block::instantiate(&template);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.missing.len(), 1);
        assert_eq!(err.missing[0].level, AssignmentLevel::Block);
        assert!(err.missing[0].slot.contains("Drive"));
    }

    #[test]
    fn test_block_instantiate_succeeds_when_assigned() -> Result<()> {
        let template = BlockTemplate::new("Drive", BlockType::Drive)
            .with_preset(PresetId::from_uuid(seed_id("preset-od1")));
        let block = crate::Block::instantiate(&template)?;
        assert!(block.first_value().is_some());
        Ok(())
    }

    // -- ModuleTemplate tests

    #[test]
    fn test_module_template_missing_assignments() {
        let template = ModuleTemplate::new("Drive Section", ModuleType::Drive)
            .with_block(BlockTemplate::new("OD", BlockType::Drive))
            .with_block(BlockTemplate::assigned(
                "Amp",
                BlockType::Amp,
                PresetId::from_uuid(seed_id("preset-amp1")),
            ));

        let missing = template.missing_assignments();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].level, AssignmentLevel::Block);
        assert!(missing[0].slot.contains("OD"));
    }

    #[test]
    fn test_module_instantiate_fails_with_unassigned_blocks() {
        let template = ModuleTemplate::new("Drive Section", ModuleType::Drive)
            .with_block(BlockTemplate::new("OD", BlockType::Drive));

        let result = crate::Module::instantiate(&template);
        assert!(result.is_err());
    }

    #[test]
    fn test_module_instantiate_succeeds() -> Result<()> {
        let template = ModuleTemplate::new("Drive Section", ModuleType::Drive)
            .with_block(BlockTemplate::assigned(
                "OD",
                BlockType::Drive,
                PresetId::from_uuid(seed_id("preset-od1")),
            ))
            .with_block(BlockTemplate::assigned(
                "Amp",
                BlockType::Amp,
                PresetId::from_uuid(seed_id("preset-amp1")),
            ));

        let module = crate::Module::instantiate(&template)?;
        assert_eq!(module.blocks().len(), 2);
        Ok(())
    }

    // -- LayerTemplate tests

    #[test]
    fn test_layer_template_missing_assignments() {
        let template = LayerTemplate::new("Main Layer").with_module(
            ModuleTemplate::new("Drive", ModuleType::Drive)
                .with_block(BlockTemplate::new("OD", BlockType::Drive)),
        );

        let missing = template.missing_assignments();
        // layer_id unassigned + block unassigned
        assert_eq!(missing.len(), 2);
        assert!(missing.iter().any(|m| m.level == AssignmentLevel::Layer));
        assert!(missing.iter().any(|m| m.level == AssignmentLevel::Block));
    }

    #[test]
    fn test_layer_template_fully_assigned() {
        let template = LayerTemplate::new("Main Layer")
            .with_layer_id(LayerId::from_uuid(seed_id("layer-1")))
            .with_module(ModuleTemplate::new("Drive", ModuleType::Drive).with_block(
                BlockTemplate::assigned("OD", BlockType::Drive, PresetId::from_uuid(seed_id("p1"))),
            ));

        assert!(template.missing_assignments().is_empty());
    }

    // -- EngineTemplate tests

    #[test]
    fn test_engine_template_cascading_missing() {
        let template = EngineTemplate::new("Guitar Engine").with_layer(
            LayerTemplate::new("Layer 1").with_module(
                ModuleTemplate::new("Drive", ModuleType::Drive)
                    .with_block(BlockTemplate::new("OD", BlockType::Drive)),
            ),
        );

        let missing = template.missing_assignments();
        // engine_id + layer_id + block preset
        assert_eq!(missing.len(), 3);
    }

    // -- RigTemplate tests

    #[test]
    fn test_rig_template_with_metadata() {
        let template = RigTemplate::new("Guitar Rig")
            .with_rig_type("guitar")
            .with_metadata(
                TemplateMetadata::new()
                    .with_description("Standard guitar template")
                    .with_tag("guitar"),
            )
            .with_engine(
                EngineTemplate::new("Main Engine")
                    .with_engine_id(EngineId::from_uuid(seed_id("eng-1")))
                    .with_layer(
                        LayerTemplate::new("Layer 1")
                            .with_layer_id(LayerId::from_uuid(seed_id("lay-1")))
                            .with_module(
                                ModuleTemplate::new("Drive", ModuleType::Drive).with_block(
                                    BlockTemplate::assigned(
                                        "OD",
                                        BlockType::Drive,
                                        PresetId::from_uuid(seed_id("p1")),
                                    ),
                                ),
                            ),
                    ),
            );

        assert_eq!(template.rig_type_id.as_ref().unwrap().as_str(), "guitar");
        assert!(template.metadata.tags.contains("guitar"));
        // Only rig_id is unassigned
        let missing = template.missing_assignments();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].level, AssignmentLevel::Rig);
    }

    // -- ProfileTemplate tests

    #[test]
    fn test_profile_template() {
        let template = ProfileTemplate::new("Worship")
            .with_profile_id(ProfileId::from_uuid(seed_id("prof-1")))
            .with_patch_name("Clean")
            .with_patch_name("Lead")
            .with_patch_name("Ambient");

        assert_eq!(template.patch_names.len(), 3);
        assert!(template.missing_assignments().is_empty());
    }

    // -- SongTemplate tests

    #[test]
    fn test_song_template() {
        let template = SongTemplate::new("Amazing Grace")
            .with_artist("Traditional")
            .with_section("Intro")
            .with_section("Verse")
            .with_section("Chorus")
            .with_metadata(
                TemplateMetadata::new()
                    .with_tag("hymn")
                    .with_description("Classic hymn arrangement"),
            );

        assert_eq!(template.section_names.len(), 3);
        assert_eq!(template.artist.as_deref(), Some("Traditional"));
        assert!(template.metadata.tags.contains("hymn"));
        // song_id unassigned
        assert_eq!(template.missing_assignments().len(), 1);
    }

    #[test]
    fn test_song_template_fully_assigned() {
        let template = SongTemplate::new("Test Song")
            .with_song_id(SongId::from_uuid(seed_id("song-1")))
            .with_section("Verse");

        assert!(template.missing_assignments().is_empty());
    }

    // -- InstantiateError display

    #[test]
    fn test_instantiate_error_display() {
        let err = InstantiateError::new(vec![
            MissingAssignment {
                level: AssignmentLevel::Block,
                slot: "OD (drive)".to_string(),
            },
            MissingAssignment {
                level: AssignmentLevel::Layer,
                slot: "layer_id for 'Main'".to_string(),
            },
        ]);

        let msg = format!("{err}");
        assert!(msg.contains("block: OD (drive)"));
        assert!(msg.contains("layer: layer_id for 'Main'"));
    }
}
