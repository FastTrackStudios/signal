//! Engine operations — CRUD for engines and engine scene variants.
//!
//! Provides [`EngineOps`], a controller handle for managing engines,
//! their scene variants, and layer selections within each scene.

use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::{
    engine::{Engine, EngineId, EngineScene, EngineSceneId},
    layer::LayerId,
    EngineType,
};

/// Handle for engine operations.
pub struct EngineOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> EngineOps<S> {
    pub async fn list(&self) -> Result<Vec<Engine>, OpsError> {
        self.0
            .service
            .list_engines()
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<EngineId>) -> Result<Option<Engine>, OpsError> {
        self.0
            .service
            .load_engine(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        engine_type: EngineType,
        layer_ids: Vec<LayerId>,
    ) -> Result<Engine, OpsError> {
        let engine = Engine::new(
            EngineId::new(),
            name,
            engine_type,
            layer_ids,
            EngineScene::new(EngineSceneId::new(), "Default"),
        );
        self.save(engine.clone()).await?;
        Ok(engine)
    }

    pub async fn save(&self, engine: Engine) -> Result<Engine, OpsError> {
        self.0
            .service
            .save_engine(engine.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(engine)
    }

    pub async fn delete(&self, id: impl Into<EngineId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_engine(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_variant(
        &self,
        engine_id: impl Into<EngineId>,
        variant_id: impl Into<EngineSceneId>,
    ) -> Result<Option<EngineScene>, OpsError> {
        self.0
            .service
            .load_engine_variant(engine_id.into(), variant_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene: EngineScene,
    ) -> Result<(), OpsError> {
        let engine_id = engine_id.into();
        if let Some(mut engine) = self.load(engine_id).await? {
            if let Some(pos) = engine.variants.iter().position(|v| v.id == scene.id) {
                engine.variants[pos] = scene;
            } else {
                engine.variants.push(scene);
            }
            self.save(engine).await?;
        }
        Ok(())
    }

    pub async fn by_tag(&self, tag: &str) -> Result<Vec<Engine>, OpsError> {
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|e| e.metadata.tags.contains(tag))
            .collect())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Engine>, OpsError> {
        Ok(self.list().await?.into_iter().find(|e| e.name == name))
    }

    pub async fn rename(
        &self,
        id: impl Into<EngineId>,
        new_name: impl Into<String>,
    ) -> Result<(), OpsError> {
        if let Some(mut engine) = self.load(id).await? {
            engine.name = new_name.into();
            self.save(engine).await?;
        }
        Ok(())
    }

    /// Load an engine, apply a closure to one of its scenes, and save.
    pub async fn update_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene_id: impl Into<EngineSceneId>,
        f: impl FnOnce(&mut EngineScene),
    ) -> Result<(), OpsError> {
        let engine_id = engine_id.into();
        let scene_id = scene_id.into();
        if let Some(mut engine) = self.load(engine_id).await? {
            if let Some(v) = engine.variants.iter_mut().find(|v| v.id == scene_id) {
                f(v);
            }
            self.save(engine).await?;
        }
        Ok(())
    }

    /// Add a scene to an engine. Returns the updated engine, or `None` if the engine doesn't exist.
    pub async fn add_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene: EngineScene,
    ) -> Result<Option<Engine>, OpsError> {
        let engine_id = engine_id.into();
        if let Some(mut engine) = self.load(engine_id).await? {
            engine.add_variant(scene);
            Ok(Some(self.save(engine).await?))
        } else {
            Ok(None)
        }
    }

    /// Remove a scene from an engine. Returns the removed scene, or `None` if not found.
    pub async fn remove_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene_id: impl Into<EngineSceneId>,
    ) -> Result<Option<EngineScene>, OpsError> {
        let engine_id = engine_id.into();
        let scene_id = scene_id.into();
        if let Some(mut engine) = self.load(engine_id).await? {
            let removed = engine.remove_variant(&scene_id);
            if removed.is_some() {
                self.save(engine).await?;
            }
            Ok(removed)
        } else {
            Ok(None)
        }
    }

    /// Duplicate a scene within an engine. Returns the new scene, or `None` if not found.
    pub async fn duplicate_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene_id: impl Into<EngineSceneId>,
        new_name: impl Into<String>,
    ) -> Result<Option<EngineScene>, OpsError> {
        let engine_id = engine_id.into();
        let scene_id = scene_id.into();
        if let Some(mut engine) = self.load(engine_id).await? {
            if let Some(original) = engine.variant(&scene_id) {
                let dup = original.duplicate(EngineSceneId::new(), new_name);
                let dup_clone = dup.clone();
                engine.add_variant(dup);
                self.save(engine).await?;
                Ok(Some(dup_clone))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Check if an engine exists.
    pub async fn exists(&self, id: impl Into<EngineId>) -> Result<bool, OpsError> {
        Ok(self.load(id).await?.is_some())
    }

    /// Count all engines.
    pub async fn count(&self) -> Result<usize, OpsError> {
        Ok(self.list().await?.len())
    }

    // region: --- try_* variants

    /// Add a scene, returning an error if the engine doesn't exist.
    pub async fn try_add_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene: EngineScene,
    ) -> Result<Engine, OpsError> {
        let engine_id = engine_id.into();
        let mut engine = self
            .load(engine_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "engine",
                id: engine_id.to_string(),
            })?;
        engine.add_variant(scene);
        Ok(self.save(engine).await?)
    }

    /// Remove a scene, returning an error if the engine or scene doesn't exist.
    pub async fn try_remove_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene_id: impl Into<EngineSceneId>,
    ) -> Result<EngineScene, OpsError> {
        let engine_id = engine_id.into();
        let scene_id = scene_id.into();
        let mut engine = self
            .load(engine_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "engine",
                id: engine_id.to_string(),
            })?;
        let removed =
            engine
                .remove_variant(&scene_id)
                .ok_or_else(|| OpsError::VariantNotFound {
                    entity_type: "scene",
                    parent_id: engine_id.to_string(),
                    variant_id: scene_id.to_string(),
                })?;
        self.save(engine).await?;
        Ok(removed)
    }

    /// Duplicate a scene, returning an error if the engine or scene doesn't exist.
    pub async fn try_duplicate_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene_id: impl Into<EngineSceneId>,
        new_name: impl Into<String>,
    ) -> Result<EngineScene, OpsError> {
        let engine_id = engine_id.into();
        let scene_id = scene_id.into();
        let mut engine = self
            .load(engine_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "engine",
                id: engine_id.to_string(),
            })?;
        let original = engine
            .variant(&scene_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "scene",
                parent_id: engine_id.to_string(),
                variant_id: scene_id.to_string(),
            })?;
        let dup = original.duplicate(EngineSceneId::new(), new_name);
        let dup_clone = dup.clone();
        engine.add_variant(dup);
        self.save(engine).await?;
        Ok(dup_clone)
    }

    /// Save a scene within an engine, returning an error if the engine doesn't exist.
    pub async fn try_save_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene: EngineScene,
    ) -> Result<(), OpsError> {
        let engine_id = engine_id.into();
        let mut engine = self
            .load(engine_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "engine",
                id: engine_id.to_string(),
            })?;
        if let Some(pos) = engine.variants.iter().position(|v| v.id == scene.id) {
            engine.variants[pos] = scene;
        } else {
            engine.variants.push(scene);
        }
        self.save(engine).await?;
        Ok(())
    }

    /// Update a scene via closure, returning an error if the engine or scene doesn't exist.
    pub async fn try_update_scene(
        &self,
        engine_id: impl Into<EngineId>,
        scene_id: impl Into<EngineSceneId>,
        f: impl FnOnce(&mut EngineScene),
    ) -> Result<(), OpsError> {
        let engine_id = engine_id.into();
        let scene_id = scene_id.into();
        let mut engine = self
            .load(engine_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "engine",
                id: engine_id.to_string(),
            })?;
        let scene = engine
            .variants
            .iter_mut()
            .find(|v| v.id == scene_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "scene",
                parent_id: engine_id.to_string(),
                variant_id: scene_id.to_string(),
            })?;
        f(scene);
        self.save(engine).await?;
        Ok(())
    }

    // endregion: --- try_* variants
}
