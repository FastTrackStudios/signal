//! Layer service implementation — CRUD for layers and layer snapshots.
//!
//! Implements [`LayerService`] on [`SignalLive`], with an in-memory cache
//! for fast repeated reads.

use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> LayerService for SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
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
    async fn list_layers(&self) -> Result<Vec<Layer>, SignalServiceError> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.layers.as_ref() {
                return Ok(cached.clone());
            }
        }
        let result = self
            .layer_repo
            .list_layers()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        {
            let mut cache = self.cache.write().await;
            cache.layers = Some(result.clone());
        }
        Ok(result)
    }

    async fn load_layer(&self, id: LayerId) -> Result<Option<Layer>, SignalServiceError> {
        self.layer_repo
            .load_layer(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn save_layer(&self, layer: Layer) -> Result<(), SignalServiceError> {
        for variant in &layer.variants {
            variant
                .validate_overrides()
                .map_err(|e| SignalServiceError::ValidationError(format!("{e:?}")))?;
        }
        self.layer_repo
            .save_layer(&layer)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.layers = None;
        Ok(())
    }

    async fn delete_layer(&self, id: LayerId) -> Result<(), SignalServiceError> {
        self.layer_repo
            .delete_layer(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.layers = None;
        Ok(())
    }

    async fn load_layer_variant(
        &self,
        layer_id: LayerId,
        variant_id: LayerSnapshotId,
    ) -> Result<Option<LayerSnapshot>, SignalServiceError> {
        self.layer_repo
            .load_variant(&layer_id, &variant_id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }
}
