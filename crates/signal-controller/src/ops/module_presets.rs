use crate::{SignalApi, SignalController};
use signal_proto::{ModulePreset, ModulePresetId, ModuleSnapshot, ModuleSnapshotId};

/// Handle for module preset (collection) operations.
pub struct ModulePresetOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> ModulePresetOps<S> {
    pub async fn list(&self) -> Vec<ModulePreset> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_module_presets(&cx).await
    }

    pub async fn load_default(
        &self,
        collection_id: impl Into<ModulePresetId>,
    ) -> Option<ModuleSnapshot> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_module_preset(&cx, collection_id.into())
            .await
    }

    pub async fn load_variant(
        &self,
        collection_id: impl Into<ModulePresetId>,
        variant_id: impl Into<ModuleSnapshotId>,
    ) -> Option<ModuleSnapshot> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_module_preset_snapshot(&cx, collection_id.into(), variant_id.into())
            .await
    }

    pub async fn save(&self, preset: ModulePreset) -> ModulePreset {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .save_module_collection(&cx, preset.clone())
            .await;
        preset
    }

    pub async fn delete(&self, id: impl Into<ModulePresetId>) {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .delete_module_collection(&cx, id.into())
            .await;
    }
}
