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
    async fn list_scene_templates(&self, _cx: &Context) -> Vec<SceneTemplate> {
        self.scene_template_repo
            .list_scene_templates()
            .await
            .unwrap_or_default()
    }

    async fn load_scene_template(
        &self,
        _cx: &Context,
        id: SceneTemplateId,
    ) -> Option<SceneTemplate> {
        self.scene_template_repo
            .load_scene_template(&id)
            .await
            .ok()
            .flatten()
    }

    async fn save_scene_template(&self, _cx: &Context, template: SceneTemplate) {
        let _ = self
            .scene_template_repo
            .save_scene_template(&template)
            .await;
    }

    async fn delete_scene_template(&self, _cx: &Context, id: SceneTemplateId) {
        let _ = self.scene_template_repo.delete_scene_template(&id).await;
    }

    async fn reorder_scene_templates(&self, _cx: &Context, ordered_ids: Vec<SceneTemplateId>) {
        let _ = self
            .scene_template_repo
            .reorder_scene_templates(&ordered_ids)
            .await;
    }
}
