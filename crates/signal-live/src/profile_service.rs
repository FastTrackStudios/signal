use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> ProfileService
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
    async fn list_profiles(&self, _cx: &Context) -> Vec<Profile> {
        self.profile_repo.list_profiles().await.unwrap_or_default()
    }

    async fn load_profile(&self, _cx: &Context, id: ProfileId) -> Option<Profile> {
        self.profile_repo.load_profile(&id).await.ok().flatten()
    }

    async fn save_profile(&self, _cx: &Context, profile: Profile) -> () {
        for variant in &profile.patches {
            if variant.validate_overrides().is_err() {
                return;
            }
        }
        let _ = self.profile_repo.save_profile(&profile).await;
    }

    async fn delete_profile(&self, _cx: &Context, id: ProfileId) -> () {
        let _ = self.profile_repo.delete_profile(&id).await;
    }

    async fn load_profile_variant(
        &self,
        _cx: &Context,
        profile_id: ProfileId,
        variant_id: PatchId,
    ) -> Option<Patch> {
        self.profile_repo
            .load_variant(&profile_id, &variant_id)
            .await
            .ok()
            .flatten()
    }
}
