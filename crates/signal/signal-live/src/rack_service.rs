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
    async fn list_racks(&self, _cx: &Context) -> Vec<Rack> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.racks.as_ref() {
                return cached.clone();
            }
        }
        let result = self.rack_repo.list_racks().await.unwrap_or_default();
        self.cache.write().await.racks = Some(result.clone());
        result
    }

    async fn load_rack(&self, _cx: &Context, id: RackId) -> Option<Rack> {
        self.rack_repo.load_rack(&id).await.ok().flatten()
    }

    async fn save_rack(&self, _cx: &Context, rack: Rack) {
        let _ = self.rack_repo.save_rack(&rack).await;
        self.cache.write().await.racks = None;
    }

    async fn delete_rack(&self, _cx: &Context, id: RackId) {
        let _ = self.rack_repo.delete_rack(&id).await;
        self.cache.write().await.racks = None;
    }
}
