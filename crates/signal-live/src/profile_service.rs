//! Profile service implementation — CRUD for profiles and patch variants.
//!
//! Implements [`ProfileService`] on [`SignalLive`], delegating persistence
//! to the underlying [`ProfileRepo`].

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
    async fn list_profiles(&self) -> Result<Vec<Profile>, SignalServiceError> {
        self.profile_repo
            .list_profiles()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn load_profile(&self, id: ProfileId) -> Result<Option<Profile>, SignalServiceError> {
        self.profile_repo
            .load_profile(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn save_profile(&self, profile: Profile) -> Result<(), SignalServiceError> {
        for variant in &profile.patches {
            variant
                .validate_overrides()
                .map_err(|e| SignalServiceError::ValidationError(format!("{e:?}")))?;
        }
        self.profile_repo
            .save_profile(&profile)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn delete_profile(&self, id: ProfileId) -> Result<(), SignalServiceError> {
        self.profile_repo
            .delete_profile(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn load_profile_variant(
        &self,
        profile_id: ProfileId,
        variant_id: PatchId,
    ) -> Result<Option<Patch>, SignalServiceError> {
        self.profile_repo
            .load_variant(&profile_id, &variant_id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }
}
