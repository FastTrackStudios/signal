use super::error::OpsError;
use crate::{SignalApi, SignalController};
use signal_proto::{
    engine::EngineId,
    rig::{Rig, RigId, RigScene, RigSceneId},
};

/// Handle for rig operations.
pub struct RigOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> RigOps<S> {
    pub async fn list(&self) -> Result<Vec<Rig>, OpsError> {
        self.0
            .service
            .list_rigs()
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<RigId>) -> Result<Option<Rig>, OpsError> {
        self.0
            .service
            .load_rig(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        engine_ids: Vec<EngineId>,
    ) -> Result<Rig, OpsError> {
        let rig = Rig::new(
            RigId::new(),
            name,
            engine_ids,
            RigScene::new(RigSceneId::new(), "Default"),
        );
        self.save(rig.clone()).await?;
        Ok(rig)
    }

    pub async fn save(&self, rig: Rig) -> Result<Rig, OpsError> {
        self.0
            .service
            .save_rig(rig.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(rig)
    }

    pub async fn delete(&self, id: impl Into<RigId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_rig(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_variant(
        &self,
        rig_id: impl Into<RigId>,
        variant_id: impl Into<RigSceneId>,
    ) -> Result<Option<RigScene>, OpsError> {
        self.0
            .service
            .load_rig_variant(rig_id.into(), variant_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene: RigScene,
    ) -> Result<(), OpsError> {
        let rig_id = rig_id.into();
        if let Some(mut rig) = self.load(rig_id).await? {
            if let Some(pos) = rig.variants.iter().position(|v| v.id == scene.id) {
                rig.variants[pos] = scene;
            } else {
                rig.variants.push(scene);
            }
            self.save(rig).await?;
        }
        Ok(())
    }

    pub async fn reorder_scenes(
        &self,
        rig_id: impl Into<RigId>,
        ordered_scene_ids: &[RigSceneId],
    ) -> Result<(), OpsError> {
        let rig_id = rig_id.into();
        if let Some(mut rig) = self.load(rig_id.clone()).await? {
            super::reorder_by_id(&mut rig.variants, ordered_scene_ids, |v| &v.id);
            self.save(rig).await?;
        }
        Ok(())
    }

    pub async fn by_tag(&self, tag: &str) -> Result<Vec<Rig>, OpsError> {
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|r| r.metadata.tags.contains(tag))
            .collect())
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Rig>, OpsError> {
        Ok(self.list().await?.into_iter().find(|r| r.name == name))
    }

    pub async fn rename(
        &self,
        id: impl Into<RigId>,
        new_name: impl Into<String>,
    ) -> Result<(), OpsError> {
        if let Some(mut rig) = self.load(id).await? {
            rig.name = new_name.into();
            self.save(rig).await?;
        }
        Ok(())
    }

    /// Load a rig, apply a closure to one of its scenes, and save.
    pub async fn update_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
        f: impl FnOnce(&mut RigScene),
    ) -> Result<(), OpsError> {
        let rig_id = rig_id.into();
        let scene_id = scene_id.into();
        if let Some(mut rig) = self.load(rig_id).await? {
            if let Some(v) = rig.variants.iter_mut().find(|v| v.id == scene_id) {
                f(v);
            }
            self.save(rig).await?;
        }
        Ok(())
    }

    /// Add a scene to a rig. Returns the updated rig, or `None` if the rig doesn't exist.
    pub async fn add_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene: RigScene,
    ) -> Result<Option<Rig>, OpsError> {
        let rig_id = rig_id.into();
        if let Some(mut rig) = self.load(rig_id).await? {
            rig.add_variant(scene);
            Ok(Some(self.save(rig).await?))
        } else {
            Ok(None)
        }
    }

    /// Remove a scene from a rig. Returns the removed scene, or `None` if not found.
    pub async fn remove_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) -> Result<Option<RigScene>, OpsError> {
        let rig_id = rig_id.into();
        let scene_id = scene_id.into();
        if let Some(mut rig) = self.load(rig_id).await? {
            let removed = rig.remove_variant(&scene_id);
            if removed.is_some() {
                self.save(rig).await?;
            }
            Ok(removed)
        } else {
            Ok(None)
        }
    }

    /// Duplicate a scene within a rig. Returns the new scene, or `None` if not found.
    pub async fn duplicate_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
        new_name: impl Into<String>,
    ) -> Result<Option<RigScene>, OpsError> {
        let rig_id = rig_id.into();
        let scene_id = scene_id.into();
        if let Some(mut rig) = self.load(rig_id).await? {
            if let Some(original) = rig.variant(&scene_id) {
                let dup = original.duplicate(RigSceneId::new(), new_name);
                let dup_clone = dup.clone();
                rig.add_variant(dup);
                self.save(rig).await?;
                Ok(Some(dup_clone))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Check if a rig exists.
    pub async fn exists(&self, id: impl Into<RigId>) -> Result<bool, OpsError> {
        Ok(self.load(id).await?.is_some())
    }

    /// Count all rigs.
    pub async fn count(&self) -> Result<usize, OpsError> {
        Ok(self.list().await?.len())
    }

    // region: --- try_* variants

    /// Add a scene, returning an error if the rig doesn't exist.
    pub async fn try_add_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene: RigScene,
    ) -> Result<Rig, OpsError> {
        let rig_id = rig_id.into();
        let mut rig = self
            .load(rig_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "rig",
                id: rig_id.to_string(),
            })?;
        rig.add_variant(scene);
        Ok(self.save(rig).await?)
    }

    /// Remove a scene, returning an error if the rig or scene doesn't exist.
    pub async fn try_remove_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) -> Result<RigScene, OpsError> {
        let rig_id = rig_id.into();
        let scene_id = scene_id.into();
        let mut rig = self
            .load(rig_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "rig",
                id: rig_id.to_string(),
            })?;
        let removed = rig
            .remove_variant(&scene_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "scene",
                parent_id: rig_id.to_string(),
                variant_id: scene_id.to_string(),
            })?;
        self.save(rig).await?;
        Ok(removed)
    }

    /// Duplicate a scene, returning an error if the rig or scene doesn't exist.
    pub async fn try_duplicate_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
        new_name: impl Into<String>,
    ) -> Result<RigScene, OpsError> {
        let rig_id = rig_id.into();
        let scene_id = scene_id.into();
        let mut rig = self
            .load(rig_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "rig",
                id: rig_id.to_string(),
            })?;
        let original = rig
            .variant(&scene_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "scene",
                parent_id: rig_id.to_string(),
                variant_id: scene_id.to_string(),
            })?;
        let dup = original.duplicate(RigSceneId::new(), new_name);
        let dup_clone = dup.clone();
        rig.add_variant(dup);
        self.save(rig).await?;
        Ok(dup_clone)
    }

    /// Save a scene within a rig, returning an error if the rig doesn't exist.
    pub async fn try_save_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene: RigScene,
    ) -> Result<(), OpsError> {
        let rig_id = rig_id.into();
        let mut rig = self
            .load(rig_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "rig",
                id: rig_id.to_string(),
            })?;
        if let Some(pos) = rig.variants.iter().position(|v| v.id == scene.id) {
            rig.variants[pos] = scene;
        } else {
            rig.variants.push(scene);
        }
        self.save(rig).await?;
        Ok(())
    }

    /// Update a scene via closure, returning an error if the rig or scene doesn't exist.
    pub async fn try_update_scene(
        &self,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
        f: impl FnOnce(&mut RigScene),
    ) -> Result<(), OpsError> {
        let rig_id = rig_id.into();
        let scene_id = scene_id.into();
        let mut rig = self
            .load(rig_id.clone())
            .await?
            .ok_or_else(|| OpsError::NotFound {
                entity_type: "rig",
                id: rig_id.to_string(),
            })?;
        let scene = rig
            .variants
            .iter_mut()
            .find(|v| v.id == scene_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "scene",
                parent_id: rig_id.to_string(),
                variant_id: scene_id.to_string(),
            })?;
        f(scene);
        self.save(rig).await?;
        Ok(())
    }

    // endregion: --- try_* variants
}
