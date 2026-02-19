//! Profile repository — data access for Profile collections and Patch variants.

use sea_orm::*;
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::profile::{Patch, PatchId, PatchTarget, Profile, ProfileId};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait ProfileRepo: Send + Sync {
    async fn list_profiles(&self) -> StorageResult<Vec<Profile>>;
    async fn load_profile(&self, id: &ProfileId) -> StorageResult<Option<Profile>>;
    async fn save_profile(&self, profile: &Profile) -> StorageResult<()>;
    async fn delete_profile(&self, id: &ProfileId) -> StorageResult<()>;
    async fn load_variant(
        &self,
        profile_id: &ProfileId,
        variant_id: &PatchId,
    ) -> StorageResult<Option<Patch>>;
}

// endregion: --- Trait

// region: --- ProfileRepoLive

#[derive(Clone)]
pub struct ProfileRepoLive {
    db: DatabaseConnection,
}

impl ProfileRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut profiles = schema.create_table_from_entity(entity::profile::Entity);
        profiles.if_not_exists();
        self.db.execute(backend.build(&profiles)).await?;

        let mut patches = schema.create_table_from_entity(entity::patch::Entity);
        patches.if_not_exists();
        self.db.execute(backend.build(&patches)).await?;

        Ok(())
    }

    fn variant_state_to_json(patch: &Patch) -> StorageResult<String> {
        let state = PatchState {
            target: &patch.target,
            overrides: &patch.overrides,
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize patch: {e}")))
    }

    fn variant_state_from_json(json: &str) -> StorageResult<PatchStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse patch json: {e}")))
    }

    fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
        serde_json::to_string(metadata)
            .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
    }

    fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
    }

    fn variant_from_model(model: &entity::patch::Model) -> StorageResult<Patch> {
        let state = Self::variant_state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(Patch {
            id: model.variant_id_branded(),
            name: model.name.clone(),
            target: state.target,
            overrides: state.overrides,
            metadata,
        })
    }

    async fn assemble_profile(&self, model: &entity::profile::Model) -> StorageResult<Profile> {
        let variant_models = entity::patch::Entity::find()
            .filter(entity::patch::Column::ProfileId.eq(model.id.clone()))
            .order_by_asc(entity::patch::Column::Position)
            .all(&self.db)
            .await?;

        let mut patches = Vec::with_capacity(variant_models.len());
        for vm in &variant_models {
            patches.push(Self::variant_from_model(vm)?);
        }

        let metadata = Self::metadata_from_json(&model.metadata_json)?;

        Ok(Profile {
            id: model.profile_id_branded(),
            name: model.name.clone(),
            default_patch_id: model.default_variant_id_branded(),
            patches,
            metadata,
        })
    }
}

// endregion: --- ProfileRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct PatchState<'a> {
    target: &'a PatchTarget,
    overrides: &'a [Override],
}

#[derive(serde::Deserialize)]
struct PatchStateOwned {
    target: PatchTarget,
    overrides: Vec<Override>,
}

// endregion: --- Serialization types

// region: --- Trait impl

#[async_trait::async_trait]
impl ProfileRepo for ProfileRepoLive {
    async fn list_profiles(&self) -> StorageResult<Vec<Profile>> {
        let models = entity::profile::Entity::find()
            .order_by_asc(entity::profile::Column::Id)
            .all(&self.db)
            .await?;

        let mut out = Vec::with_capacity(models.len());
        for model in &models {
            out.push(self.assemble_profile(model).await?);
        }
        Ok(out)
    }

    async fn load_profile(&self, id: &ProfileId) -> StorageResult<Option<Profile>> {
        let model = entity::profile::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(self.assemble_profile(m).await?)),
            None => Ok(None),
        }
    }

    async fn save_profile(&self, profile: &Profile) -> StorageResult<()> {
        entity::profile::Entity::delete_by_id(profile.id.as_str().to_string())
            .exec(&self.db)
            .await
            .ok();

        entity::profile::Entity::insert(entity::profile::ActiveModel {
            id: Set(profile.id.as_str().to_string()),
            name: Set(profile.name.clone()),
            default_variant_id: Set(profile.default_patch_id.as_str().to_string()),
            metadata_json: Set(Self::metadata_to_json(&profile.metadata)?),
        })
        .exec(&self.db)
        .await?;

        for (position, patch) in profile.patches.iter().enumerate() {
            entity::patch::Entity::insert(entity::patch::ActiveModel {
                id: Set(patch.id.as_str().to_string()),
                profile_id: Set(profile.id.as_str().to_string()),
                position: Set(position as i32),
                name: Set(patch.name.clone()),
                state_json: Set(Self::variant_state_to_json(patch)?),
                metadata_json: Set(Self::metadata_to_json(&patch.metadata)?),
            })
            .exec(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn delete_profile(&self, id: &ProfileId) -> StorageResult<()> {
        entity::profile::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn load_variant(
        &self,
        profile_id: &ProfileId,
        variant_id: &PatchId,
    ) -> StorageResult<Option<Patch>> {
        let model = entity::patch::Entity::find_by_id(variant_id.as_str().to_string())
            .filter(entity::patch::Column::ProfileId.eq(profile_id.as_str().to_string()))
            .one(&self.db)
            .await?;

        match model {
            Some(ref m) => Ok(Some(Self::variant_from_model(m)?)),
            None => Ok(None),
        }
    }
}

// endregion: --- Trait impl

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;
    use signal_proto::rig::{RigId, RigSceneId};
    use signal_proto::seed_id;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn pid(name: &str) -> ProfileId {
        ProfileId::from_uuid(seed_id(name))
    }
    fn paid(name: &str) -> PatchId {
        PatchId::from_uuid(seed_id(name))
    }

    async fn test_repo() -> Result<ProfileRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = ProfileRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    fn sample_profile() -> Profile {
        let patch1 = Patch::from_rig_scene(
            seed_id("p-clean"),
            "Clean",
            seed_id("rig-1"),
            seed_id("rs-clean"),
        );
        let patch2 = Patch::from_rig_scene(
            seed_id("p-lead"),
            "Lead",
            seed_id("rig-1"),
            seed_id("rs-lead"),
        );
        let mut profile = Profile::new(seed_id("profile-1"), "Worship", patch1);
        profile.add_patch(patch2);
        profile
    }

    #[tokio::test]
    async fn save_load_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_profile(&sample_profile()).await?;

        let loaded = repo.load_profile(&pid("profile-1")).await?;
        let loaded = loaded.expect("should find profile");
        assert_eq!(loaded.name, "Worship");
        assert_eq!(loaded.patches.len(), 2);
        assert_eq!(loaded.default_patch_id, paid("p-clean"));
        Ok(())
    }

    #[tokio::test]
    async fn list_profiles_returns_all() -> Result<()> {
        let repo = test_repo().await?;
        let p1 = Profile::new(
            seed_id("p1"),
            "Profile 1",
            Patch::from_rig_scene(seed_id("pa1"), "Default", seed_id("r1"), seed_id("rs1")),
        );
        let p2 = Profile::new(
            seed_id("p2"),
            "Profile 2",
            Patch::from_rig_scene(seed_id("pa2"), "Default", seed_id("r2"), seed_id("rs2")),
        );
        repo.save_profile(&p1).await?;
        repo.save_profile(&p2).await?;

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn load_missing_returns_none() -> Result<()> {
        let repo = test_repo().await?;
        let loaded = repo.load_profile(&pid("nonexistent")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn delete_profile_removes_it() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_profile(&sample_profile()).await?;
        repo.delete_profile(&pid("profile-1")).await?;
        let loaded = repo.load_profile(&pid("profile-1")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_variant_by_id() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_profile(&sample_profile()).await?;

        let variant = repo
            .load_variant(&pid("profile-1"), &paid("p-lead"))
            .await?;
        let variant = variant.expect("should find patch");
        assert_eq!(variant.name, "Lead");
        assert_eq!(
            variant.target,
            PatchTarget::RigScene {
                rig_id: RigId::from_uuid(seed_id("rig-1")),
                scene_id: RigSceneId::from_uuid(seed_id("rs-lead")),
            }
        );
        Ok(())
    }

    #[tokio::test]
    async fn patch_rig_references_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        let patch = Patch::from_rig_scene(
            seed_id("p1"),
            "Test",
            seed_id("rig-guitar"),
            seed_id("scene-heavy"),
        )
        .with_override(signal_proto::overrides::Override::set(
            "module.drive.param.gain",
            0.9,
        ));
        let profile = Profile::new(seed_id("prof-1"), "Test Profile", patch);
        repo.save_profile(&profile).await?;

        let loaded = repo.load_profile(&pid("prof-1")).await?.unwrap();
        let p = &loaded.patches[0];
        assert_eq!(
            p.target,
            PatchTarget::RigScene {
                rig_id: RigId::from_uuid(seed_id("rig-guitar")),
                scene_id: RigSceneId::from_uuid(seed_id("scene-heavy")),
            }
        );
        assert_eq!(p.overrides.len(), 1);
        Ok(())
    }
}

// endregion: --- Tests
