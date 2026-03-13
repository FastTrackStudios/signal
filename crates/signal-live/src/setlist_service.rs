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
    async fn list_setlists(&self) -> Result<Vec<Setlist>, String> {
        self.setlist_repo
            .list_setlists()
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_setlist(&self, id: SetlistId) -> Result<Option<Setlist>, String> {
        self.setlist_repo
            .load_setlist(&id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn save_setlist(&self, setlist: Setlist) -> Result<(), String> {
        self.setlist_repo
            .save_setlist(&setlist)
            .await
            .map_err(|e| e.to_string())
    }

    async fn delete_setlist(&self, id: SetlistId) -> Result<(), String> {
        self.setlist_repo
            .delete_setlist(&id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_setlist_entry(
        &self,
        setlist_id: SetlistId,
        entry_id: SetlistEntryId,
    ) -> Result<Option<SetlistEntry>, String> {
        self.setlist_repo
            .load_entry(&setlist_id, &entry_id)
            .await
            .map_err(|e| e.to_string())
    }
}
