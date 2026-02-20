use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::traits::Collection;
use signal_proto::{Block, BlockType, Preset, PresetId, Snapshot, SnapshotId};

/// Handle for block preset (collection) operations.
pub struct BlockPresetOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> BlockPresetOps<S> {
    pub async fn list(&self, block_type: BlockType) -> Result<Vec<Preset>, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .list_block_presets(&cx, block_type)
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_default(
        &self,
        block_type: BlockType,
        collection_id: impl Into<PresetId>,
    ) -> Result<Option<Block>, OpsError> {
        let cx = self.0.context_factory.make_context();
        let snapshot = self
            .0
            .service
            .load_block_preset(&cx, block_type, collection_id.into())
            .await
            .map_err(OpsError::Storage)?;
        Ok(snapshot.map(|s| s.block()))
    }

    pub async fn load_variant(
        &self,
        block_type: BlockType,
        collection_id: impl Into<PresetId>,
        variant_id: impl Into<SnapshotId>,
    ) -> Result<Option<Block>, OpsError> {
        let cx = self.0.context_factory.make_context();
        let snapshot = self
            .0
            .service
            .load_block_preset_snapshot(&cx, block_type, collection_id.into(), variant_id.into())
            .await
            .map_err(OpsError::Storage)?;
        Ok(snapshot.map(|s| s.block()))
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        block_type: BlockType,
        default_block: Block,
    ) -> Result<Preset, OpsError> {
        let preset = Preset::with_default_snapshot(
            PresetId::new(),
            name,
            block_type,
            Snapshot::new(SnapshotId::new(), "Default", default_block),
        );
        self.save(preset.clone()).await?;
        Ok(preset)
    }

    pub async fn save(&self, preset: Preset) -> Result<Preset, OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .save_block_preset(&cx, preset.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(preset)
    }

    pub async fn delete(
        &self,
        block_type: BlockType,
        preset_id: impl Into<PresetId>,
    ) -> Result<(), OpsError> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .delete_block_preset(&cx, block_type, preset_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn update_snapshot_params(
        &self,
        block_type: BlockType,
        preset_id: impl Into<PresetId>,
        snapshot_id: impl Into<SnapshotId>,
        block: Block,
    ) -> Result<(), OpsError> {
        let preset_id = preset_id.into();
        let snapshot_id = snapshot_id.into();
        let presets = self.list(block_type).await?;
        if let Some(mut preset) = presets.into_iter().find(|p| *p.id() == preset_id) {
            if let Some(snap) = preset
                .variants_mut()
                .iter_mut()
                .find(|s| *s.id() == snapshot_id)
            {
                snap.set_block(block);
                snap.increment_version();
            }
            self.save(preset).await?;
        }
        Ok(())
    }

    /// Count all block presets of a given type.
    pub async fn count(&self, block_type: BlockType) -> Result<usize, OpsError> {
        Ok(self.list(block_type).await?.len())
    }
}
