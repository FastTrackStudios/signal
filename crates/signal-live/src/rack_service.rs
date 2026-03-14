//! Rack service implementation — CRUD for racks and rack slots.
//!
//! Implements [`RackService`] on [`SignalLive`], with an in-memory cache
//! for fast repeated reads.

use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> RackService for SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
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
    async fn list_racks(&self) -> Result<Vec<Rack>, SignalServiceError> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.racks.as_ref() {
                return Ok(cached.clone());
            }
        }
        let result = self
            .rack_repo
            .list_racks()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.racks = Some(result.clone());
        Ok(result)
    }

    async fn load_rack(&self, id: RackId) -> Result<Option<Rack>, SignalServiceError> {
        self.rack_repo
            .load_rack(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn save_rack(&self, rack: Rack) -> Result<(), SignalServiceError> {
        self.rack_repo
            .save_rack(&rack)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.racks = None;
        Ok(())
    }

    async fn delete_rack(&self, id: RackId) -> Result<(), SignalServiceError> {
        self.rack_repo
            .delete_rack(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.racks = None;
        Ok(())
    }
}
