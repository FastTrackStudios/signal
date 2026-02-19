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
    async fn list_songs(&self, _cx: &Context) -> Vec<Song> {
        self.song_repo.list_songs().await.unwrap_or_default()
    }

    async fn load_song(&self, _cx: &Context, id: SongId) -> Option<Song> {
        self.song_repo.load_song(&id).await.ok().flatten()
    }

    async fn save_song(&self, _cx: &Context, song: Song) -> () {
        for variant in &song.sections {
            if variant.validate_overrides().is_err() {
                return;
            }
        }
        let _ = self.song_repo.save_song(&song).await;
    }

    async fn delete_song(&self, _cx: &Context, id: SongId) -> () {
        let _ = self.song_repo.delete_song(&id).await;
    }

    async fn load_song_variant(
        &self,
        _cx: &Context,
        song_id: SongId,
        variant_id: SectionId,
    ) -> Option<Section> {
        self.song_repo
            .load_variant(&song_id, &variant_id)
            .await
            .ok()
            .flatten()
    }
}
