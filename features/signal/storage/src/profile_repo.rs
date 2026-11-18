//! Profile repository — data access for Profile collections and Patch variants.

use sea_orm::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ColIdx, ColumnTrait, ConnectionTrait,
    EntityTrait, Iden, IdenStatic, Iterable, QueryFilter, QueryOrder, QueryTrait, Schema, Set,
    StatementBuilder,
};
use signal_proto::default_profile::{
    DEFAULT_PROFILE_SETTING_KEY, LibrarySettings, all_around_profile, all_around_seed_ids,
    choose_default_profile,
};
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::profile::{Patch, PatchId, PatchTarget, Profile, ProfileId};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait ProfileRepo: Send + Sync + 'static {
    async fn list_profiles(&self) -> StorageResult<Vec<Profile>>;
    async fn load_profile(&self, id: &ProfileId) -> StorageResult<Option<Profile>>;
    async fn save_profile(&self, profile: &Profile) -> StorageResult<()>;
    /// Delete a profile. Refused ([`StorageError::Refused`]) for the library
    /// default profile.
    async fn delete_profile(&self, id: &ProfileId) -> StorageResult<()>;
    async fn load_variant(
        &self,
        profile_id: &ProfileId,
        variant_id: &PatchId,
    ) -> StorageResult<Option<Patch>>;

    /// Library-level settings, including the default profile pointer.
    async fn library_settings(&self) -> StorageResult<LibrarySettings>;
    /// Make `id` the library default profile. The profile must exist.
    async fn set_default_profile(&self, id: &ProfileId) -> StorageResult<()>;
    /// Make sure the library has exactly one valid default profile and return
    /// it: keep a valid pointer, else adopt a profile named "All-Around", else
    /// seed All-Around with one patch on `seed_target`.
    async fn ensure_default_profile(&self, seed_target: PatchTarget) -> StorageResult<Profile>;
}

// endregion: --- Trait

// region: --- ProfileRepoLive

#[derive(Clone)]
pub struct ProfileRepoLive {
    db: DatabaseConnection,
}

impl ProfileRepoLive {
    #[must_use]
    pub const fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Initialize the database schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the database schema cannot be created.
    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut profiles = schema.create_table_from_entity(entity::profile::Entity);
        profiles.if_not_exists();
        self.db.execute(backend.build(&profiles)).await?;

        let mut patches = schema.create_table_from_entity(entity::patch::Entity);
        patches.if_not_exists();
        self.db.execute(backend.build(&patches)).await?;

        // Library-level settings live beside the profiles they point at.
        let mut settings = schema.create_table_from_entity(entity::library_setting::Entity);
        settings.if_not_exists();
        self.db.execute(backend.build(&settings)).await?;

        Ok(())
    }

    async fn write_setting(&self, key: &str, value: &str) -> StorageResult<()> {
        entity::library_setting::Entity::delete_by_id(key.to_string())
            .exec(&self.db)
            .await?;
        entity::library_setting::Entity::insert(entity::library_setting::ActiveModel {
            key: Set(key.to_string()),
            value: Set(value.to_string()),
        })
        .exec(&self.db)
        .await?;
        Ok(())
    }

    fn variant_state_to_json(patch: &Patch) -> StorageResult<String> {
        let state = PatchState {
            slug: &patch.slug,
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
        // A patch saved before slugs existed gets the one its name makes —
        // its live-doc identity from here on (`Patch::slug`).
        let slug = state
            .slug
            .filter(|slug| !slug.is_empty())
            .unwrap_or_else(|| signal_proto::slug::slugify(&model.name));
        Ok(Patch {
            id: model.variant_id_branded(),
            name: model.name.clone(),
            slug,
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

/// A patch's row state. The slug rides here rather than in a column of
/// its own, so a library written before slugs needs no migration.
#[derive(serde::Serialize)]
struct PatchState<'a> {
    slug: &'a str,
    target: &'a PatchTarget,
    overrides: &'a [Override],
}

#[derive(serde::Deserialize)]
struct PatchStateOwned {
    #[serde(default)]
    slug: Option<String>,
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
        self.library_settings()
            .await?
            .check_delete(id)
            .map_err(|e| StorageError::Refused(e.to_string()))?;
        entity::profile::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn library_settings(&self) -> StorageResult<LibrarySettings> {
        let row =
            entity::library_setting::Entity::find_by_id(DEFAULT_PROFILE_SETTING_KEY.to_string())
                .one(&self.db)
                .await?;
        Ok(LibrarySettings {
            default_profile_id: row
                .filter(|r| !r.value.is_empty())
                .map(|r| ProfileId::from(r.value)),
        })
    }

    async fn set_default_profile(&self, id: &ProfileId) -> StorageResult<()> {
        if self.load_profile(id).await?.is_none() {
            return Err(StorageError::Data(format!(
                "cannot make {id} the default profile: no such profile"
            )));
        }
        self.write_setting(DEFAULT_PROFILE_SETTING_KEY, id.as_str())
            .await
    }

    async fn ensure_default_profile(&self, seed_target: PatchTarget) -> StorageResult<Profile> {
        let settings = self.library_settings().await?;
        let profiles = self.list_profiles().await?;
        let profile = if let Some(existing) = choose_default_profile(&settings, &profiles) {
            existing.clone()
        } else {
            let (profile_id, patch_id) = all_around_seed_ids();
            let seeded = all_around_profile(profile_id, patch_id, seed_target);
            self.save_profile(&seeded).await?;
            seeded
        };
        if !settings.is_default(&profile.id) {
            self.write_setting(DEFAULT_PROFILE_SETTING_KEY, profile.id.as_str())
                .await?;
        }
        Ok(profile)
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

// endregion: --- Tests
