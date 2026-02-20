use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::traits::Collection;
use signal_proto::{Module, ModulePreset, ModulePresetId, ModuleSnapshot, ModuleSnapshotId};

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
}
