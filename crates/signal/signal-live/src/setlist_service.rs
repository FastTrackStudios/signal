use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> SetlistService
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
    async fn list_setlists(&self, _cx: &Context) -> Vec<Setlist> {
        self.setlist_repo.list_setlists().await.unwrap_or_default()
    }

    async fn load_setlist(&self, _cx: &Context, id: SetlistId) -> Option<Setlist> {
        self.setlist_repo.load_setlist(&id).await.ok().flatten()
    }

    async fn save_setlist(&self, _cx: &Context, setlist: Setlist) -> () {
        let _ = self.setlist_repo.save_setlist(&setlist).await;
    }

    async fn delete_setlist(&self, _cx: &Context, id: SetlistId) -> () {
        let _ = self.setlist_repo.delete_setlist(&id).await;
    }

    async fn load_setlist_entry(
        &self,
        _cx: &Context,
        setlist_id: SetlistId,
        entry_id: SetlistEntryId,
    ) -> Option<SetlistEntry> {
        self.setlist_repo
            .load_entry(&setlist_id, &entry_id)
            .await
            .ok()
            .flatten()
    }
}
