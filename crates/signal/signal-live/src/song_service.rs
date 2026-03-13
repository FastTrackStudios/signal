use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> SongService for SignalLive<B, M, L, E, R, P, So, Se, St, Ra>
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
    async fn list_songs(&self) -> Result<Vec<Song>, String> {
        self.song_repo.list_songs().await.map_err(|e| e.to_string())
    }

    async fn load_song(&self, id: SongId) -> Result<Option<Song>, String> {
        self.song_repo
            .load_song(&id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn save_song(&self, song: Song) -> Result<(), String> {
        for variant in &song.sections {
            variant.validate_overrides().map_err(|e| format!("{e:?}"))?;
        }
        self.song_repo
            .save_song(&song)
            .await
            .map_err(|e| e.to_string())
    }

    async fn delete_song(&self, id: SongId) -> Result<(), String> {
        self.song_repo
            .delete_song(&id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_song_variant(
        &self,
        song_id: SongId,
        variant_id: SectionId,
    ) -> Result<Option<Section>, String> {
        self.song_repo
            .load_variant(&song_id, &variant_id)
            .await
            .map_err(|e| e.to_string())
    }
}
