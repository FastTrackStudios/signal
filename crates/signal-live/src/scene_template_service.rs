//! Scene template service implementation — CRUD for reusable scene configurations.
//!
//! Implements [`SceneTemplateService`] on [`SignalLive`], delegating persistence
//! to the underlying [`SceneTemplateRepo`].

use super::*;

impl<B, M, L, E, R, P, So, Se, St, Ra> SceneTemplateService
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
    async fn list_scene_templates(&self) -> Result<Vec<SceneTemplate>, SignalServiceError> {
        self.scene_template_repo
            .list_scene_templates()
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn load_scene_template(
        &self,
        id: SceneTemplateId,
    ) -> Result<Option<SceneTemplate>, SignalServiceError> {
        self.scene_template_repo
            .load_scene_template(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn save_scene_template(&self, template: SceneTemplate) -> Result<(), SignalServiceError> {
        self.scene_template_repo
            .save_scene_template(&template)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn delete_scene_template(&self, id: SceneTemplateId) -> Result<(), SignalServiceError> {
        self.scene_template_repo
            .delete_scene_template(&id)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }

    async fn reorder_scene_templates(
        &self,
        ordered_ids: Vec<SceneTemplateId>,
    ) -> Result<(), SignalServiceError> {
        self.scene_template_repo
            .reorder_scene_templates(&ordered_ids)
            .await
            .map_err(|e| SignalServiceError::StorageError(e.to_string()))
    }
}
