//! Block repository — data access for block state, collections, and variants.

use std::collections::HashMap;

use sea_orm::sea_query::Index;
use sea_orm::*;
use sea_orm::{ConnectionTrait, Schema};
use signal_proto::{
    metadata::Metadata, Block, BlockType, Preset, PresetId, Snapshot, SnapshotId, ALL_BLOCK_TYPES,
};

use crate::entity;
use crate::{Database, DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait BlockRepo: Send + Sync {
    async fn load_block_state(&self, block_type: BlockType) -> StorageResult<Option<Block>>;
    async fn save_block_state(&self, block_type: BlockType, block: Block) -> StorageResult<()>;
    async fn list_block_collections(&self, block_type: BlockType) -> StorageResult<Vec<Preset>>;
    async fn load_block_default_variant(
        &self,
        block_type: BlockType,
        collection_id: &PresetId,
    ) -> StorageResult<Option<Snapshot>>;
    async fn load_block_variant(
        &self,
        block_type: BlockType,
        collection_id: &PresetId,
        variant_id: &SnapshotId,
    ) -> StorageResult<Option<Snapshot>>;
    async fn save_block_collection(&self, preset: Preset) -> StorageResult<()>;
    async fn delete_block_collection(&self, id: &PresetId) -> StorageResult<()>;
}

// endregion: --- Trait

// region: --- BlockRepoLive

#[derive(Clone)]
pub struct BlockRepoLive {
    db: DatabaseConnection,
}

impl BlockRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn connect_sqlite(url: &str) -> StorageResult<Self> {
        let db = Database::connect(url).await?;
        Ok(Self::new(db))
    }

    pub async fn connect_sqlite_in_memory() -> StorageResult<Self> {
        Self::connect_sqlite("sqlite::memory:").await
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut presets = schema.create_table_from_entity(entity::preset::Entity);
        presets.if_not_exists();
        self.db.execute(backend.build(&presets)).await?;

        let mut snapshots = schema.create_table_from_entity(entity::snapshot::Entity);
        snapshots.if_not_exists();
        self.db.execute(backend.build(&snapshots)).await?;
        self.db
            .execute(
                backend.build(
                    Index::create()
                        .name("idx_snapshots_preset_id_id")
                        .table(entity::snapshot::Entity)
                        .col(entity::snapshot::Column::PresetId)
                        .col(entity::snapshot::Column::Id)
                        .if_not_exists(),
                ),
            )
            .await?;

        let mut current_block = schema.create_table_from_entity(entity::current_block::Entity);
        current_block.if_not_exists();
        self.db.execute(backend.build(&current_block)).await?;

        // Add version column if missing (handles existing DBs created before versioning).
        self.db
            .execute_unprepared(
                "ALTER TABLE snapshots ADD COLUMN version INTEGER NOT NULL DEFAULT 1",
            )
            .await
            .ok();
        self.db
            .execute_unprepared(
                "ALTER TABLE presets ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
            )
            .await
            .ok();
        self.db
            .execute_unprepared(
                "ALTER TABLE snapshots ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
            )
            .await
            .ok();

        Ok(())
    }

    pub async fn reseed_defaults(&self, block_collections: &[Preset]) -> StorageResult<()> {
        entity::snapshot::Entity::delete_many()
            .exec(&self.db)
            .await?;
        entity::preset::Entity::delete_many().exec(&self.db).await?;
        entity::current_block::Entity::delete_many()
            .exec(&self.db)
            .await?;

        for collection in block_collections {
            entity::preset::Entity::insert(entity::preset::ActiveModel {
                id: Set(collection.id().to_string()),
                block_type: Set(collection.block_type().as_str().to_string()),
                name: Set(collection.name().to_string()),
                default_snapshot_id: Set(collection.default_snapshot().id().to_string()),
                metadata_json: Set(metadata_to_json(collection.metadata())?),
            })
            .exec(&self.db)
            .await?;

            for variant in collection.snapshots() {
                entity::snapshot::Entity::insert(entity::snapshot::ActiveModel {
                    id: Set(variant.id().to_string()),
                    preset_id: Set(collection.id().to_string()),
                    name: Set(variant.name().to_string()),
                    state_json: Set(block_to_json(&variant.block())?),
                    metadata_json: Set(metadata_to_json(variant.metadata())?),
                    version: Set(variant.version() as i32),
                    state_data_b64: Set(state_data_to_b64(variant.state_data())),
                })
                .exec(&self.db)
                .await?;
            }
        }

        for &block_type in ALL_BLOCK_TYPES {
            let block = block_collections
                .iter()
                .find(|c| c.block_type() == block_type)
                .map(|c| c.default_snapshot().block())
                .unwrap_or_default();
            self.save_block_state(block_type, block).await?;
        }

        Ok(())
    }
}

// endregion: --- BlockRepoLive

// region: --- Private helpers

fn block_to_json(block: &Block) -> StorageResult<String> {
    serde_json::to_string(block)
        .map_err(|e| StorageError::Data(format!("failed to serialize block state: {e}")))
}

fn block_from_json(state_json: &str) -> StorageResult<Block> {
    serde_json::from_str::<Block>(state_json)
        .map_err(|e| StorageError::Data(format!("failed to parse block state json: {e}")))
}

fn snapshot_from_model(model: &entity::snapshot::Model) -> StorageResult<Snapshot> {
    use base64::Engine as _;

    let mut snapshot = Snapshot::with_version_and_metadata(
        model.snapshot_id_branded(),
        model.name.clone(),
        block_from_json(&model.state_json)?,
        model.version as u32,
        metadata_from_json(&model.metadata_json)?,
    );

    if let Some(b64) = &model.state_data_b64 {
        if let Ok(data) = base64::engine::general_purpose::STANDARD.decode(b64) {
            snapshot = snapshot.with_state_data(data);
        }
    }

    Ok(snapshot)
}

fn state_data_to_b64(data: Option<&[u8]>) -> Option<String> {
    use base64::Engine as _;
    data.map(|d| base64::engine::general_purpose::STANDARD.encode(d))
}

fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
    serde_json::to_string(metadata)
        .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
}

fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
    serde_json::from_str(json)
        .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
}

// endregion: --- Private helpers

// region: --- Shared query helpers

impl BlockRepoLive {
    /// Assemble a full `Preset` (collection) from its entity model and variant snapshot models.
    fn assemble_block_collection(
        preset_model: &entity::preset::Model,
        snapshot_models: &[entity::snapshot::Model],
        block_type: BlockType,
    ) -> StorageResult<Preset> {
        let mut variants = Vec::with_capacity(snapshot_models.len());
        for model in snapshot_models {
            variants.push(snapshot_from_model(model)?);
        }

        let default_variant_id = preset_model.default_snapshot_id_branded();
        let default_variant = variants
            .iter()
            .find(|s| s.id() == &default_variant_id)
            .cloned()
            .ok_or_else(|| {
                StorageError::Data(format!(
                    "collection '{}' references missing default variant '{}'",
                    preset_model.id, preset_model.default_snapshot_id
                ))
            })?;

        let additional = variants
            .into_iter()
            .filter(|s| s.id() != &default_variant_id)
            .collect::<Vec<_>>();

        let preset = Preset::new(
            preset_model.preset_id_branded(),
            preset_model.name.clone(),
            block_type,
            default_variant,
            additional,
        )
        .with_metadata(metadata_from_json(&preset_model.metadata_json)?);

        Ok(preset)
    }
}

// endregion: --- Shared query helpers

// region: --- Trait impl

#[async_trait::async_trait]
impl BlockRepo for BlockRepoLive {
    async fn load_block_state(&self, block_type: BlockType) -> StorageResult<Option<Block>> {
        let model = entity::current_block::Entity::find_by_id(block_type.as_str().to_string())
            .one(&self.db)
            .await?;

        match model {
            Some(model) => Ok(Some(block_from_json(&model.state_json)?)),
            None => Ok(None),
        }
    }

    async fn save_block_state(&self, block_type: BlockType, block: Block) -> StorageResult<()> {
        let existing = entity::current_block::Entity::find_by_id(block_type.as_str().to_string())
            .one(&self.db)
            .await?;
        let state_json = block_to_json(&block)?;

        if let Some(model) = existing {
            let mut active: entity::current_block::ActiveModel = model.into();
            active.state_json = Set(state_json);
            active.update(&self.db).await?;
        } else {
            entity::current_block::Entity::insert(entity::current_block::ActiveModel {
                block_type: Set(block_type.as_str().to_string()),
                state_json: Set(state_json),
            })
            .exec(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn list_block_collections(&self, block_type: BlockType) -> StorageResult<Vec<Preset>> {
        let preset_models: Vec<entity::preset::Model> = entity::preset::Entity::find()
            .filter(entity::preset::Column::BlockType.eq(block_type.as_str().to_string()))
            .order_by_asc(entity::preset::Column::Id)
            .all(&self.db)
            .await?;
        let preset_models: Vec<entity::preset::Model> = preset_models
            .into_iter()
            .filter(|p| !p.name.starts_with("__phantom__"))
            .collect();

        if preset_models.is_empty() {
            return Ok(Vec::new());
        }

        let preset_ids: Vec<String> = preset_models.iter().map(|p| p.id.clone()).collect();
        let snapshot_models: Vec<entity::snapshot::Model> = entity::snapshot::Entity::find()
            .filter(entity::snapshot::Column::PresetId.is_in(preset_ids))
            .order_by_asc(entity::snapshot::Column::PresetId)
            .order_by_asc(entity::snapshot::Column::Id)
            .all(&self.db)
            .await?;
        let mut snapshots_by_preset: HashMap<String, Vec<entity::snapshot::Model>> = HashMap::new();
        for snapshot_model in snapshot_models {
            snapshots_by_preset
                .entry(snapshot_model.preset_id.clone())
                .or_default()
                .push(snapshot_model);
        }

        let mut out = Vec::with_capacity(preset_models.len());
        for preset_model in &preset_models {
            let snapshot_models = snapshots_by_preset
                .remove(&preset_model.id)
                .unwrap_or_default();
            out.push(Self::assemble_block_collection(
                preset_model,
                &snapshot_models,
                block_type,
            )?);
        }

        Ok(out)
    }

    async fn load_block_default_variant(
        &self,
        block_type: BlockType,
        collection_id: &PresetId,
    ) -> StorageResult<Option<Snapshot>> {
        let preset = entity::preset::Entity::find_by_id(collection_id.to_string())
            .filter(entity::preset::Column::BlockType.eq(block_type.as_str().to_string()))
            .one(&self.db)
            .await?;

        let Some(preset) = preset else {
            return Ok(None);
        };

        self.load_block_variant(
            block_type,
            collection_id,
            &SnapshotId::from(preset.default_snapshot_id),
        )
        .await
    }

    async fn load_block_variant(
        &self,
        block_type: BlockType,
        collection_id: &PresetId,
        variant_id: &SnapshotId,
    ) -> StorageResult<Option<Snapshot>> {
        let preset = entity::preset::Entity::find_by_id(collection_id.to_string())
            .filter(entity::preset::Column::BlockType.eq(block_type.as_str().to_string()))
            .one(&self.db)
            .await?;
        if preset.is_none() {
            return Ok(None);
        }

        let model = entity::snapshot::Entity::find_by_id(variant_id.to_string())
            .filter(entity::snapshot::Column::PresetId.eq(collection_id.to_string()))
            .one(&self.db)
            .await?;

        match model {
            Some(model) => Ok(Some(snapshot_from_model(&model)?)),
            None => Ok(None),
        }
    }

    async fn save_block_collection(&self, preset: Preset) -> StorageResult<()> {
        let preset_id = preset.id().to_string();

        // Delete existing snapshots for this preset
        entity::snapshot::Entity::delete_many()
            .filter(entity::snapshot::Column::PresetId.eq(preset_id.clone()))
            .exec(&self.db)
            .await?;

        // Delete existing preset row
        entity::preset::Entity::delete_by_id(preset_id.clone())
            .exec(&self.db)
            .await?;

        // Insert the preset row
        entity::preset::Entity::insert(entity::preset::ActiveModel {
            id: Set(preset_id.clone()),
            block_type: Set(preset.block_type().as_str().to_string()),
            name: Set(preset.name().to_string()),
            default_snapshot_id: Set(preset.default_snapshot().id().to_string()),
            metadata_json: Set(metadata_to_json(preset.metadata())?),
        })
        .exec(&self.db)
        .await?;

        // Insert all snapshots
        for variant in preset.snapshots() {
            entity::snapshot::Entity::insert(entity::snapshot::ActiveModel {
                id: Set(variant.id().to_string()),
                preset_id: Set(preset_id.clone()),
                name: Set(variant.name().to_string()),
                state_json: Set(block_to_json(&variant.block())?),
                metadata_json: Set(metadata_to_json(variant.metadata())?),
                version: Set(variant.version() as i32),
                state_data_b64: Set(state_data_to_b64(variant.state_data())),
            })
            .exec(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn delete_block_collection(&self, id: &PresetId) -> StorageResult<()> {
        let preset_id = id.to_string();

        entity::snapshot::Entity::delete_many()
            .filter(entity::snapshot::Column::PresetId.eq(preset_id.clone()))
            .exec(&self.db)
            .await?;

        entity::preset::Entity::delete_by_id(preset_id)
            .exec(&self.db)
            .await?;

        Ok(())
    }
}

// endregion: --- Trait impl

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::{metadata::Metadata, seed_id, traits::Collection, BlockParameter};

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    async fn seeded_repo() -> Result<BlockRepoLive> {
        let repo = BlockRepoLive::connect_sqlite_in_memory().await?;
        repo.init_schema().await?;
        repo.reseed_defaults(&crate::seed_data::default_block_collections())
            .await?;
        Ok(repo)
    }

    // -- Block state round-trip

    #[tokio::test]
    async fn block_state_round_trip() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let block = Block::from_parameters(vec![
            BlockParameter::new("gain", "Gain", 0.77),
            BlockParameter::new("bass", "Bass", 0.33),
        ]);

        // -- Exec
        repo.save_block_state(BlockType::Amp, block.clone()).await?;
        let loaded = repo.load_block_state(BlockType::Amp).await?;

        // -- Check
        assert_eq!(loaded, Some(block));
        Ok(())
    }

    #[tokio::test]
    async fn block_state_missing_returns_none() -> Result<()> {
        // -- Setup & Fixtures
        let repo = BlockRepoLive::connect_sqlite_in_memory().await?;
        repo.init_schema().await?;

        // -- Exec
        let loaded = repo.load_block_state(BlockType::Amp).await?;

        // -- Check
        assert_eq!(loaded, None);
        Ok(())
    }

    // -- Block collection listing

    #[tokio::test]
    async fn block_collection_contains_all_variants() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;

        // -- Exec
        let collections = repo.list_block_collections(BlockType::Amp).await?;
        let twin = collections
            .iter()
            .find(|c| c.name() == "Fender Twin Reverb")
            .unwrap();

        // -- Check: default + 4 additional = 5 total
        assert_eq!(twin.snapshots().len(), 5);
        assert_eq!(twin.default_snapshot().name(), "Default");
        Ok(())
    }

    // -- Block variant loading

    #[tokio::test]
    async fn load_block_default_variant_returns_snapshot() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = PresetId::from_uuid(seed_id("amp-twin"));

        // -- Exec
        let variant = repo
            .load_block_default_variant(BlockType::Amp, &collection_id)
            .await?;

        // -- Check
        let variant = variant.expect("should find default variant");
        assert_eq!(variant.name(), "Default");
        assert_eq!(
            variant.id(),
            &SnapshotId::from_uuid(seed_id("amp-twin-default"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn load_block_variant_by_id() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = PresetId::from_uuid(seed_id("amp-twin"));
        let variant_id = SnapshotId::from_uuid(seed_id("amp-twin-surf"));

        // -- Exec
        let variant = repo
            .load_block_variant(BlockType::Amp, &collection_id, &variant_id)
            .await?;

        // -- Check
        let variant = variant.expect("should find variant");
        assert_eq!(variant.name(), "Surf");
        let block = variant.block();
        let params = block.parameters();
        let reverb = params.iter().find(|p| p.id() == "reverb").unwrap();
        assert!((reverb.value().get() - 0.75).abs() < 0.001);
        Ok(())
    }

    #[tokio::test]
    async fn load_block_variant_wrong_type_returns_none() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = PresetId::from_uuid(seed_id("amp-twin"));

        // -- Exec: amp-twin is Amp, not Drive
        let variant = repo
            .load_block_default_variant(BlockType::Drive, &collection_id)
            .await?;

        // -- Check
        assert!(variant.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_block_variant_missing_collection_returns_none() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = PresetId::new();

        // -- Exec
        let variant = repo
            .load_block_default_variant(BlockType::Amp, &collection_id)
            .await?;

        // -- Check
        assert!(variant.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_block_variant_missing_variant_returns_none() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = PresetId::from_uuid(seed_id("amp-twin"));
        let variant_id = SnapshotId::new();

        // -- Exec
        let variant = repo
            .load_block_variant(BlockType::Amp, &collection_id, &variant_id)
            .await?;

        // -- Check
        assert!(variant.is_none());
        Ok(())
    }

    // -- Metadata round-trip (verifies JSON serialization preserves all fields)

    #[tokio::test]
    async fn block_metadata_round_trip_preserves_parameter_names() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;

        // -- Exec
        let collections = repo.list_block_collections(BlockType::Drive).await?;
        let ts = collections
            .iter()
            .find(|c| c.name() == "Tubescreamer")
            .unwrap();
        let default = ts.default_snapshot();

        // -- Check: verify parameter metadata survived serialization
        let block = default.block();
        let params = block.parameters();
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].id(), "drive");
        assert_eq!(params[0].name(), "Drive");
        assert!((params[0].value().get() - 0.50).abs() < 0.001);
        assert_eq!(params[1].id(), "tone");
        assert_eq!(params[2].id(), "level");
        Ok(())
    }

    // -- Default normalization (verifies default variant is always the first in list)

    #[tokio::test]
    async fn default_normalization_first_variant_is_default() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;

        // -- Exec
        let collections = repo.list_block_collections(BlockType::Amp).await?;

        // -- Check: for every collection, default_snapshot_id points to a valid variant
        for collection in &collections {
            let default_id = collection.default_snapshot().id().clone();
            assert!(
                collection.snapshots().iter().any(|s| *s.id() == default_id),
                "default variant '{}' not found in collection '{}'",
                default_id,
                collection.name()
            );
            // The first snapshot is always the default
            assert_eq!(collection.snapshots()[0].id(), &default_id);
        }
        Ok(())
    }

    // -- Block state overwrite

    #[tokio::test]
    async fn save_block_state_overwrites_previous() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let block1 = Block::from_parameters(vec![BlockParameter::new("a", "A", 0.1)]);
        let block2 = Block::from_parameters(vec![BlockParameter::new("b", "B", 0.9)]);

        // -- Exec
        repo.save_block_state(BlockType::Amp, block1).await?;
        repo.save_block_state(BlockType::Amp, block2.clone())
            .await?;
        let loaded = repo.load_block_state(BlockType::Amp).await?;

        // -- Check
        assert_eq!(loaded, Some(block2));
        Ok(())
    }

    // -- Snapshot version round-trip

    #[tokio::test]
    async fn snapshot_version_round_trips_through_db() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;

        // -- Exec: load an amp collection and check version
        let collections = repo.list_block_collections(BlockType::Amp).await?;
        let twin = collections
            .iter()
            .find(|c| c.name() == "Fender Twin Reverb")
            .expect("should find Twin Reverb");

        // -- Check: seed data starts at version 1
        for snap in twin.snapshots() {
            assert_eq!(
                snap.version(),
                1,
                "seed snapshot '{}' should start at version 1",
                snap.name()
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn block_collection_and_variant_metadata_round_trip() -> Result<()> {
        let repo = BlockRepoLive::connect_sqlite_in_memory().await?;
        repo.init_schema().await?;

        let variant = Snapshot::new(
            seed_id("meta-snap"),
            "MetaSnap",
            Block::from_parameters(vec![BlockParameter::new("gain", "Gain", 0.5)]),
        )
        .with_metadata(
            Metadata::new()
                .with_tag("snapshot-tag")
                .with_notes("snapshot-notes"),
        );
        let preset = Preset::new(
            seed_id("meta-preset"),
            "MetaPreset",
            BlockType::Amp,
            variant,
            vec![],
        )
        .with_metadata(
            Metadata::new()
                .with_tag("preset-tag")
                .with_description("preset-desc"),
        );

        repo.reseed_defaults(&[preset]).await?;
        let amp = repo.list_block_collections(BlockType::Amp).await?;
        let loaded = amp
            .iter()
            .find(|p| p.id().as_str() == seed_id("meta-preset").to_string())
            .expect("meta preset exists");

        assert!(loaded.metadata().tags.contains("preset-tag"));
        assert_eq!(
            loaded.metadata().description.as_deref(),
            Some("preset-desc")
        );
        assert!(loaded
            .default_snapshot()
            .metadata()
            .tags
            .contains("snapshot-tag"));
        assert_eq!(
            loaded.default_snapshot().metadata().notes.as_deref(),
            Some("snapshot-notes")
        );
        Ok(())
    }

    // -- save_block_collection

    #[tokio::test]
    async fn save_block_collection_round_trip() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let preset_id = PresetId::from_uuid(seed_id("amp-twin"));

        // Load the preset, mutate a snapshot param, save it back
        let collections = repo.list_block_collections(BlockType::Amp).await?;
        let mut preset = collections
            .into_iter()
            .find(|c| *c.id() == preset_id)
            .expect("twin preset");
        let snap = &mut preset.variants_mut()[0];
        let mut block = snap.block();
        block.set_parameter_value(0, 0.99);
        snap.set_block(block);
        snap.increment_version();

        repo.save_block_collection(preset.clone()).await?;

        // Reload and verify
        let reloaded = repo.list_block_collections(BlockType::Amp).await?;
        let twin = reloaded
            .iter()
            .find(|c| *c.id() == preset_id)
            .expect("twin after save");
        assert_eq!(twin.snapshots().len(), preset.snapshots().len());
        let first_param = twin.snapshots()[0].block().parameters()[0].value().get();
        assert!((first_param - 0.99).abs() < 0.001);
        assert_eq!(twin.snapshots()[0].version(), 2);
        Ok(())
    }
}

// endregion: --- Tests
