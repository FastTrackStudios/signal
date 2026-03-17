//! Engine service implementation — CRUD for engines and engine scenes.
//!
//! Implements [`EngineService`] on [`SignalLive`], with an in-memory cache
//! for fast repeated reads.

use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> EngineService
    for SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
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
    async fn list_engines(&self) -> Result<Vec<Engine>, SignalServiceError> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.engines.as_ref() {
                return Ok(cached.clone());
            }
        }
        let result = self
            .engine_repo
            .list_engines()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        {
            let mut cache = self.cache.write().await;
            cache.engines = Some(result.clone());
        }
        Ok(result)
    }

    async fn load_engine(&self, id: EngineId) -> Result<Option<Engine>, SignalServiceError> {
        self.engine_repo
            .load_engine(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn save_engine(&self, engine: Engine) -> Result<(), SignalServiceError> {
        for variant in &engine.variants {
            variant
                .validate_overrides()
                .map_err(|e| SignalServiceError::ValidationError(format!("{e:?}")))?;
        }
        for layer_id in &engine.layer_ids {
            let layer = self
                .layer_repo
                .load_layer(layer_id)
                .await
                .map_err(|e| SignalServiceError::StorageError(e.to_string()))?
                .ok_or_else(|| SignalServiceError::not_found("Layer", &layer_id))?;
            if !engine.is_layer_type_compatible(layer.engine_type) {
                return Err(SignalServiceError::ValidationError(format!(
                    "layer {} engine type {:?} incompatible with engine {:?}",
                    layer_id, layer.engine_type, engine.engine_type
                )));
            }
        }
        self.engine_repo
            .save_engine(&engine)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.engines = None;
        Ok(())
    }

    async fn delete_engine(&self, id: EngineId) -> Result<(), SignalServiceError> {
        self.engine_repo
            .delete_engine(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))?;
        self.cache.write().await.engines = None;
        Ok(())
    }

    async fn load_engine_variant(
        &self,
        engine_id: EngineId,
        variant_id: EngineSceneId,
    ) -> Result<Option<EngineScene>, SignalServiceError> {
        self.engine_repo
            .load_variant(&engine_id, &variant_id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }
}
