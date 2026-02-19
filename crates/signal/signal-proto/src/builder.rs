//! Fluent builder for constructing the full signal hierarchy.
//!
//! The standard hierarchy for a single-engine rig is:
//!
//! ```text
//! BlockParameter → Block → Snapshot → Preset
//!   → ModuleBlock → Module → ModuleSnapshot → ModulePreset
//!     → LayerSnapshot → Layer
//!       → EngineScene → Engine
//!         → RigScene → Rig
//!           → Patch → Profile
//! ```
//!
//! Building this by hand requires ~125 lines and 16+ manually-created IDs.
//! [`RigBuilder`] reduces this to ~20 lines by auto-generating all
//! intermediate IDs and wiring the layers together.
//!
//! # Example
//!
//! ```ignore
//! let built = RigBuilder::new("My Guitar Rig")
//!     .block_preset("JM Amp", BlockType::Amp, |bp| {
//!         bp.param("gain", "Gain", 0.45)
//!           .param("bass", "Bass", 0.5)
//!           .snapshot("Lead", |sp| sp.param("gain", "Gain", 0.8))
//!     })
//!     .scene("Clean")
//!     .scene("Lead")
//!     .build();
//!
//! // Save all entities via the controller:
//! built.save(&ctrl).await;
//! ```

use crate::block::BlockType;
use crate::engine::{Engine, EngineId, EngineScene, EngineSceneId, LayerSelection};
use crate::layer::{Layer, LayerId, LayerSnapshot, LayerSnapshotId, ModuleRef};
use crate::module_type::ModuleType;
use crate::profile::{Patch, PatchId, Profile, ProfileId};
use crate::rig::{EngineSelection, Rig, RigId, RigScene, RigSceneId, RigType};
use crate::{
    Block, BlockParameter, EngineType, Module, ModuleBlock, ModuleBlockSource, ModulePreset,
    ModulePresetId, ModuleSnapshot, ModuleSnapshotId, Preset, PresetId, Snapshot, SnapshotId,
};

// ─── Block preset builder ───────────────────────────────────────

/// Builder for a block preset with default + additional snapshots.
pub struct BlockPresetBuilder {
    name: String,
    block_type: BlockType,
    params: Vec<BlockParameter>,
    snapshots: Vec<(String, Vec<BlockParameter>)>,
}

impl BlockPresetBuilder {
    fn new(name: impl Into<String>, block_type: BlockType) -> Self {
        Self {
            name: name.into(),
            block_type,
            params: Vec::new(),
            snapshots: Vec::new(),
        }
    }

    /// Add a parameter to the default snapshot.
    #[must_use]
    pub fn param(mut self, id: &str, name: &str, value: f32) -> Self {
        self.params.push(BlockParameter::new(id, name, value));
        self
    }

    /// Add an additional snapshot (variant) with its own parameter values.
    /// The closure receives a fresh [`SnapshotParamBuilder`].
    #[must_use]
    pub fn snapshot(
        mut self,
        name: &str,
        f: impl FnOnce(SnapshotParamBuilder) -> SnapshotParamBuilder,
    ) -> Self {
        let builder = f(SnapshotParamBuilder::new());
        self.snapshots.push((name.to_string(), builder.params));
        self
    }
}

/// Builder for snapshot parameter values.
pub struct SnapshotParamBuilder {
    params: Vec<BlockParameter>,
}

impl SnapshotParamBuilder {
    fn new() -> Self {
        Self { params: Vec::new() }
    }

    #[must_use]
    pub fn param(mut self, id: &str, name: &str, value: f32) -> Self {
        self.params.push(BlockParameter::new(id, name, value));
        self
    }
}

// ─── Built output ───────────────────────────────────────────────

/// A fully-constructed block preset with all generated IDs.
pub struct BuiltBlockPreset {
    pub preset: Preset,
    pub preset_id: PresetId,
    pub default_snapshot_id: SnapshotId,
    /// Maps snapshot name → SnapshotId (including the default).
    pub snapshot_ids: Vec<(String, SnapshotId)>,
}

/// All entities produced by [`RigBuilder::build`].
///
/// Call [`BuiltRig::entities`] to get all saveable domain objects.
pub struct BuiltRig {
    pub block_presets: Vec<BuiltBlockPreset>,
    pub module_preset: ModulePreset,
    pub module_preset_id: ModulePresetId,
    pub layer: Layer,
    pub layer_id: LayerId,
    pub engine: Engine,
    pub engine_id: EngineId,
    pub rig: Rig,
    pub rig_id: RigId,
    pub profile: Option<Profile>,
    pub profile_id: Option<ProfileId>,
    /// Maps scene name → RigSceneId.
    pub scene_ids: Vec<(String, RigSceneId)>,
    /// Maps patch name → PatchId (only populated if profile was built).
    pub patch_ids: Vec<(String, PatchId)>,
}

impl BuiltRig {
    /// Look up a scene ID by name.
    pub fn scene_id(&self, name: &str) -> Option<&RigSceneId> {
        self.scene_ids
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, id)| id)
    }

    /// Look up a patch ID by name.
    pub fn patch_id(&self, name: &str) -> Option<&PatchId> {
        self.patch_ids
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, id)| id)
    }

    /// Look up a block preset's snapshot ID by preset name and snapshot name.
    pub fn snapshot_id(&self, preset_name: &str, snapshot_name: &str) -> Option<&SnapshotId> {
        self.block_presets
            .iter()
            .find(|bp| bp.preset.name() == preset_name)
            .and_then(|bp| {
                bp.snapshot_ids
                    .iter()
                    .find(|(n, _)| n == snapshot_name)
                    .map(|(_, id)| id)
            })
    }
}

// ─── RigBuilder ─────────────────────────────────────────────────

/// Reference to an already-persisted block preset to wire into the module chain.
struct ExistingPresetRef {
    preset_id: PresetId,
    name: String,
    block_type: BlockType,
}

/// Fluent builder for the full Block→Module→Layer→Engine→Rig→Profile hierarchy.
pub struct RigBuilder {
    name: String,
    rig_type: RigType,
    engine_type: EngineType,
    block_presets: Vec<BlockPresetBuilder>,
    existing_presets: Vec<ExistingPresetRef>,
    scenes: Vec<String>,
    with_profile: bool,
    profile_name: Option<String>,
}

impl RigBuilder {
    /// Start building a rig with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rig_type: RigType::Guitar,
            engine_type: EngineType::Guitar,
            block_presets: Vec::new(),
            existing_presets: Vec::new(),
            scenes: Vec::new(),
            with_profile: false,
            profile_name: None,
        }
    }

    /// Set the rig type (default: Guitar).
    #[must_use]
    pub fn rig_type(mut self, rig_type: RigType) -> Self {
        self.rig_type = rig_type;
        self
    }

    /// Set the engine type (default: Guitar).
    #[must_use]
    pub fn engine_type(mut self, engine_type: EngineType) -> Self {
        self.engine_type = engine_type;
        self
    }

    /// Add a block preset (e.g., an amp, drive, EQ) to the module chain.
    /// The closure receives a [`BlockPresetBuilder`] for configuring parameters and snapshots.
    #[must_use]
    pub fn block_preset(
        mut self,
        name: &str,
        block_type: BlockType,
        f: impl FnOnce(BlockPresetBuilder) -> BlockPresetBuilder,
    ) -> Self {
        let builder = f(BlockPresetBuilder::new(name, block_type));
        self.block_presets.push(builder);
        self
    }

    /// Reference an already-saved block preset by ID instead of building a new one.
    ///
    /// The preset won't appear in `built.block_presets` (it's already persisted),
    /// but a `ModuleBlock` referencing it will be wired into the module chain.
    #[must_use]
    pub fn existing_block_preset(
        mut self,
        preset_id: impl Into<PresetId>,
        name: &str,
        block_type: BlockType,
    ) -> Self {
        self.existing_presets.push(ExistingPresetRef {
            preset_id: preset_id.into(),
            name: name.to_string(),
            block_type,
        });
        self
    }

    /// Add a named scene to the rig. The first scene is the default.
    /// Each scene also becomes a patch when building a profile.
    #[must_use]
    pub fn scene(mut self, name: &str) -> Self {
        self.scenes.push(name.to_string());
        self
    }

    /// Also build a Profile with patches for each scene.
    #[must_use]
    pub fn with_profile(mut self) -> Self {
        self.with_profile = true;
        self
    }

    /// Also build a Profile with a custom name.
    #[must_use]
    pub fn with_named_profile(mut self, name: &str) -> Self {
        self.with_profile = true;
        self.profile_name = Some(name.to_string());
        self
    }

    /// Build all domain entities. No I/O — returns pure domain objects.
    pub fn build(self) -> BuiltRig {
        // Ensure at least one scene
        let scenes = if self.scenes.is_empty() {
            vec!["Default".to_string()]
        } else {
            self.scenes
        };

        // ── Step 1: Build block presets ──
        let mut built_block_presets = Vec::new();
        let mut module_blocks = Vec::new();

        for bp_builder in &self.block_presets {
            let preset_id = PresetId::new();
            let default_snap_id = SnapshotId::new();
            let mut snapshot_ids = Vec::new();

            let default_block = Block::from_parameters(bp_builder.params.clone());
            let default_snap = Snapshot::new(default_snap_id.clone(), "Default", default_block);
            snapshot_ids.push(("Default".to_string(), default_snap_id.clone()));

            let mut additional_snaps = Vec::new();
            for (snap_name, snap_params) in &bp_builder.snapshots {
                let snap_id = SnapshotId::new();
                let block = Block::from_parameters(snap_params.clone());
                additional_snaps.push(Snapshot::new(snap_id.clone(), snap_name.as_str(), block));
                snapshot_ids.push((snap_name.clone(), snap_id));
            }

            let preset = Preset::new(
                preset_id.clone(),
                &bp_builder.name,
                bp_builder.block_type,
                default_snap,
                additional_snaps,
            );

            // Create a module block referencing this preset
            let block_id = bp_builder.name.to_lowercase().replace(' ', "-");
            module_blocks.push(ModuleBlock::new(
                &block_id,
                &bp_builder.name,
                bp_builder.block_type,
                ModuleBlockSource::PresetDefault {
                    preset_id: preset_id.clone(),
                    saved_at_version: None,
                },
            ));

            built_block_presets.push(BuiltBlockPreset {
                preset,
                preset_id,
                default_snapshot_id: default_snap_id,
                snapshot_ids,
            });
        }

        // ── Step 1b: Wire existing (already-saved) block presets ──
        for existing in &self.existing_presets {
            let block_id = existing.name.to_lowercase().replace(' ', "-");
            module_blocks.push(ModuleBlock::new(
                &block_id,
                &existing.name,
                existing.block_type,
                ModuleBlockSource::PresetDefault {
                    preset_id: existing.preset_id.clone(),
                    saved_at_version: None,
                },
            ));
        }

        // ── Step 2: Build module preset ──
        let module_preset_id = ModulePresetId::new();
        let module_snap_id = ModuleSnapshotId::new();
        let module = Module::from_blocks(module_blocks);
        let module_snap = ModuleSnapshot::new(module_snap_id.clone(), "Default", module);
        let module_type = match self.engine_type {
            EngineType::Guitar | EngineType::Bass => ModuleType::Amp,
            EngineType::Keys => ModuleType::Custom,
            _ => ModuleType::Custom,
        };
        let module_preset = ModulePreset::new(
            module_preset_id.clone(),
            format!("{} Module", self.name),
            module_type,
            module_snap,
            vec![],
        );

        // ── Step 3: Build layer ──
        let layer_id = LayerId::new();
        let layer_snap_id = LayerSnapshotId::new();
        let layer_snap = LayerSnapshot::new(layer_snap_id.clone(), "Default")
            .with_module(ModuleRef::new(module_preset_id.clone()));
        let layer = Layer::new(
            layer_id.clone(),
            format!("{} Layer", self.name),
            self.engine_type,
            layer_snap,
        );

        // ── Step 4: Build engine with scenes ──
        let engine_id = EngineId::new();
        let mut engine_scene_ids: Vec<(String, EngineSceneId)> = Vec::new();

        let first_scene_name = &scenes[0];
        let first_engine_scene_id = EngineSceneId::new();
        engine_scene_ids.push((first_scene_name.clone(), first_engine_scene_id.clone()));

        let default_engine_scene =
            EngineScene::new(first_engine_scene_id.clone(), first_scene_name.as_str())
                .with_layer(LayerSelection::new(layer_id.clone(), layer_snap_id.clone()));

        let mut engine = Engine::new(
            engine_id.clone(),
            format!("{} Engine", self.name),
            self.engine_type,
            vec![layer_id.clone()],
            default_engine_scene,
        );

        for scene_name in scenes.iter().skip(1) {
            let es_id = EngineSceneId::new();
            engine_scene_ids.push((scene_name.clone(), es_id.clone()));
            engine.add_variant(
                EngineScene::new(es_id, scene_name.as_str())
                    .with_layer(LayerSelection::new(layer_id.clone(), layer_snap_id.clone())),
            );
        }

        // ── Step 5: Build rig with scenes ──
        let rig_id = RigId::new();
        let mut rig_scene_ids: Vec<(String, RigSceneId)> = Vec::new();

        let first_rig_scene_id = RigSceneId::new();
        rig_scene_ids.push((scenes[0].clone(), first_rig_scene_id.clone()));

        let default_rig_scene =
            RigScene::new(first_rig_scene_id.clone(), scenes[0].as_str()).with_engine(
                EngineSelection::new(engine_id.clone(), engine_scene_ids[0].1.clone()),
            );

        let mut rig = Rig::new(
            rig_id.clone(),
            &self.name,
            vec![engine_id.clone()],
            default_rig_scene,
        )
        .with_rig_type(self.rig_type);

        for (i, scene_name) in scenes.iter().enumerate().skip(1) {
            let rs_id = RigSceneId::new();
            rig_scene_ids.push((scene_name.clone(), rs_id.clone()));
            rig.add_variant(RigScene::new(rs_id, scene_name.as_str()).with_engine(
                EngineSelection::new(engine_id.clone(), engine_scene_ids[i].1.clone()),
            ));
        }

        // ── Step 6: Optionally build profile ──
        let mut profile = None;
        let mut profile_id_out = None;
        let mut patch_ids = Vec::new();

        if self.with_profile {
            let pid = ProfileId::new();
            let profile_name = self
                .profile_name
                .unwrap_or_else(|| format!("{} Profile", self.name));

            let first_patch_id = PatchId::new();
            patch_ids.push((scenes[0].clone(), first_patch_id.clone()));

            let default_patch = Patch::from_rig_scene(
                first_patch_id,
                scenes[0].as_str(),
                rig_id.clone(),
                rig_scene_ids[0].1.clone(),
            );

            let mut prof = Profile::new(pid.clone(), &profile_name, default_patch);

            for (i, scene_name) in scenes.iter().enumerate().skip(1) {
                let patch_id = PatchId::new();
                patch_ids.push((scene_name.clone(), patch_id.clone()));
                prof.add_patch(Patch::from_rig_scene(
                    patch_id,
                    scene_name.as_str(),
                    rig_id.clone(),
                    rig_scene_ids[i].1.clone(),
                ));
            }

            profile_id_out = Some(pid);
            profile = Some(prof);
        }

        BuiltRig {
            block_presets: built_block_presets,
            module_preset,
            module_preset_id,
            layer,
            layer_id,
            engine,
            engine_id,
            rig,
            rig_id,
            profile,
            profile_id: profile_id_out,
            scene_ids: rig_scene_ids,
            patch_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_produces_complete_hierarchy() {
        let built = RigBuilder::new("Test Rig")
            .block_preset("Amp", BlockType::Amp, |bp| {
                bp.param("gain", "Gain", 0.45)
                    .param("bass", "Bass", 0.5)
                    .snapshot("Lead", |sp| sp.param("gain", "Gain", 0.8))
            })
            .block_preset("Drive", BlockType::Drive, |bp| {
                bp.param("level", "Level", 0.5)
            })
            .scene("Clean")
            .scene("Lead")
            .scene("Crunch")
            .with_profile()
            .build();

        // Block presets
        assert_eq!(built.block_presets.len(), 2);
        assert_eq!(built.block_presets[0].preset.name(), "Amp");
        assert_eq!(built.block_presets[0].preset.snapshots().len(), 2); // Default + Lead
        assert_eq!(built.block_presets[1].preset.name(), "Drive");

        // Module
        assert_eq!(built.module_preset.name(), "Test Rig Module");
        assert_eq!(
            built
                .module_preset
                .default_snapshot()
                .module()
                .blocks()
                .len(),
            2
        );

        // Layer
        assert_eq!(built.layer.name, "Test Rig Layer");
        assert_eq!(built.layer.variants.len(), 1);

        // Engine
        assert_eq!(built.engine.name, "Test Rig Engine");
        assert_eq!(built.engine.variants.len(), 3);

        // Rig
        assert_eq!(built.rig.name, "Test Rig");
        assert_eq!(built.rig.variants.len(), 3);
        assert_eq!(built.rig.rig_type, Some(RigType::Guitar));

        // Scene IDs
        assert_eq!(built.scene_ids.len(), 3);
        assert!(built.scene_id("Clean").is_some());
        assert!(built.scene_id("Lead").is_some());
        assert!(built.scene_id("Crunch").is_some());

        // Profile
        let profile = built.profile.as_ref().unwrap();
        assert_eq!(profile.name, "Test Rig Profile");
        assert_eq!(profile.patches.len(), 3);
        assert_eq!(profile.default_patch().unwrap().name, "Clean");

        // Patch IDs
        assert_eq!(built.patch_ids.len(), 3);
        assert!(built.patch_id("Clean").is_some());
        assert!(built.patch_id("Lead").is_some());
    }

    #[test]
    fn builder_defaults_to_single_scene() {
        let built = RigBuilder::new("Minimal").build();

        assert_eq!(built.rig.variants.len(), 1);
        assert_eq!(built.scene_ids.len(), 1);
        assert_eq!(built.scene_ids[0].0, "Default");
        assert!(built.profile.is_none());
    }

    #[test]
    fn builder_existing_block_preset() {
        let existing_id = PresetId::new();
        let built = RigBuilder::new("Mixed Rig")
            .block_preset("New Amp", BlockType::Amp, |bp| {
                bp.param("gain", "Gain", 0.5)
            })
            .existing_block_preset(existing_id.clone(), "External Drive", BlockType::Drive)
            .scene("Clean")
            .build();

        // Only the new preset appears in built.block_presets
        assert_eq!(built.block_presets.len(), 1);
        assert_eq!(built.block_presets[0].preset.name(), "New Amp");

        // But the module should have 2 blocks (new + existing)
        let blocks = built.module_preset.default_snapshot().module().blocks();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].label, "New Amp");
        assert_eq!(blocks[1].label, "External Drive");
    }

    #[test]
    fn builder_keys_rig_type() {
        let built = RigBuilder::new("Keys Rig")
            .rig_type(RigType::Keys)
            .engine_type(EngineType::Keys)
            .scene("Warm")
            .scene("Bright")
            .build();

        assert_eq!(built.rig.rig_type, Some(RigType::Keys));
        assert_eq!(built.engine.engine_type, EngineType::Keys);
        assert_eq!(built.layer.engine_type, EngineType::Keys);
    }
}
