use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> BlockService for SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
where
    B: BlockRepo,
    M: ModuleRepo,
    L: LayerRepo,
    E: EngineRepo,
    R: RigRepo,
    P: ProfileRepo,
    So: SongRepo,
    Se: SetlistRepo,
    St: SceneTemplateRepo,
    Ra: RackRepo,
{
    /// Load the current active block state for a given block type.
    /// Returns `Block::default()` when no state has been persisted yet.
    async fn get_block(&self, _cx: &Context, block_type: BlockType) -> Result<Block, String> {
        Ok(self
            .block_repo
            .load_block_state(block_type)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_default())
    }

    /// Persist a new block state and return it.
    async fn set_block(
        &self,
        _cx: &Context,
        block_type: BlockType,
        block: Block,
    ) -> Result<Block, String> {
        self.block_repo
            .save_block_state(block_type, block.clone())
            .await
            .map_err(|e| e.to_string())?;
        Ok(block)
    }

    /// List all block collections (presets) for a given block type.
    async fn list_block_presets(
        &self,
        _cx: &Context,
        block_type: BlockType,
    ) -> Result<Vec<Preset>, String> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.block_collections.get(&block_type) {
                return Ok(cached.clone());
            }
        }
        let result = self
            .block_repo
            .list_block_collections(block_type)
            .await
            .map_err(|e| e.to_string())?;
        {
            let mut cache = self.cache.write().await;
            cache.block_collections.insert(block_type, result.clone());
        }
        Ok(result)
    }

    /// Load the default variant of a block collection and apply it as the
    /// current active block.
    async fn load_block_preset(
        &self,
        _cx: &Context,
        block_type: BlockType,
        preset_id: PresetId,
    ) -> Result<Option<Snapshot>, String> {
        let snapshot = self
            .block_repo
            .load_block_default_variant(block_type, &preset_id)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(snapshot) = snapshot.as_ref() {
            self.block_repo
                .save_block_state(block_type, snapshot.block())
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(snapshot)
    }

    /// Load a specific variant from a block collection and apply it as the
    /// current active block.
    async fn load_block_preset_snapshot(
        &self,
        _cx: &Context,
        block_type: BlockType,
        preset_id: PresetId,
        snapshot_id: SnapshotId,
    ) -> Result<Option<Snapshot>, String> {
        let snapshot = self
            .block_repo
            .load_block_variant(block_type, &preset_id, &snapshot_id)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(snapshot) = snapshot.as_ref() {
            self.block_repo
                .save_block_state(block_type, snapshot.block())
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(snapshot)
    }

    /// List all module collections.
    async fn list_module_presets(&self, _cx: &Context) -> Result<Vec<ModulePreset>, String> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.module_collections.as_ref() {
                return Ok(cached.clone());
            }
        }
        let result = self
            .module_repo
            .list_module_collections()
            .await
            .map_err(|e| e.to_string())?;
        {
            let mut cache = self.cache.write().await;
            cache.module_collections = Some(result.clone());
        }
        Ok(result)
    }

    /// Load the default variant of a module collection.
    async fn load_module_preset(
        &self,
        _cx: &Context,
        preset_id: ModulePresetId,
    ) -> Result<Option<ModuleSnapshot>, String> {
        self.module_repo
            .load_module_default_variant(&preset_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Load a specific variant from a module collection.
    async fn load_module_preset_snapshot(
        &self,
        _cx: &Context,
        preset_id: ModulePresetId,
        snapshot_id: ModuleSnapshotId,
    ) -> Result<Option<ModuleSnapshot>, String> {
        self.module_repo
            .load_module_variant(&preset_id, &snapshot_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Save (upsert) a block collection and invalidate the cache for its block type.
    async fn save_block_preset(&self, _cx: &Context, preset: Preset) -> Result<(), String> {
        let bt = preset.block_type();
        self.block_repo
            .save_block_collection(preset)
            .await
            .map_err(|e| e.to_string())?;
        let mut cache = self.cache.write().await;
        cache.block_collections.remove(&bt);
        Ok(())
    }

    /// Delete a block collection (preset) by ID and invalidate the cache for its block type.
    async fn delete_block_preset(
        &self,
        _cx: &Context,
        block_type: BlockType,
        preset_id: PresetId,
    ) -> Result<(), String> {
        self.block_repo
            .delete_block_collection(&preset_id)
            .await
            .map_err(|e| e.to_string())?;
        let mut cache = self.cache.write().await;
        cache.block_collections.remove(&block_type);
        Ok(())
    }

    /// Save (upsert) a module collection and invalidate the module cache.
    async fn save_module_collection(
        &self,
        _cx: &Context,
        preset: ModulePreset,
    ) -> Result<(), String> {
        self.module_repo
            .save_module_collection(preset)
            .await
            .map_err(|e| e.to_string())?;
        let mut cache = self.cache.write().await;
        cache.module_collections = None;
        Ok(())
    }

    /// Delete a module collection and invalidate the module cache.
    async fn delete_module_collection(
        &self,
        _cx: &Context,
        id: ModulePresetId,
    ) -> Result<(), String> {
        self.module_repo
            .delete_module_collection(&id)
            .await
            .map_err(|e| e.to_string())?;
        let mut cache = self.cache.write().await;
        cache.module_collections = None;
        Ok(())
    }
}
