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
    async fn list_engines(&self, _cx: &Context) -> Vec<Engine> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.engines.as_ref() {
                return cached.clone();
            }
        }
        let result = self.engine_repo.list_engines().await.unwrap_or_default();
        {
            let mut cache = self.cache.write().await;
            cache.engines = Some(result.clone());
        }
        result
    }

    async fn load_engine(&self, _cx: &Context, id: EngineId) -> Option<Engine> {
        self.engine_repo.load_engine(&id).await.ok().flatten()
    }

    async fn save_engine(&self, _cx: &Context, engine: Engine) -> () {
        for variant in &engine.variants {
            if variant.validate_overrides().is_err() {
                return;
            }
        }
        for layer_id in &engine.layer_ids {
            let Some(layer) = self.layer_repo.load_layer(layer_id).await.ok().flatten() else {
                return;
            };
            if !engine.is_layer_type_compatible(layer.engine_type) {
                return;
            }
        }
        let _ = self.engine_repo.save_engine(&engine).await;
        self.cache.write().await.engines = None;
    }

    async fn delete_engine(&self, _cx: &Context, id: EngineId) -> () {
        let _ = self.engine_repo.delete_engine(&id).await;
        self.cache.write().await.engines = None;
    }

    async fn load_engine_variant(
        &self,
        _cx: &Context,
        engine_id: EngineId,
        variant_id: EngineSceneId,
    ) -> Option<EngineScene> {
        self.engine_repo
            .load_variant(&engine_id, &variant_id)
            .await
            .ok()
            .flatten()
    }
}
