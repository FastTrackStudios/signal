use super::error::OpsError;
use crate::{events, SignalApi, SignalController};
use signal_proto::engine::{EngineId, EngineSceneId};
use signal_proto::layer::{LayerId, LayerSnapshotId};
use signal_proto::resolve::{
    LayerSource, ResolveTarget, ResolvedBlock, ResolvedEngine, ResolvedGraph, ResolvedLayer,
    ResolvedModule,
};
use signal_proto::rig::{RigId, RigSceneId};
use signal_proto::traits::Collection;
use signal_proto::{
    Block, BlockType, ModulePresetId, ModuleSnapshotId, Preset, PresetId, Snapshot, SnapshotId,
};

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

    /// Load a block preset snapshot and apply it directly to the DAW.
    ///
    /// Builds a minimal single-block `ResolvedGraph` and pushes it through the
    /// DAW applier (if attached). This is the "Preset mode" path — no profile,
    /// no rig hierarchy, just a single FX chain swap.
    pub async fn activate(
        &self,
        block_type: BlockType,
        preset_id: impl Into<PresetId>,
        snapshot_id: impl Into<SnapshotId>,
    ) -> Result<ResolvedGraph, OpsError> {
        let preset_id = preset_id.into();
        let snapshot_id = snapshot_id.into();

        // Load the specific snapshot
        let cx = self.0.context_factory.make_context();
        let snapshot = self
            .0
            .service
            .load_block_preset_snapshot(&cx, block_type, preset_id.clone(), snapshot_id.clone())
            .await
            .map_err(OpsError::Storage)?
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "BlockPreset",
                parent_id: preset_id.to_string(),
                variant_id: snapshot_id.to_string(),
            })?;

        // Build a minimal single-block resolved graph (same pattern as
        // try_resolve_block_snapshot in signal-live's resolve_service).
        let resolved_block = ResolvedBlock {
            node_id: "plugin".to_string(),
            label: snapshot.name().to_string(),
            block_type,
            source_preset_id: Some(preset_id.clone()),
            source_variant_id: Some(snapshot_id.clone()),
            state_data: snapshot.state_data().map(|d| d.to_vec()),
            block: snapshot.block(),
            stale: false,
        };

        let graph = ResolvedGraph {
            target: ResolveTarget::RigScene {
                rig_id: RigId::from(signal_proto::seed_id("block-snapshot-rig")),
                scene_id: RigSceneId::from(signal_proto::seed_id("block-snapshot-scene")),
            },
            rig_id: RigId::from(signal_proto::seed_id("block-snapshot-rig")),
            rig_scene_id: RigSceneId::from(signal_proto::seed_id("block-snapshot-scene")),
            engines: vec![ResolvedEngine {
                engine_id: EngineId::from(signal_proto::seed_id("block-snapshot-engine")),
                engine_scene_id: EngineSceneId::from(signal_proto::seed_id(
                    "block-snapshot-engine-scene",
                )),
                layers: vec![ResolvedLayer {
                    layer_id: LayerId::from(signal_proto::seed_id("block-snapshot-layer")),
                    layer_variant_id: LayerSnapshotId::from(signal_proto::seed_id(
                        "block-snapshot-layer-variant",
                    )),
                    source: LayerSource::InlinedInParent,
                    modules: vec![ResolvedModule {
                        source_preset_id: ModulePresetId::from(signal_proto::seed_id(
                            "block-snapshot-module",
                        )),
                        source_variant_id: ModuleSnapshotId::from(signal_proto::seed_id(
                            "block-snapshot-module-variant",
                        )),
                        blocks: vec![resolved_block],
                    }],
                    standalone_blocks: vec![],
                }],
            }],
            effective_overrides: vec![],
        };

        // Apply to DAW if an applier is attached
        let snapshot_name = snapshot.name().to_string();
        let applied_to_daw =
            if let Some(applier) = self.0.daw_applier.read().expect("lock poisoned").clone() {
                match applier.apply_graph(&graph, Some(&snapshot_name)).await {
                    Ok(_) => true,
                    Err(e) => {
                        eprintln!("[signal] block preset activate DAW apply failed: {e}");
                        false
                    }
                }
            } else {
                false
            };

        // Emit event
        self.0.event_bus.emit(events::SignalEvent::PresetActivated {
            block_type,
            preset_id: preset_id.to_string(),
            snapshot_id: snapshot_id.to_string(),
            applied_to_daw,
        });

        Ok(graph)
    }
}
