use crate::{SignalApi, SignalController};
use signal_proto::{
    engine::{Engine, EngineId, EngineScene, EngineSceneId},
    layer::LayerId,
    EngineType,
};

/// Handle for engine operations.
pub struct EngineOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> EngineOps<S> {
    pub async fn list(&self) -> Vec<Engine> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_engines(&cx).await
    }

    pub async fn load(&self, id: impl Into<EngineId>) -> Option<Engine> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_engine(&cx, id.into()).await
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        engine_type: EngineType,
        layer_ids: Vec<LayerId>,
    ) -> Engine {
        let engine = Engine::new(
            EngineId::new(),
            name,
            engine_type,
            layer_ids,
            EngineScene::new(EngineSceneId::new(), "Default"),
        );
        self.save(engine.clone()).await;
        engine
    }

    pub async fn save(&self, engine: Engine) -> Engine {
        let cx = self.0.context_factory.make_context();
        self.0.service.save_engine(&cx, engine.clone()).await;
        engine
    }

    pub async fn delete(&self, id: impl Into<EngineId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_engine(&cx, id.into()).await;
    }

    pub async fn load_variant(
        &self,
        engine_id: impl Into<EngineId>,
        variant_id: impl Into<EngineSceneId>,
    ) -> Option<EngineScene> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_engine_variant(&cx, engine_id.into(), variant_id.into())
            .await
    }

    pub async fn save_scene(&self, engine_id: impl Into<EngineId>, scene: EngineScene) {
        let engine_id = engine_id.into();
        if let Some(mut engine) = self.load(engine_id).await {
            if let Some(pos) = engine.variants.iter().position(|v| v.id == scene.id) {
                engine.variants[pos] = scene;
            } else {
                engine.variants.push(scene);
            }
            self.save(engine).await;
        }
    }

    pub async fn by_tag(&self, tag: &str) -> Vec<Engine> {
        let all = self.list().await;
        all.into_iter()
            .filter(|e| e.metadata.tags.contains(tag))
            .collect()
    }

    pub async fn find_by_name(&self, name: &str) -> Option<Engine> {
        self.list().await.into_iter().find(|e| e.name == name)
    }

    pub async fn rename(&self, id: impl Into<EngineId>, new_name: impl Into<String>) {
        if let Some(mut engine) = self.load(id).await {
            engine.name = new_name.into();
            self.save(engine).await;
        }
    }

    /// Load an engine, apply a closure to one of its scenes, and save.
    pub async fn update_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene_id: impl Into<EngineSceneId>,
        f: impl FnOnce(&mut EngineScene),
    ) {
        let engine_id = engine_id.into();
        let scene_id = scene_id.into();
        if let Some(mut engine) = self.load(engine_id).await {
            if let Some(v) = engine.variants.iter_mut().find(|v| v.id == scene_id) {
                f(v);
            }
            self.save(engine).await;
        }
    }
}
