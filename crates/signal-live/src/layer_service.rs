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
    async fn list_layers(&self, _cx: &Context) -> Vec<Layer> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.layers.as_ref() {
                return cached.clone();
            }
        }
        let result = self.layer_repo.list_layers().await.unwrap_or_default();
        {
            let mut cache = self.cache.write().await;
            cache.layers = Some(result.clone());
        }
        result
    }

    async fn load_layer(&self, _cx: &Context, id: LayerId) -> Option<Layer> {
        self.layer_repo.load_layer(&id).await.ok().flatten()
    }

    async fn save_layer(&self, _cx: &Context, layer: Layer) -> () {
        for variant in &layer.variants {
            if variant.validate_overrides().is_err() {
                return;
            }
        }
        let _ = self.layer_repo.save_layer(&layer).await;
        self.cache.write().await.layers = None;
    }

    async fn delete_layer(&self, _cx: &Context, id: LayerId) -> () {
        let _ = self.layer_repo.delete_layer(&id).await;
        self.cache.write().await.layers = None;
    }

    async fn load_layer_variant(
        &self,
        _cx: &Context,
        layer_id: LayerId,
        variant_id: LayerSnapshotId,
    ) -> Option<LayerSnapshot> {
        self.layer_repo
            .load_variant(&layer_id, &variant_id)
            .await
            .ok()
            .flatten()
    }
}
