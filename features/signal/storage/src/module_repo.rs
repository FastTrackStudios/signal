//! Module repository — data access for module collections and variants.

use std::collections::HashMap;

use sea_orm::sea_query::Index;
use sea_orm::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ColIdx, ColumnTrait, EntityTrait, Iden,
    IdenStatic, Iterable, QueryFilter, QueryOrder, QueryTrait, Set, StatementBuilder,
};
use sea_orm::{ConnectionTrait, Schema};
use signal_proto::{
    Module, ModulePreset, ModulePresetId, ModuleSnapshot, ModuleSnapshotId, metadata::Metadata,
};

use crate::entity;
use crate::{Database, DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait ModuleRepo: Send + Sync + 'static {
    async fn list_module_collections(&self) -> StorageResult<Vec<ModulePreset>>;
    async fn load_module_default_variant(
        &self,
        collection_id: &ModulePresetId,
    ) -> StorageResult<Option<ModuleSnapshot>>;
    async fn load_module_variant(
        &self,
        collection_id: &ModulePresetId,
        variant_id: &ModuleSnapshotId,
    ) -> StorageResult<Option<ModuleSnapshot>>;
    async fn save_module_collection(&self, preset: ModulePreset) -> StorageResult<()>;
    async fn delete_module_collection(&self, id: &ModulePresetId) -> StorageResult<()>;
}

// endregion: --- Trait

// region: --- ModuleRepoLive

#[derive(Clone)]
pub struct ModuleRepoLive {
    db: DatabaseConnection,
}

impl ModuleRepoLive {
    #[must_use]
    pub const fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Connect to a `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database connection fails.
    pub async fn connect_sqlite(url: &str) -> StorageResult<Self> {
        let db = Database::connect(url).await?;
        Ok(Self::new(db))
    }

    /// Connect to an in-memory `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database connection fails.
    pub async fn connect_sqlite_in_memory() -> StorageResult<Self> {
        Self::connect_sqlite("sqlite::memory:").await
    }

    /// Initialize the database schema.
    ///
    /// # Errors
    ///
    /// Returns an error if database operations fail.
    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut module_presets = schema.create_table_from_entity(entity::module_preset::Entity);
        module_presets.if_not_exists();
        self.db.execute(backend.build(&module_presets)).await?;

        let mut module_snapshots = schema.create_table_from_entity(entity::module_snapshot::Entity);
        module_snapshots.if_not_exists();
        self.db.execute(backend.build(&module_snapshots)).await?;
        self.db
            .execute(
                backend.build(
                    Index::create()
                        .name("idx_module_snapshots_preset_id_id")
                        .table(entity::module_snapshot::Entity)
                        .col(entity::module_snapshot::Column::ModulePresetId)
                        .col(entity::module_snapshot::Column::Id)
                        .if_not_exists(),
                ),
            )
            .await?;

        // Add version column if missing
        self.db
            .execute_unprepared(
                "ALTER TABLE module_snapshots ADD COLUMN version INTEGER NOT NULL DEFAULT 1",
            )
            .await
            .ok();

        // Add module_type column if missing
        self.db
            .execute_unprepared(
                "ALTER TABLE module_presets ADD COLUMN module_type TEXT NOT NULL DEFAULT 'custom'",
            )
            .await
            .ok();
        self.db
            .execute_unprepared(
                "ALTER TABLE module_presets ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
            )
            .await
            .ok();
        self.db
            .execute_unprepared(
                "ALTER TABLE module_snapshots ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
            )
            .await
            .ok();

        Ok(())
    }

    /// Reseed the database with default module collections.
    ///
    /// # Errors
    ///
    /// Returns an error if database operations fail.
    pub async fn reseed_defaults(&self, module_collections: &[ModulePreset]) -> StorageResult<()> {
        entity::module_snapshot::Entity::delete_many()
            .exec(&self.db)
            .await?;
        entity::module_preset::Entity::delete_many()
            .exec(&self.db)
            .await?;

        for collection in module_collections {
            entity::module_preset::Entity::insert(entity::module_preset::ActiveModel {
                id: Set(collection.id().to_string()),
                name: Set(collection.name().to_string()),
                module_type: Set(collection.module_type().as_str().to_string()),
                default_snapshot_id: Set(collection.default_snapshot().id().to_string()),
                metadata_json: Set(Self::metadata_to_json(collection.metadata())?),
            })
            .exec(&self.db)
            .await?;

            for variant in collection.snapshots() {
                entity::module_snapshot::Entity::insert(entity::module_snapshot::ActiveModel {
                    id: Set(variant.id().to_string()),
                    module_preset_id: Set(collection.id().to_string()),
                    name: Set(variant.name().to_string()),
                    state_json: Set(Self::module_to_json(variant.module())?),
                    metadata_json: Set(Self::metadata_to_json(variant.metadata())?),
                    version: Set(variant.version() as i32),
                })
                .exec(&self.db)
                .await?;
            }
        }

        Ok(())
    }

    // region: --- JSON helpers

    fn module_to_json(module: &Module) -> StorageResult<String> {
        serde_json::to_string(module)
            .map_err(|e| StorageError::Data(format!("failed to serialize module state: {e}")))
    }

    fn module_from_json(state_json: &str) -> StorageResult<Module> {
        serde_json::from_str::<Module>(state_json)
            .map_err(|e| StorageError::Data(format!("failed to parse module state json: {e}")))
    }

    fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
        serde_json::to_string(metadata)
            .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
    }

    fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
    }

    // endregion: --- JSON helpers

    // region: --- Model converters

    fn module_snapshot_from_model(
        model: &entity::module_snapshot::Model,
    ) -> StorageResult<ModuleSnapshot> {
        Ok(ModuleSnapshot::with_version_and_metadata(
            model.snapshot_id_branded(),
            model.name.clone(),
            Self::module_from_json(&model.state_json)?,
            model.version as u32,
            Self::metadata_from_json(&model.metadata_json)?,
        ))
    }

    // endregion: --- Model converters

    // region: --- Shared query helpers

    /// Assemble a full `ModulePreset` (collection) from its entity model and variant snapshot models.
    fn assemble_module_collection(
        preset_model: &entity::module_preset::Model,
        snapshot_models: &[entity::module_snapshot::Model],
    ) -> StorageResult<ModulePreset> {
        let mut variants = Vec::with_capacity(snapshot_models.len());
        for model in snapshot_models {
            variants.push(Self::module_snapshot_from_model(model)?);
        }

        let default_variant_id = preset_model.default_snapshot_id_branded();
        let default_variant = variants
            .iter()
            .find(|s| s.id() == &default_variant_id)
            .cloned()
            .ok_or_else(|| {
                StorageError::Data(format!(
                    "module collection '{}' references missing default variant '{}'",
                    preset_model.id, preset_model.default_snapshot_id
                ))
            })?;

        let additional = variants
            .into_iter()
            .filter(|snapshot| snapshot.id() != &default_variant_id)
            .collect::<Vec<_>>();

        let preset = ModulePreset::new(
            preset_model.preset_id_branded(),
            preset_model.name.clone(),
            preset_model.module_type_branded(),
            default_variant,
            additional,
        )
        .with_metadata(Self::metadata_from_json(&preset_model.metadata_json)?);

        Ok(preset)
    }

    // endregion: --- Shared query helpers
}

// endregion: --- ModuleRepoLive

// region: --- Trait impl

#[async_trait::async_trait]
impl ModuleRepo for ModuleRepoLive {
    async fn list_module_collections(&self) -> StorageResult<Vec<ModulePreset>> {
        let preset_models: Vec<entity::module_preset::Model> =
            entity::module_preset::Entity::find()
                .order_by_asc(entity::module_preset::Column::Id)
                .all(&self.db)
                .await?;
        let preset_models: Vec<entity::module_preset::Model> = preset_models
            .into_iter()
            .filter(|p| !p.name.starts_with("__phantom__"))
            .collect();

        if preset_models.is_empty() {
            return Ok(Vec::new());
        }

        let preset_ids: Vec<String> = preset_models.iter().map(|p| p.id.clone()).collect();
        let snapshot_models: Vec<entity::module_snapshot::Model> =
            entity::module_snapshot::Entity::find()
                .filter(entity::module_snapshot::Column::ModulePresetId.is_in(preset_ids))
                .order_by_asc(entity::module_snapshot::Column::ModulePresetId)
                .order_by_asc(entity::module_snapshot::Column::Id)
                .all(&self.db)
                .await?;
        let mut snapshots_by_preset: HashMap<String, Vec<entity::module_snapshot::Model>> =
            HashMap::new();
        for snapshot_model in snapshot_models {
            snapshots_by_preset
                .entry(snapshot_model.module_preset_id.clone())
                .or_default()
                .push(snapshot_model);
        }

        let mut out = Vec::with_capacity(preset_models.len());
        for preset_model in &preset_models {
            let snapshot_models = snapshots_by_preset
                .remove(&preset_model.id)
                .unwrap_or_default();
            out.push(Self::assemble_module_collection(
                preset_model,
                &snapshot_models,
            )?);
        }

        Ok(out)
    }

    async fn load_module_default_variant(
        &self,
        collection_id: &ModulePresetId,
    ) -> StorageResult<Option<ModuleSnapshot>> {
        let preset = entity::module_preset::Entity::find_by_id(collection_id.to_string())
            .one(&self.db)
            .await?;

        let Some(preset) = preset else {
            return Ok(None);
        };

        self.load_module_variant(
            collection_id,
            &ModuleSnapshotId::from(preset.default_snapshot_id),
        )
        .await
    }

    async fn load_module_variant(
        &self,
        collection_id: &ModulePresetId,
        variant_id: &ModuleSnapshotId,
    ) -> StorageResult<Option<ModuleSnapshot>> {
        let preset = entity::module_preset::Entity::find_by_id(collection_id.to_string())
            .one(&self.db)
            .await?;
        if preset.is_none() {
            return Ok(None);
        }

        let model = entity::module_snapshot::Entity::find_by_id(variant_id.to_string())
            .filter(entity::module_snapshot::Column::ModulePresetId.eq(collection_id.to_string()))
            .one(&self.db)
            .await?;

        match model {
            Some(model) => Ok(Some(Self::module_snapshot_from_model(&model)?)),
            None => Ok(None),
        }
    }

    async fn save_module_collection(&self, preset: ModulePreset) -> StorageResult<()> {
        let preset_id = preset.id().to_string();

        // Delete existing snapshots for this preset
        entity::module_snapshot::Entity::delete_many()
            .filter(entity::module_snapshot::Column::ModulePresetId.eq(preset_id.clone()))
            .exec(&self.db)
            .await?;

        // Delete existing preset row
        entity::module_preset::Entity::delete_by_id(preset_id.clone())
            .exec(&self.db)
            .await?;

        // Insert the preset row
        entity::module_preset::Entity::insert(entity::module_preset::ActiveModel {
            id: Set(preset_id.clone()),
            name: Set(preset.name().to_string()),
            module_type: Set(preset.module_type().as_str().to_string()),
            default_snapshot_id: Set(preset.default_snapshot().id().to_string()),
            metadata_json: Set(Self::metadata_to_json(preset.metadata())?),
        })
        .exec(&self.db)
        .await?;

        // Insert all snapshots
        for variant in preset.snapshots() {
            entity::module_snapshot::Entity::insert(entity::module_snapshot::ActiveModel {
                id: Set(variant.id().to_string()),
                module_preset_id: Set(preset_id.clone()),
                name: Set(variant.name().to_string()),
                state_json: Set(Self::module_to_json(variant.module())?),
                metadata_json: Set(Self::metadata_to_json(variant.metadata())?),
                version: Set(variant.version() as i32),
            })
            .exec(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn delete_module_collection(&self, id: &ModulePresetId) -> StorageResult<()> {
        let preset_id = id.to_string();

        entity::module_snapshot::Entity::delete_many()
            .filter(entity::module_snapshot::Column::ModulePresetId.eq(preset_id.clone()))
            .exec(&self.db)
            .await?;

        entity::module_preset::Entity::delete_by_id(preset_id)
            .exec(&self.db)
            .await?;

        Ok(())
    }
}

// endregion: --- Trait impl

// region: --- Tests

// endregion: --- Tests
