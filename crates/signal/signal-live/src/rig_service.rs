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
    async fn list_rigs(&self, _cx: &Context) -> Vec<Rig> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.rigs.as_ref() {
                return cached.clone();
            }
        }
        let result = self.rig_repo.list_rigs().await.unwrap_or_default();
        {
            let mut cache = self.cache.write().await;
            cache.rigs = Some(result.clone());
        }
        result
    }

    async fn load_rig(&self, _cx: &Context, id: RigId) -> Option<Rig> {
        self.rig_repo.load_rig(&id).await.ok().flatten()
    }

    async fn save_rig(&self, _cx: &Context, rig: Rig) -> () {
        for variant in &rig.variants {
            if variant.validate_overrides().is_err() {
                return;
            }
        }
        let _ = self.rig_repo.save_rig(&rig).await;
        self.cache.write().await.rigs = None;
    }

    async fn delete_rig(&self, _cx: &Context, id: RigId) -> () {
        let _ = self.rig_repo.delete_rig(&id).await;
        self.cache.write().await.rigs = None;
    }

    async fn load_rig_variant(
        &self,
        _cx: &Context,
        rig_id: RigId,
        variant_id: RigSceneId,
    ) -> Option<RigScene> {
        self.rig_repo
            .load_variant(&rig_id, &variant_id)
            .await
            .ok()
            .flatten()
    }
}
