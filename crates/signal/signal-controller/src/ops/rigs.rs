use crate::{SignalApi, SignalController};
use signal_proto::{
    engine::EngineId,
    rig::{Rig, RigId, RigScene, RigSceneId},
};

/// Handle for rig operations.
pub struct RigOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> RigOps<S> {
    pub async fn list(&self) -> Vec<Rig> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_rigs(&cx).await
    }

    pub async fn load(&self, id: impl Into<RigId>) -> Option<Rig> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_rig(&cx, id.into()).await
    }

    pub async fn create(&self, name: impl Into<String>, engine_ids: Vec<EngineId>) -> Rig {
        let rig = Rig::new(
            RigId::new(),
            name,
            engine_ids,
            RigScene::new(RigSceneId::new(), "Default"),
        );
        self.save(rig.clone()).await;
        rig
    }

    pub async fn save(&self, rig: Rig) -> Rig {
        let cx = self.0.context_factory.make_context();
        self.0.service.save_rig(&cx, rig.clone()).await;
        rig
    }

    pub async fn delete(&self, id: impl Into<RigId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_rig(&cx, id.into()).await;
    }

    pub async fn load_variant(
        &self,
        rig_id: impl Into<RigId>,
        variant_id: impl Into<RigSceneId>,
    ) -> Option<RigScene> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_rig_variant(&cx, rig_id.into(), variant_id.into())
            .await
    }

    pub async fn save_scene(&self, rig_id: impl Into<RigId>, scene: RigScene) {
        let rig_id = rig_id.into();
        if let Some(mut rig) = self.load(rig_id).await {
            if let Some(pos) = rig.variants.iter().position(|v| v.id == scene.id) {
                rig.variants[pos] = scene;
            } else {
                rig.variants.push(scene);
            }
            self.save(rig).await;
        }
    }

    pub async fn reorder_scenes(&self, rig_id: impl Into<RigId>, ordered_scene_ids: &[RigSceneId]) {
        let rig_id = rig_id.into();
        if let Some(mut rig) = self.load(rig_id.clone()).await {
            super::reorder_by_id(&mut rig.variants, ordered_scene_ids, |v| &v.id);
            self.save(rig).await;
        }
    }

    pub async fn by_tag(&self, tag: &str) -> Vec<Rig> {
        let all = self.list().await;
        all.into_iter()
            .filter(|r| r.metadata.tags.contains(tag))
            .collect()
    }

    pub async fn find_by_name(&self, name: &str) -> Option<Rig> {
        self.list().await.into_iter().find(|r| r.name == name)
    }

    pub async fn rename(&self, id: impl Into<RigId>, new_name: impl Into<String>) {
        if let Some(mut rig) = self.load(id).await {
            rig.name = new_name.into();
            self.save(rig).await;
        }
    }

    /// Load a rig, apply a closure to one of its scenes, and save.
    pub async fn update_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
        f: impl FnOnce(&mut RigScene),
    ) {
        let rig_id = rig_id.into();
        let scene_id = scene_id.into();
        if let Some(mut rig) = self.load(rig_id).await {
            if let Some(v) = rig.variants.iter_mut().find(|v| v.id == scene_id) {
                f(v);
            }
            self.save(rig).await;
        }
    }
}
