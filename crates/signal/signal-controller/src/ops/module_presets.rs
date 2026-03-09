use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::traits::Collection;
use signal_proto::{
    BlockType, Module, ModuleBlock, ModuleBlockSource, ModulePreset, ModulePresetId,
    ModuleSnapshot, ModuleSnapshotId, ModuleType, PresetId,
};

/// Handle for module preset (collection) operations.
pub struct ModulePresetOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> ModulePresetOps<S> {
    pub async fn list(&self) -> Result<Vec<ModulePreset>, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .list_module_presets(&cx)
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_default(
        &self,
        collection_id: impl Into<ModulePresetId>,
    ) -> Result<Option<ModuleSnapshot>, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_module_preset(&cx, collection_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_variant(
        &self,
        collection_id: impl Into<ModulePresetId>,
        variant_id: impl Into<ModuleSnapshotId>,
    ) -> Result<Option<ModuleSnapshot>, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_module_preset_snapshot(&cx, collection_id.into(), variant_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save(&self, preset: ModulePreset) -> Result<ModulePreset, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .save_module_collection(&cx, preset.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(preset)
    }

    pub async fn delete(&self, id: impl Into<ModulePresetId>) -> Result<(), OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .delete_module_collection(&cx, id.into())
            .await
            .map_err(OpsError::Storage)
    }

    /// Update a specific snapshot's module content and bump its version.
    ///
    /// Mirrors [`BlockPresetOps::update_snapshot_params`] for the module layer.
    pub async fn update_snapshot_module(
        &self,
        preset_id: impl Into<ModulePresetId>,
        snapshot_id: impl Into<ModuleSnapshotId>,
        module: Module,
    ) -> Result<(), OpsError> {
        let preset_id = preset_id.into();
        let snapshot_id = snapshot_id.into();
        let presets = self.list().await?;
        if let Some(mut preset) = presets.into_iter().find(|p| *p.id() == preset_id) {
            if let Some(snap) = preset
                .variants_mut()
                .iter_mut()
                .find(|s| *s.id() == snapshot_id)
            {
                snap.set_module(module);
                snap.increment_version();
            }
            self.save(preset).await?;
        }
        Ok(())
    }

    /// Count all module presets.
    pub async fn count(&self) -> Result<usize, OpsError> {
        Ok(self.list().await?.len())
    }

    /// Create a new module preset from block preset references.
    ///
    /// Verifies each `PresetId` exists under its `BlockType`, then builds
    /// the full `ModulePreset` hierarchy and persists it.
    pub async fn create(
        &self,
        name: impl Into<String>,
        module_type: ModuleType,
        blocks: Vec<(BlockType, PresetId, String)>,
    ) -> Result<ModulePreset, OpsError> {
        let name = name.into();
        let mut module_blocks = Vec::new();

        for (i, (bt, preset_id, label)) in blocks.into_iter().enumerate() {
            // Verify the block preset exists
            let presets = self.0.block_presets().list(bt).await?;
            if !presets.iter().any(|p| *p.id() == preset_id) {
                return Err(OpsError::NotFound {
                    entity_type: "BlockPreset",
                    id: preset_id.to_string(),
                });
            }

            let block_id = format!("{}_{}", bt.as_str(), i);
            let source = ModuleBlockSource::PresetDefault {
                preset_id,
                saved_at_version: None,
            };
            module_blocks.push(ModuleBlock::new(block_id, label, bt, source));
        }

        let module = Module::from_blocks(module_blocks);
        let snapshot = ModuleSnapshot::new(ModuleSnapshotId::new(), &name, module);
        let preset = ModulePreset::new(
            ModulePresetId::new(),
            &name,
            module_type,
            snapshot,
            vec![],
        );

        self.save(preset).await
    }

    /// Add a snapshot (variation) to an existing module preset.
    pub async fn add_snapshot(
        &self,
        preset_id: impl Into<ModulePresetId>,
        snapshot: ModuleSnapshot,
    ) -> Result<ModulePreset, OpsError> {
        let preset_id = preset_id.into();
        let mut preset = self
            .list()
            .await?
            .into_iter()
            .find(|p| *p.id() == preset_id)
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "ModulePreset",
                id: preset_id.to_string(),
            })?;
        preset.add_snapshot(snapshot);
        self.save(preset).await
    }
}
