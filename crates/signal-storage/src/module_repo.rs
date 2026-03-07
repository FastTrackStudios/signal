//! Module repository — data access for module collections and variants.

use std::collections::HashMap;

use sea_orm::sea_query::Index;
use sea_orm::*;
use sea_orm::{ConnectionTrait, Schema};
use signal_proto::{
    metadata::Metadata, Module, ModulePreset, ModulePresetId, ModuleSnapshot, ModuleSnapshotId,
};

use crate::entity;
use crate::{Database, DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait ModuleRepo: Send + Sync {
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

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::{
        metadata::Metadata, seed_id, Block, BlockParameter, BlockParameterOverride, BlockType,
        ModuleBlock, ModuleBlockSource, ModuleType, PresetId, SignalChain, SignalNode,
    };

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    /// Build a test "drive stack" module preset with 4 blocks and 2 snapshots.
    fn test_drive_stack_preset() -> ModulePreset {
        let default_module = Module::from_blocks(vec![
            ModuleBlock::new(
                "boost",
                "Boost",
                BlockType::Drive,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from_uuid(seed_id("test-boost")),
                    saved_at_version: None,
                },
            ),
            ModuleBlock::new(
                "drive-1",
                "TS808",
                BlockType::Drive,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from_uuid(seed_id("test-ts808")),
                    saved_at_version: None,
                },
            ),
            ModuleBlock::new(
                "drive-2",
                "Klon",
                BlockType::Drive,
                ModuleBlockSource::PresetSnapshot {
                    preset_id: PresetId::from_uuid(seed_id("test-klon")),
                    snapshot_id: signal_proto::SnapshotId::from_uuid(seed_id("test-klon-bright")),
                    saved_at_version: None,
                },
            )
            .with_overrides(vec![
                BlockParameterOverride::new("treble", 0.55),
                BlockParameterOverride::new("output", 0.65),
            ]),
            ModuleBlock::new(
                "drive-3",
                "OCD",
                BlockType::Drive,
                ModuleBlockSource::PresetSnapshot {
                    preset_id: PresetId::from_uuid(seed_id("test-ocd")),
                    snapshot_id: signal_proto::SnapshotId::from_uuid(seed_id("test-ocd-hot")),
                    saved_at_version: None,
                },
            ),
        ]);

        let push_module = Module::from_blocks(vec![
            ModuleBlock::new(
                "boost",
                "Boost",
                BlockType::Drive,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from_uuid(seed_id("test-boost")),
                    saved_at_version: None,
                },
            ),
            ModuleBlock::new(
                "drive-1",
                "TS808",
                BlockType::Drive,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from_uuid(seed_id("test-ts808")),
                    saved_at_version: None,
                },
            ),
            ModuleBlock::new(
                "drive-2",
                "Klon",
                BlockType::Drive,
                ModuleBlockSource::PresetSnapshot {
                    preset_id: PresetId::from_uuid(seed_id("test-klon")),
                    snapshot_id: signal_proto::SnapshotId::from_uuid(seed_id("test-klon-bright")),
                    saved_at_version: None,
                },
            ),
            ModuleBlock::new(
                "drive-3",
                "OCD",
                BlockType::Drive,
                ModuleBlockSource::PresetSnapshot {
                    preset_id: PresetId::from_uuid(seed_id("test-ocd")),
                    snapshot_id: signal_proto::SnapshotId::from_uuid(seed_id("test-ocd-hot")),
                    saved_at_version: None,
                },
            ),
        ]);

        ModulePreset::new(
            seed_id("test-drive-stack"),
            "Test Drive Stack",
            ModuleType::Custom,
            ModuleSnapshot::new(seed_id("test-drive-stack-default"), "Default", default_module),
            vec![ModuleSnapshot::new(
                seed_id("test-drive-stack-push"),
                "Push",
                push_module,
            )],
        )
    }

    /// Build a test "parallel time" module preset with splits.
    fn test_parallel_time_preset() -> ModulePreset {
        let module = Module::from_chain(SignalChain::new(vec![
            SignalNode::Split {
                lanes: vec![
                    SignalChain::serial(vec![ModuleBlock::new(
                        "dly-1",
                        "Delay A",
                        BlockType::Delay,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from_uuid(seed_id("test-delay-a")),
                            saved_at_version: None,
                        },
                    )]),
                    SignalChain::serial(vec![ModuleBlock::new(
                        "dly-2",
                        "Delay B",
                        BlockType::Delay,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from_uuid(seed_id("test-delay-b")),
                            saved_at_version: None,
                        },
                    )]),
                ],
            },
            SignalNode::Split {
                lanes: vec![
                    SignalChain::serial(vec![ModuleBlock::new(
                        "verb-1",
                        "Reverb A",
                        BlockType::Reverb,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from_uuid(seed_id("test-reverb-a")),
                            saved_at_version: None,
                        },
                    )]),
                    SignalChain::serial(vec![ModuleBlock::new(
                        "verb-2",
                        "Reverb B",
                        BlockType::Reverb,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from_uuid(seed_id("test-reverb-b")),
                            saved_at_version: None,
                        },
                    )]),
                ],
            },
        ]));

        ModulePreset::new(
            seed_id("test-time-parallel"),
            "Test Parallel Time",
            ModuleType::Time,
            ModuleSnapshot::new(seed_id("test-time-parallel-default"), "Default", module),
            vec![],
        )
    }

    async fn seeded_repo() -> Result<ModuleRepoLive> {
        let repo = ModuleRepoLive::connect_sqlite_in_memory().await?;
        repo.init_schema().await?;
        repo.reseed_defaults(&[test_drive_stack_preset(), test_parallel_time_preset()])
            .await?;
        Ok(repo)
    }

    // -- Module collection listing

    #[tokio::test]
    async fn module_collection_contains_all_variants() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;

        // -- Exec
        let collections = repo.list_module_collections().await?;
        let stack = collections
            .iter()
            .find(|c| c.name() == "Test Drive Stack")
            .unwrap();

        // -- Check: default + 1 additional = 2 total
        assert_eq!(stack.snapshots().len(), 2);
        assert_eq!(stack.default_snapshot().name(), "Default");
        Ok(())
    }

    // -- Module variant loading

    #[tokio::test]
    async fn load_module_default_variant_returns_snapshot() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = ModulePresetId::from_uuid(seed_id("test-drive-stack"));

        // -- Exec
        let variant = repo.load_module_default_variant(&collection_id).await?;

        // -- Check
        let variant = variant.expect("should find default variant");
        assert_eq!(variant.name(), "Default");
        assert_eq!(variant.module().blocks().len(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn load_module_variant_by_id() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = ModulePresetId::from_uuid(seed_id("test-drive-stack"));
        let variant_id = ModuleSnapshotId::from_uuid(seed_id("test-drive-stack-push"));

        // -- Exec
        let variant = repo
            .load_module_variant(&collection_id, &variant_id)
            .await?;

        // -- Check
        let variant = variant.expect("should find variant");
        assert_eq!(variant.name(), "Push");
        assert_eq!(variant.module().blocks().len(), 4);
        Ok(())
    }

    #[tokio::test]
    async fn load_module_variant_missing_collection_returns_none() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = ModulePresetId::new();

        // -- Exec
        let variant = repo.load_module_default_variant(&collection_id).await?;

        // -- Check
        assert!(variant.is_none());
        Ok(())
    }

    // -- Override round-trip

    #[tokio::test]
    async fn module_override_round_trip() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = ModulePresetId::from_uuid(seed_id("test-drive-stack"));

        // -- Exec
        let variant = repo
            .load_module_default_variant(&collection_id)
            .await?
            .expect("should find default variant");

        // -- Check: drive-2 (Klon) has overrides
        let blocks = variant.module().blocks();
        let drive_2 = blocks.iter().find(|b| b.id() == "drive-2").unwrap();
        let overrides = drive_2.overrides();
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0].parameter_id(), "treble");
        assert!((overrides[0].value().get() - 0.55).abs() < 0.001);
        assert_eq!(overrides[1].parameter_id(), "output");
        assert!((overrides[1].value().get() - 0.65).abs() < 0.001);

        // Check: drive-3 (OCD) is a preset snapshot with no overrides
        let drive_3 = blocks.iter().find(|b| b.id() == "drive-3").unwrap();
        assert!(drive_3.overrides().is_empty());
        assert!(matches!(
            drive_3.source(),
            ModuleBlockSource::PresetSnapshot { .. }
        ));
        Ok(())
    }

    // -- Default normalization

    #[tokio::test]
    async fn module_default_normalization() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;

        // -- Exec
        let collections = repo.list_module_collections().await?;

        // -- Check
        for collection in &collections {
            let default_id = collection.default_snapshot().id().clone();
            assert!(
                collection.snapshots().iter().any(|s| *s.id() == default_id),
                "default variant '{}' not found in module collection '{}'",
                default_id,
                collection.name()
            );
            assert_eq!(collection.snapshots()[0].id(), &default_id);
        }
        Ok(())
    }

    // -- Module block source round-trip

    #[tokio::test]
    async fn module_block_source_types_round_trip() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;
        let collection_id = ModulePresetId::from_uuid(seed_id("test-drive-stack"));

        // -- Exec
        let variant = repo
            .load_module_default_variant(&collection_id)
            .await?
            .expect("should find default variant");

        // -- Check: each slot has the correct source type
        // boost (PresetDefault), drive-1 (PresetDefault), drive-2 (PresetSnapshot), drive-3 (PresetSnapshot)
        let blocks = variant.module().blocks();
        assert!(matches!(
            blocks[0].source(),
            ModuleBlockSource::PresetDefault { .. }
        ));
        assert!(matches!(
            blocks[1].source(),
            ModuleBlockSource::PresetDefault { .. }
        ));
        assert!(matches!(
            blocks[2].source(),
            ModuleBlockSource::PresetSnapshot { .. }
        ));
        assert!(matches!(
            blocks[3].source(),
            ModuleBlockSource::PresetSnapshot { .. }
        ));
        Ok(())
    }

    // -- Snapshot version round-trip

    #[tokio::test]
    async fn module_snapshot_version_round_trips_through_db() -> Result<()> {
        // -- Setup & Fixtures
        let repo = seeded_repo().await?;

        // -- Exec
        let collections = repo.list_module_collections().await?;

        // -- Check: all module snapshots should be at version 1
        for collection in &collections {
            for snap in collection.snapshots() {
                assert_eq!(
                    snap.version(),
                    1,
                    "module snapshot '{}' should be at version 1",
                    snap.name()
                );
            }
        }
        Ok(())
    }

    // -- Override on block inside parallel split survives DB round-trip

    #[tokio::test]
    async fn parallel_block_override_round_trip() -> Result<()> {
        // -- Setup & Fixtures: module with overrides on blocks inside a split
        let repo = ModuleRepoLive::connect_sqlite_in_memory().await?;
        repo.init_schema().await?;

        let module = Module::from_chain(SignalChain::new(vec![
            SignalNode::Block(ModuleBlock::new(
                "pre-eq",
                "Pre EQ",
                BlockType::Eq,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from_uuid(seed_id("eq-reaeq")),
                    saved_at_version: None,
                },
            )),
            SignalNode::Split {
                lanes: vec![
                    SignalChain::serial(vec![ModuleBlock::new(
                        "delay",
                        "Delay",
                        BlockType::Delay,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from_uuid(seed_id("delay-timeline")),
                            saved_at_version: None,
                        },
                    )
                    .with_overrides(vec![
                        BlockParameterOverride::new("time", 0.65),
                        BlockParameterOverride::new("feedback", 0.40),
                    ])]),
                    SignalChain::serial(vec![ModuleBlock::new(
                        "reverb",
                        "Reverb",
                        BlockType::Reverb,
                        ModuleBlockSource::PresetDefault {
                            preset_id: PresetId::from_uuid(seed_id("reverb-bigsky")),
                            saved_at_version: None,
                        },
                    )
                    .with_overrides(vec![BlockParameterOverride::new("decay", 0.80)])]),
                ],
            },
            SignalNode::Block(ModuleBlock::new(
                "post-vol",
                "Post Volume",
                BlockType::Volume,
                ModuleBlockSource::Inline {
                    block: Block::from_parameters(vec![BlockParameter::new(
                        "level", "Level", 0.70,
                    )]),
                },
            )),
        ]));

        let collection = ModulePreset::new(
            seed_id("test-parallel-overrides"),
            "Parallel Overrides Test",
            ModuleType::Custom,
            ModuleSnapshot::new(seed_id("par-ov-default"), "Default", module),
            vec![],
        );

        repo.reseed_defaults(&[collection]).await?;

        // -- Exec: load it back from DB
        let loaded = repo
            .load_module_default_variant(&ModulePresetId::from_uuid(seed_id(
                "test-parallel-overrides",
            )))
            .await?
            .expect("should find variant");

        // -- Check: topology preserved
        let chain = loaded.module().chain();
        assert!(!chain.is_serial()); // has a split
        assert_eq!(chain.len(), 3); // pre-eq, split, post-vol

        // -- Check: overrides on blocks inside the split survived
        let blocks = loaded.module().blocks();
        assert_eq!(blocks.len(), 4); // pre-eq, delay, reverb, post-vol

        let delay = blocks.iter().find(|b| b.id() == "delay").unwrap();
        assert_eq!(delay.overrides().len(), 2);
        assert_eq!(delay.overrides()[0].parameter_id(), "time");
        assert!((delay.overrides()[0].value().get() - 0.65).abs() < 0.001);
        assert_eq!(delay.overrides()[1].parameter_id(), "feedback");
        assert!((delay.overrides()[1].value().get() - 0.40).abs() < 0.001);

        let reverb = blocks.iter().find(|b| b.id() == "reverb").unwrap();
        assert_eq!(reverb.overrides().len(), 1);
        assert_eq!(reverb.overrides()[0].parameter_id(), "decay");
        assert!((reverb.overrides()[0].value().get() - 0.80).abs() < 0.001);

        // pre-eq and post-vol have no overrides
        let pre_eq = blocks.iter().find(|b| b.id() == "pre-eq").unwrap();
        assert!(pre_eq.overrides().is_empty());
        let post_vol = blocks.iter().find(|b| b.id() == "post-vol").unwrap();
        assert!(post_vol.overrides().is_empty());
        Ok(())
    }

    // -- Replace a block in a module's signal chain

    #[tokio::test]
    async fn replace_block_in_parallel_module() -> Result<()> {
        // -- Setup & Fixtures: load the "Parallel Time" default variant
        let repo = seeded_repo().await?;
        let original = repo
            .load_module_default_variant(&ModulePresetId::from_uuid(seed_id(
                "test-time-parallel",
            )))
            .await?
            .expect("should find variant");

        // Verify original has dly-1 in the first split
        let orig_blocks = original.module().blocks();
        assert!(orig_blocks.iter().any(|b| b.id() == "dly-1"));
        assert!(!orig_blocks.iter().any(|b| b.id() == "chorus-new"));

        // -- Exec: clone the module, find dly-1 in split 0, replace it with a chorus
        let mut chain = original.module().chain().clone();
        // The delay split is at index 0 in the top-level nodes
        let split_node = &mut chain.nodes_mut()[0];
        if let SignalNode::Split { lanes } = split_node {
            // Lane 0 has dly-1 — replace it
            lanes[0] = SignalChain::serial(vec![ModuleBlock::new(
                "chorus-new",
                "Chorus",
                BlockType::Chorus,
                ModuleBlockSource::PresetDefault {
                    preset_id: PresetId::from_uuid(seed_id("chorus-js")),
                    saved_at_version: None,
                },
            )]);
        } else {
            panic!("expected split at index 0");
        }

        // Build a new module collection with the modified chain
        let modified_module = Module::from_chain(chain);
        let modified_snapshot = ModuleSnapshot::new(
            seed_id("time-parallel-chorus"),
            "Chorus Variant",
            modified_module,
        );
        let collection = ModulePreset::new(
            seed_id("time-parallel-modified"),
            "Modified Parallel Time",
            ModuleType::Time,
            modified_snapshot,
            vec![],
        );
        repo.reseed_defaults(&[collection]).await?;

        // -- Exec: load it back
        let loaded = repo
            .load_module_default_variant(&ModulePresetId::from_uuid(seed_id(
                "time-parallel-modified",
            )))
            .await?
            .expect("should find modified variant");

        // -- Check: dly-1 is gone, chorus-new is in its place
        let loaded_blocks = loaded.module().blocks();
        assert!(!loaded_blocks.iter().any(|b| b.id() == "dly-1"));
        assert!(loaded_blocks.iter().any(|b| b.id() == "chorus-new"));

        let chorus = loaded_blocks
            .iter()
            .find(|b| b.id() == "chorus-new")
            .unwrap();
        assert_eq!(chorus.block_type(), BlockType::Chorus);

        // dly-2 is still in the other lane of the delay split
        assert!(loaded_blocks.iter().any(|b| b.id() == "dly-2"));
        Ok(())
    }

    #[tokio::test]
    async fn module_collection_and_variant_metadata_round_trip() -> Result<()> {
        let repo = ModuleRepoLive::connect_sqlite_in_memory().await?;
        repo.init_schema().await?;

        let module = Module::from_blocks(vec![ModuleBlock::new(
            "eq",
            "EQ",
            BlockType::Eq,
            ModuleBlockSource::PresetDefault {
                preset_id: PresetId::from_uuid(seed_id("eq-reaeq")),
                saved_at_version: None,
            },
        )]);
        let variant = ModuleSnapshot::new(seed_id("meta-mod-snap"), "Meta", module).with_metadata(
            Metadata::new()
                .with_tag("module-snapshot")
                .with_notes("snapshot"),
        );
        let collection = ModulePreset::new(
            seed_id("meta-mod-preset"),
            "MetaMod",
            ModuleType::Custom,
            variant,
            vec![],
        )
        .with_metadata(
            Metadata::new()
                .with_tag("module-preset")
                .with_description("desc"),
        );

        repo.reseed_defaults(&[collection]).await?;
        let loaded = repo
            .list_module_collections()
            .await?
            .into_iter()
            .find(|m| m.id().as_str() == seed_id("meta-mod-preset").to_string())
            .expect("meta module preset exists");

        assert!(loaded.metadata().tags.contains("module-preset"));
        assert_eq!(loaded.metadata().description.as_deref(), Some("desc"));
        assert!(loaded
            .default_snapshot()
            .metadata()
            .tags
            .contains("module-snapshot"));
        assert_eq!(
            loaded.default_snapshot().metadata().notes.as_deref(),
            Some("snapshot")
        );
        Ok(())
    }
}

// endregion: --- Tests
