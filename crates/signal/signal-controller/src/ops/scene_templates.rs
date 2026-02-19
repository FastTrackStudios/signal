use crate::{SignalApi, SignalController};
use signal_proto::scene_template::{SceneTemplate, SceneTemplateId};

/// Handle for scene template operations.
pub struct SceneTemplateOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> SceneTemplateOps<S> {
    pub async fn list(&self) -> Vec<SceneTemplate> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_scene_templates(&cx).await
    }

    pub async fn load(&self, id: impl Into<SceneTemplateId>) -> Option<SceneTemplate> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_scene_template(&cx, id.into()).await
    }

    pub async fn save(&self, template: SceneTemplate) -> SceneTemplate {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .save_scene_template(&cx, template.clone())
            .await;
        template
    }

    pub async fn delete(&self, id: impl Into<SceneTemplateId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_scene_template(&cx, id.into()).await;
    }

    pub async fn reorder(&self, ordered_ids: Vec<SceneTemplateId>) {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .reorder_scene_templates(&cx, ordered_ids)
            .await;
    }
}
