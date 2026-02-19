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
    pub async fn list(&self) -> Vec<Profile> {
        let cx = self.0.context_factory.make_context();
        self.0.service.list_profiles(&cx).await
    }

    pub async fn load(&self, id: impl Into<ProfileId>) -> Option<Profile> {
        let cx = self.0.context_factory.make_context();
        self.0.service.load_profile(&cx, id.into()).await
    }

    pub async fn create(
        &self,
        name: impl Into<String>,
        default_patch_name: impl Into<String>,
        target: PatchTarget,
    ) -> Profile {
        let profile = Profile::new(
            ProfileId::new(),
            name,
            Patch::new(PatchId::new(), default_patch_name, target),
        );
        self.save(profile.clone()).await;
        profile
    }

    pub async fn save(&self, profile: Profile) -> Profile {
        let cx = self.0.context_factory.make_context();
        self.0.service.save_profile(&cx, profile.clone()).await;
        profile
    }

    pub async fn delete(&self, id: impl Into<ProfileId>) {
        let cx = self.0.context_factory.make_context();
        self.0.service.delete_profile(&cx, id.into()).await;
    }

    pub async fn load_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
    ) -> Option<Patch> {
        let cx = self.0.context_factory.make_context();
        self.0
            .service
            .load_profile_variant(&cx, profile_id.into(), patch_id.into())
            .await
    }

    pub async fn save_patch(&self, profile_id: impl Into<ProfileId>, patch: Patch) {
        let profile_id = profile_id.into();
        if let Some(mut profile) = self.load(profile_id).await {
            if let Some(pos) = profile.patches.iter().position(|p| p.id == patch.id) {
                profile.patches[pos] = patch;
            } else {
                profile.patches.push(patch);
            }
            self.save(profile).await;
        }
    }

    pub async fn activate(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: Option<impl Into<PatchId>>,
    ) -> Result<ResolvedGraph, ResolveError> {
        let profile_id = profile_id.into();
        let cx = self.0.context_factory.make_context();

        let profile = self
            .0
            .service
            .load_profile(&cx, profile_id.clone())
            .await
            .ok_or_else(|| ResolveError::NotFound(format!("profile {profile_id}")))?;

        let patch_id = match patch_id {
            Some(id) => id.into(),
            None => profile.default_patch_id.clone(),
        };

        let graph = self
            .0
            .service
            .resolve_target(
                &cx,
                ResolveTarget::ProfilePatch {
                    profile_id: profile_id.clone(),
                    patch_id: patch_id.clone(),
                },
            )
            .await?;

        let patch_name = profile
            .patches
            .iter()
            .find(|p| p.id == patch_id)
            .map(|p| p.name.as_str());
        let applied_to_daw = if let Some(applier) = &self.0.daw_applier {
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
    ) {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        if let Some(mut profile) = self.load(profile_id).await {
            if let Some(patch) = profile.patches.iter_mut().find(|p| p.id == patch_id) {
                patch.target = target;
            }
            self.save(profile).await;
        }
    }

    pub async fn set_patch_preset(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        rig_id: impl Into<RigId>,
        scene_id: impl Into<RigSceneId>,
    ) {
        self.set_patch_target(
            profile_id,
            patch_id,
            PatchTarget::RigScene {
                rig_id: rig_id.into(),
                scene_id: scene_id.into(),
            },
        )
        .await;
    }

    pub async fn reorder_patches(
        &self,
        profile_id: impl Into<ProfileId>,
        ordered_patch_ids: &[PatchId],
    ) {
        let profile_id = profile_id.into();
        if let Some(mut profile) = self.load(profile_id.clone()).await {
            super::reorder_by_id(&mut profile.patches, ordered_patch_ids, |p| &p.id);
            self.save(profile).await;
        }
    }

    pub async fn by_tag(&self, tag: &str) -> Vec<Profile> {
        let all = self.list().await;
        all.into_iter()
            .filter(|p| p.metadata.tags.contains(tag))
            .collect()
    }

    /// Find a profile by name (first match).
    pub async fn find_by_name(&self, name: &str) -> Option<Profile> {
        self.list().await.into_iter().find(|p| p.name == name)
    }

    /// Rename a profile.
    pub async fn rename(&self, id: impl Into<ProfileId>, new_name: impl Into<String>) {
        if let Some(mut profile) = self.load(id).await {
            profile.name = new_name.into();
            self.save(profile).await;
        }
    }

    /// Load a profile, apply a closure to one of its patches, and save.
    pub async fn update_patch(
        &self,
        profile_id: impl Into<ProfileId>,
        patch_id: impl Into<PatchId>,
        f: impl FnOnce(&mut Patch),
    ) {
        let profile_id = profile_id.into();
        let patch_id = patch_id.into();
        if let Some(mut profile) = self.load(profile_id).await {
            if let Some(patch) = profile.patches.iter_mut().find(|p| p.id == patch_id) {
                f(patch);
            }
            self.save(profile).await;
        }
    }
}
