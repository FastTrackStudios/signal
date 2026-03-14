//! Rig service implementation — CRUD for rigs and rig scenes.
//!
//! Implements [`RigService`] on [`SignalLive`], with an in-memory cache
//! for fast repeated reads.

use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> RigService for SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
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
    async fn list_rigs(&self) -> Result<Vec<Rig>, SignalServiceError> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.rigs.as_ref() {
                return Ok(cached.clone());
            }
        }
        let result = self.rig_repo.list_rigs().await.map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        {
            let mut cache = self.cache.write().await;
            cache.rigs = Some(result.clone());
        }
        Ok(result)
    }

    async fn load_rig(&self, id: RigId) -> Result<Option<Rig>, SignalServiceError> {
        self.rig_repo.load_rig(&id).await.map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn save_rig(&self, rig: Rig) -> Result<(), SignalServiceError> {
        for variant in &rig.variants {
            variant.validate_overrides().map_err(|e| SignalServiceError::ValidationError(format!("{e:?}")))?;
        }
        self.rig_repo
            .save_rig(&rig)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.rigs = None;
        Ok(())
    }

    async fn delete_rig(&self, id: RigId) -> Result<(), SignalServiceError> {
        self.rig_repo
            .delete_rig(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.rigs = None;
        Ok(())
    }

    async fn load_rig_variant(
        &self,
        rig_id: RigId,
        variant_id: RigSceneId,
    ) -> Result<Option<RigScene>, SignalServiceError> {
        self.rig_repo
            .load_variant(&rig_id, &variant_id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }
}
