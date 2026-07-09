//! Profile operations — CRUD for profiles and patch variants.
//!
//! Provides [`ProfileOps`], a controller handle for managing profiles,
//! their patch variants, and resolving patches to full signal graphs.

use super::error::OpsError;
use crate::events;
use crate::{SignalApi, SignalController};
use signal_proto::{
    profile::{Patch, PatchId, PatchTarget, Profile, ProfileId},
    resolve::{ResolveError, ResolveTarget, ResolvedGraph},
    rig::{RigId, RigSceneId},
};

/// Handle for profile operations.
pub struct ProfileOps<S: SignalApi>(pub(crate) SignalController<S>);

impl<S: SignalApi> ProfileOps<S> {
    pub async fn list(&self) -> Result<Vec<Profile>, OpsError> {
        self.0
            .service
            .list_profiles()
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load(&self, id: impl Into<ProfileId>) -> Result<Option<Profile>, OpsError> {
        self.0
            .service
            .load_profile(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        default_patch_name: impl Into<String>,
        target: PatchTarget,
    ) -> Result<Profile, OpsError> {
        let profile = Profile::new(
            ProfileId::new(),
            name,
            Patch::new(PatchId::new(), default_patch_name, target),
        );
        self.save(profile.clone()).await?;
        Ok(profile)
    }

    pub async fn save(&self, profile: Profile) -> Result<Profile, OpsError> {
        self.0
            .service
            .save_profile(profile.clone())
            .await
            .map_err(OpsError::Storage)?;
        Ok(profile)
    }

    pub async fn delete(&self, id: impl Into<ProfileId>) -> Result<(), OpsError> {
        self.0
            .service
            .delete_profile(id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn load_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
    ) -> Result<Option<Patch>, OpsError> {
        self.0
            .service
            .load_profile_variant(profile_id.into(), patch_id.into())
            .await
            .map_err(OpsError::Storage)
    }

    pub async fn save_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch: Patch,
    ) -> Result<(), OpsError> {
        let profile_id = profile_id.into();
        if let Some(mut profile) = self.load(profile_id).await? {
            if let Some(pos) = profile.patches.iter().position(|p| p.id == patch.id) {
                profile.patches[pos] = patch;
            } else {
                profile.patches.push(patch);
            }
            self.save(profile).await?;
        }
        Ok(())
    }

    pub async fn activate(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: Option<impl Into<PatchId>>,
    ) -> Result<ResolvedGraph, ResolveError> {
        let profile_id = profile_id.into();

        let profile = self
            .0
            .service
            .load_profile(profile_id.clone())
            .await
            .map_err(|e| {
                ResolveError::NotFound(format!("storage error loading profile {profile_id}: {e}"))
            })?
            .ok_or_else(|| ResolveError::NotFound(format!("profile {profile_id}")))?;

        let patch_id = match patch_id {
            Some(id) => id.into(),
            None => profile.default_patch_id.clone(),
        };

        // Fast path: if the patch targets a RigScene and a rig scene applier is
        // available, switch via preloaded rig hierarchies (mute/unmute only, <5ms).
        // This skips the expensive graph resolution entirely.
        let patch = profile.patches.iter().find(|p| p.id == patch_id);
        if let Some(p) = &patch {
            if let PatchTarget::RigScene {
                ref rig_id,
                ref scene_id,
            } = p.target
            {
                if let Some(rig_applier) = self
                    .0
                    .daw_rig_applier
                    .read()
                    .expect("lock poisoned")
                    .clone()
                {
                    let applied = match rig_applier
                        .switch_scene(&rig_id.to_string(), &scene_id.to_string(), Some(&p.name))
                        .await
                    {
                        Ok(applied) => applied,
                        Err(e) => {
                            eprintln!("[signal] activate rig scene switch failed: {e}");
                            false
                        }
                    };

                    self.0.event_bus.emit(events::SignalEvent::PatchActivated {
                        profile_id: profile_id.to_string(),
                        patch_id: patch_id.to_string(),
                        applied_to_daw: applied,
                    });

                    // Return a minimal graph — the rig hierarchy is already in REAPER
                    return Ok(ResolvedGraph {
                        target: ResolveTarget::ProfilePatch {
                            profile_id: profile_id.clone(),
                            patch_id: patch_id.clone(),
                        },
                        rig_id: rig_id.clone(),
                        rig_scene_id: scene_id.clone(),
                        engines: Vec::new(),
                        effective_overrides: Vec::new(),
                    });
                }
            }
        }

        let graph = self
            .0
            .service
            .resolve_target(ResolveTarget::ProfilePatch {
                profile_id: profile_id.clone(),
                patch_id: patch_id.clone(),
            })
            .await?;

        let patch_name = profile
            .patches
            .iter()
            .find(|p| p.id == patch_id)
            .map(|p| p.name.as_str());
        let applied_to_daw =
            if let Some(applier) = self.0.daw_applier.read().expect("lock poisoned").clone() {
                match applier.apply_graph(&graph, patch_name).await {
                    Ok(_) => true,
                    Err(e) => {
                        eprintln!("[signal] activate_patch DAW apply failed: {e}");
                        false
                    }
                }
            } else {
                false
            };

        self.0.event_bus.emit(events::SignalEvent::PatchActivated {
            profile_id: profile_id.to_string(),
            patch_id: patch_id.to_string(),
            applied_to_daw,
        });

        Ok(graph)
    }

    pub async fn activate_default(
        &self,
        profile_id: impl Into<ProfileId>,
    ) -> Result<ResolvedGraph, ResolveError> {
        self.activate(profile_id, None::<PatchId>).await
    }

    pub async fn set_patch_target(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        target: PatchTarget,
    ) -> Result<(), OpsError> {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        if let Some(mut profile) = self.load(profile_id).await? {
            if let Some(patch) = profile.patches.iter_mut().find(|p| p.id == patch_id) {
                patch.target = target;
            }
            self.save(profile).await?;
        }
        Ok(())
    }

    pub async fn set_patch_preset(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) -> Result<(), OpsError> {
        self.set_patch_target(
            profile_id,
            patch_id,
            PatchTarget::RigScene {
                rig_id: rig_id.into(),
                scene_id: scene_id.into(),
            },
        )
        .await
    }

    pub async fn reorder_patches(
        &self,
        profile_id: impl Into<ProfileId>,
        ordered_patch_ids: &[PatchId],
    ) -> Result<(), OpsError> {
        let profile_id = profile_id.into();
        if let Some(mut profile) = self.load(profile_id.clone()).await? {
            super::reorder_by_id(&mut profile.patches, ordered_patch_ids, |p| &p.id);
            self.save(profile).await?;
        }
        Ok(())
    }

    pub async fn by_tag(&self, tag: &str) -> Result<Vec<Profile>, OpsError> {
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|p| p.metadata.tags.contains(tag))
            .collect())
    }

    /// Find a profile by name (first match).
    pub async fn find_by_name(&self, name: &str) -> Result<Option<Profile>, OpsError> {
        Ok(self.list().await?.into_iter().find(|p| p.name == name))
    }

    /// Rename a profile.
    pub async fn rename(
        &self,
        id: impl Into<ProfileId>,
        new_name: impl Into<String>,
    ) -> Result<(), OpsError> {
        if let Some(mut profile) = self.load(id).await? {
            profile.name = new_name.into();
            self.save(profile).await?;
        }
        Ok(())
    }

    /// Load a profile, apply a closure to one of its patches, and save.
    pub async fn update_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        f: impl FnOnce(&mut Patch),
    ) -> Result<(), OpsError> {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        if let Some(mut profile) = self.load(profile_id).await? {
            if let Some(patch) = profile.patches.iter_mut().find(|p| p.id == patch_id) {
                f(patch);
            }
            self.save(profile).await?;
        }
        Ok(())
    }

    /// Add a patch to a profile. Returns the updated profile, or `None` if the profile doesn't exist.
    pub async fn add_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch: Patch,
    ) -> Result<Option<Profile>, OpsError> {
        let profile_id = profile_id.into();
        if let Some(mut profile) = self.load(profile_id).await? {
            profile.add_patch(patch);
            Ok(Some(self.save(profile).await?))
        } else {
            Ok(None)
        }
    }

    /// Remove a patch from a profile. Returns the removed patch, or `None` if not found.
    pub async fn remove_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
    ) -> Result<Option<Patch>, OpsError> {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        if let Some(mut profile) = self.load(profile_id).await? {
            let removed = profile.remove_patch(&patch_id);
            if removed.is_some() {
                self.save(profile).await?;
            }
            Ok(removed)
        } else {
            Ok(None)
        }
    }

    /// Duplicate a patch within a profile. Returns the new patch, or `None` if not found.
    pub async fn duplicate_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        new_name: impl Into<String>,
    ) -> Result<Option<Patch>, OpsError> {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        if let Some(mut profile) = self.load(profile_id).await? {
            if let Some(original) = profile.patch(&patch_id) {
                let dup = original.duplicate(PatchId::new(), new_name);
                let dup_clone = dup.clone();
                profile.add_patch(dup);
                self.save(profile).await?;
                Ok(Some(dup_clone))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Check if a profile exists.
    pub async fn exists(&self, id: impl Into<ProfileId>) -> Result<bool, OpsError> {
        Ok(self.load(id).await?.is_some())
    }

    /// Count all profiles.
    pub async fn count(&self) -> Result<usize, OpsError> {
        Ok(self.list().await?.len())
    }

    // region: --- try_* variants

    /// Add a patch, returning an error if the profile doesn't exist.
    pub async fn try_add_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch: Patch,
    ) -> Result<Profile, OpsError> {
        let profile_id = profile_id.into();
        let mut profile =
            self.load(profile_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "profile",
                    id: profile_id.to_string(),
                })?;
        profile.add_patch(patch);
        Ok(self.save(profile).await?)
    }

    /// Remove a patch, returning an error if the profile or patch doesn't exist.
    pub async fn try_remove_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
    ) -> Result<Patch, OpsError> {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        let mut profile =
            self.load(profile_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "profile",
                    id: profile_id.to_string(),
                })?;
        let removed = profile
            .remove_patch(&patch_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "patch",
                parent_id: profile_id.to_string(),
                variant_id: patch_id.to_string(),
            })?;
        self.save(profile).await?;
        Ok(removed)
    }

    /// Duplicate a patch, returning an error if the profile or patch doesn't exist.
    pub async fn try_duplicate_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        new_name: impl Into<String>,
    ) -> Result<Patch, OpsError> {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        let mut profile =
            self.load(profile_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "profile",
                    id: profile_id.to_string(),
                })?;
        let original = profile
            .patch(&patch_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "patch",
                parent_id: profile_id.to_string(),
                variant_id: patch_id.to_string(),
            })?;
        let dup = original.duplicate(PatchId::new(), new_name);
        let dup_clone = dup.clone();
        profile.add_patch(dup);
        self.save(profile).await?;
        Ok(dup_clone)
    }

    /// Save a patch within a profile, returning an error if the profile doesn't exist.
    pub async fn try_save_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch: Patch,
    ) -> Result<(), OpsError> {
        let profile_id = profile_id.into();
        let mut profile =
            self.load(profile_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "profile",
                    id: profile_id.to_string(),
                })?;
        if let Some(pos) = profile.patches.iter().position(|p| p.id == patch.id) {
            profile.patches[pos] = patch;
        } else {
            profile.patches.push(patch);
        }
        self.save(profile).await?;
        Ok(())
    }

    /// Update a patch via closure, returning an error if the profile or patch doesn't exist.
    pub async fn try_update_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        f: impl FnOnce(&mut Patch),
    ) -> Result<(), OpsError> {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        let mut profile =
            self.load(profile_id.clone())
                .await?
                .ok_or_else(|| OpsError::NotFound {
                    entity_type: "profile",
                    id: profile_id.to_string(),
                })?;
        let patch = profile
            .patches
            .iter_mut()
            .find(|p| p.id == patch_id)
            .ok_or_else(|| OpsError::VariantNotFound {
                entity_type: "patch",
                parent_id: profile_id.to_string(),
                variant_id: patch_id.to_string(),
            })?;
        f(patch);
        self.save(profile).await?;
        Ok(())
    }

    // endregion: --- try_* variants
}
