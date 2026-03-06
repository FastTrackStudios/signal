//! Layer repository — data access for Layer collections and LayerSnapshot variants.

use sea_orm::*;
use signal_proto::layer::{
    BlockRef, Layer, LayerId, LayerRef, LayerSnapshot, LayerSnapshotId, ModuleRef, PluginRef,
};
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::EngineType;

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

/// Data-access trait for Layer collections.
#[async_trait::async_trait]
pub trait LayerRepo: Send + Sync {
    async fn list_layers(&self) -> StorageResult<Vec<Layer>>;
    async fn load_layer(&self, id: &LayerId) -> StorageResult<Option<Layer>>;
    async fn save_layer(&self, layer: &Layer) -> StorageResult<()>;
    async fn delete_layer(&self, id: &LayerId) -> StorageResult<()>;
    async fn load_variant(
        &self,
        layer_id: &LayerId,
        variant_id: &LayerSnapshotId,
    ) -> StorageResult<Option<LayerSnapshot>>;
}

// endregion: --- Trait

// region: --- LayerRepoLive

#[derive(Clone)]
pub struct LayerRepoLive {
    db: DatabaseConnection,
}

impl LayerRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut layers = schema.create_table_from_entity(entity::layer::Entity);
        layers.if_not_exists();
        self.db.execute(backend.build(&layers)).await?;

        let mut variants = schema.create_table_from_entity(entity::layer_snapshot::Entity);
        variants.if_not_exists();
        self.db.execute(backend.build(&variants)).await?;

        Ok(())
    }

    // region: --- JSON helpers

    fn variant_state_to_json(variant: &LayerSnapshot) -> StorageResult<String> {
        let state = VariantState {
            layer_refs: &variant.layer_refs,
            module_refs: &variant.module_refs,
            block_refs: &variant.block_refs,
            plugin_refs: &variant.plugin_refs,
            overrides: &variant.overrides,
            enabled: variant.enabled,
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize layer snapshot: {e}")))
    }

    fn variant_state_from_json(json: &str) -> StorageResult<VariantStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse layer snapshot json: {e}")))
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

    // region: --- Assembly

    fn variant_from_model(model: &entity::layer_snapshot::Model) -> StorageResult<LayerSnapshot> {
        let state = Self::variant_state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(LayerSnapshot {
            id: model.variant_id_branded(),
            name: model.name.clone(),
            layer_refs: state.layer_refs,
            module_refs: state.module_refs,
            block_refs: state.block_refs,
            plugin_refs: state.plugin_refs,
            overrides: state.overrides,
            enabled: state.enabled,
            metadata,
        })
    }

    async fn assemble_layer(&self, model: &entity::layer::Model) -> StorageResult<Layer> {
        let variant_models = entity::layer_snapshot::Entity::find()
            .filter(entity::layer_snapshot::Column::LayerId.eq(model.id.clone()))
            .order_by_asc(entity::layer_snapshot::Column::Position)
            .all(&self.db)
            .await?;

        let mut variants = Vec::with_capacity(variant_models.len());
        for vm in &variant_models {
            variants.push(Self::variant_from_model(vm)?);
        }

        let metadata = Self::metadata_from_json(&model.metadata_json)?;

        Ok(Layer {
            id: model.layer_id_branded(),
            name: model.name.clone(),
            engine_type: EngineType::from_str(&model.engine_type).unwrap_or_default(),
            default_variant_id: model.default_variant_id_branded(),
            variants,
            fx_sends: Vec::new(),
            macro_bank: None,
            modulation: None,
            metadata,
        })
    }

    // endregion: --- Assembly
}

// endregion: --- LayerRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct VariantState<'a> {
    layer_refs: &'a [LayerRef],
    module_refs: &'a [ModuleRef],
    block_refs: &'a [BlockRef],
    plugin_refs: &'a [PluginRef],
    overrides: &'a [Override],
    enabled: bool,
}

#[derive(serde::Deserialize)]
struct VariantStateOwned {
    layer_refs: Vec<LayerRef>,
    module_refs: Vec<ModuleRef>,
    block_refs: Vec<BlockRef>,
    #[serde(default)]
    plugin_refs: Vec<PluginRef>,
    overrides: Vec<Override>,
    enabled: bool,
}

// endregion: --- Serialization types

// region: --- Trait impl

#[async_trait::async_trait]
impl LayerRepo for LayerRepoLive {
    async fn list_layers(&self) -> StorageResult<Vec<Layer>> {
        let models = entity::layer::Entity::find()
            .order_by_asc(entity::layer::Column::Id)
            .all(&self.db)
            .await?;

        let mut out = Vec::with_capacity(models.len());
        for model in &models {
            out.push(self.assemble_layer(model).await?);
        }
        Ok(out)
    }

    async fn load_layer(&self, id: &LayerId) -> StorageResult<Option<Layer>> {
        let model = entity::layer::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(self.assemble_layer(m).await?)),
            None => Ok(None),
        }
    }

    async fn save_layer(&self, layer: &Layer) -> StorageResult<()> {
        // Delete existing (cascade deletes variants)
        entity::layer::Entity::delete_by_id(layer.id.as_str().to_string())
            .exec(&self.db)
            .await
            .ok();

        entity::layer::Entity::insert(entity::layer::ActiveModel {
            id: Set(layer.id.as_str().to_string()),
            name: Set(layer.name.clone()),
            engine_type: Set(layer.engine_type.as_str().to_string()),
            default_variant_id: Set(layer.default_variant_id.as_str().to_string()),
            metadata_json: Set(Self::metadata_to_json(&layer.metadata)?),
        })
        .exec(&self.db)
        .await?;

        for (position, variant) in layer.variants.iter().enumerate() {
            entity::layer_snapshot::Entity::insert(entity::layer_snapshot::ActiveModel {
                id: Set(variant.id.as_str().to_string()),
                layer_id: Set(layer.id.as_str().to_string()),
                position: Set(position as i32),
                name: Set(variant.name.clone()),
                state_json: Set(Self::variant_state_to_json(variant)?),
                metadata_json: Set(Self::metadata_to_json(&variant.metadata)?),
            })
            .exec(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn delete_layer(&self, id: &LayerId) -> StorageResult<()> {
        entity::layer::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn load_variant(
        &self,
        layer_id: &LayerId,
        variant_id: &LayerSnapshotId,
    ) -> StorageResult<Option<LayerSnapshot>> {
        let model = entity::layer_snapshot::Entity::find_by_id(variant_id.as_str().to_string())
            .filter(entity::layer_snapshot::Column::LayerId.eq(layer_id.as_str().to_string()))
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
    use signal_proto::layer::ModuleRef;
    use signal_proto::seed_id;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    async fn test_repo() -> Result<LayerRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = LayerRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    fn sample_layer() -> Layer {
        let v1 = LayerSnapshot::new(seed_id("v1"), "Clean")
            .with_module(ModuleRef::new(seed_id("mod-drive")));
        let v2 = LayerSnapshot::new(seed_id("v2"), "Heavy")
            .with_module(ModuleRef::new(seed_id("mod-drive")).with_variant(seed_id("push")));
        let mut layer = Layer::new(seed_id("layer-1"), "Guitar Layer", EngineType::Guitar, v1);
        layer.add_variant(v2);
        layer
    }

    fn lid(name: &str) -> LayerId {
        LayerId::from_uuid(seed_id(name))
    }
    fn lsid(name: &str) -> LayerSnapshotId {
        LayerSnapshotId::from_uuid(seed_id(name))
    }

    #[tokio::test]
    async fn save_load_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        let layer = sample_layer();

        repo.save_layer(&layer).await?;
        let loaded = repo.load_layer(&lid("layer-1")).await?;

        let loaded = loaded.expect("should find layer");
        assert_eq!(loaded.name, "Guitar Layer");
        assert_eq!(loaded.variants.len(), 2);
        assert_eq!(loaded.default_variant_id, lsid("v1"));
        Ok(())
    }

    #[tokio::test]
    async fn list_layers_returns_all() -> Result<()> {
        let repo = test_repo().await?;

        let l1 = Layer::new(
            seed_id("l1"),
            "Layer 1",
            EngineType::Guitar,
            LayerSnapshot::new(seed_id("v1"), "Default"),
        );
        let l2 = Layer::new(
            seed_id("l2"),
            "Layer 2",
            EngineType::Guitar,
            LayerSnapshot::new(seed_id("v2"), "Default"),
        );
        repo.save_layer(&l1).await?;
        repo.save_layer(&l2).await?;

        let layers = repo.list_layers().await?;
        assert_eq!(layers.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn load_missing_returns_none() -> Result<()> {
        let repo = test_repo().await?;
        let loaded = repo.load_layer(&lid("nonexistent")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn delete_layer_removes_it() -> Result<()> {
        let repo = test_repo().await?;
        let layer = sample_layer();
        repo.save_layer(&layer).await?;

        repo.delete_layer(&lid("layer-1")).await?;
        let loaded = repo.load_layer(&lid("layer-1")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_variant_by_id() -> Result<()> {
        let repo = test_repo().await?;
        let layer = sample_layer();
        repo.save_layer(&layer).await?;

        let variant = repo.load_variant(&lid("layer-1"), &lsid("v2")).await?;
        let variant = variant.expect("should find variant");
        assert_eq!(variant.name, "Heavy");
        assert_eq!(variant.module_refs.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn load_variant_missing_returns_none() -> Result<()> {
        let repo = test_repo().await?;
        let layer = sample_layer();
        repo.save_layer(&layer).await?;

        let variant = repo
            .load_variant(&lid("layer-1"), &lsid("nonexistent"))
            .await?;
        assert!(variant.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn save_overwrites_existing() -> Result<()> {
        let repo = test_repo().await?;

        let v1 = LayerSnapshot::new(seed_id("v1"), "Original");
        let layer = Layer::new(seed_id("layer-1"), "Layer", EngineType::Guitar, v1);
        repo.save_layer(&layer).await?;

        let v1 = LayerSnapshot::new(seed_id("v1"), "Updated");
        let layer = Layer::new(seed_id("layer-1"), "Layer Renamed", EngineType::Guitar, v1);
        repo.save_layer(&layer).await?;

        let loaded = repo.load_layer(&lid("layer-1")).await?.unwrap();
        assert_eq!(loaded.name, "Layer Renamed");
        assert_eq!(loaded.variants.len(), 1);
        assert_eq!(loaded.variants[0].name, "Updated");
        Ok(())
    }

    #[tokio::test]
    async fn metadata_round_trip() -> Result<()> {
        let repo = test_repo().await?;

        let v1 = LayerSnapshot::new(seed_id("v1"), "Default").with_metadata(
            Metadata::new()
                .with_tag("guitar")
                .with_description("Clean tone"),
        );
        let layer = Layer::new(seed_id("layer-1"), "Guitar", EngineType::Guitar, v1)
            .with_metadata(Metadata::new().with_tag("main").with_notes("Primary layer"));
        repo.save_layer(&layer).await?;

        let loaded = repo.load_layer(&lid("layer-1")).await?.unwrap();
        assert!(loaded.metadata.tags.contains("main"));
        assert_eq!(loaded.metadata.notes.as_deref(), Some("Primary layer"));

        let v = &loaded.variants[0];
        assert!(v.metadata.tags.contains("guitar"));
        assert_eq!(v.metadata.description.as_deref(), Some("Clean tone"));
        Ok(())
    }

    // -- Replace module in layer: two variants reference different modules

    #[tokio::test]
    async fn replace_module_in_layer_via_variant_switch() -> Result<()> {
        // -- Setup & Fixtures
        let repo = test_repo().await?;

        let mod_serial = signal_proto::ModulePresetId::from_uuid(seed_id("mod-serial-drive"));
        let mod_parallel = signal_proto::ModulePresetId::from_uuid(seed_id("mod-parallel-time"));

        // Variant "Clean": uses module "mod-serial-drive" (serial chain)
        let v_clean = LayerSnapshot::new(seed_id("v-clean"), "Clean Tone")
            .with_module(ModuleRef::new(mod_serial.clone()))
            .with_override(signal_proto::overrides::Override::set(
                "module.mod-serial-drive.block.drive.param.gain",
                0.30,
            ));

        // Variant "Ambient": uses module "mod-parallel-time" (parallel chain)
        let v_ambient = LayerSnapshot::new(seed_id("v-ambient"), "Ambient")
            .with_module(ModuleRef::new(mod_parallel.clone()).with_variant(seed_id("lush")))
            .with_override(signal_proto::overrides::Override::set(
                "module.mod-parallel-time.block.delay.param.time",
                0.75,
            ))
            .with_override(signal_proto::overrides::Override::bypass(
                "module.mod-parallel-time.block.pre-eq",
                true,
            ));

        let mut layer = Layer::new(seed_id("layer-fx"), "FX Layer", EngineType::Guitar, v_clean);
        layer.add_variant(v_ambient);
        repo.save_layer(&layer).await?;

        // -- Exec: load both variants back
        let loaded = repo.load_layer(&lid("layer-fx")).await?.unwrap();

        // -- Check: Clean variant
        let clean = loaded.variant(&lsid("v-clean")).unwrap();
        assert_eq!(clean.module_refs.len(), 1);
        assert_eq!(clean.module_refs[0].collection_id, mod_serial);
        assert!(clean.module_refs[0].variant_id.is_none()); // default variant
        assert_eq!(clean.overrides.len(), 1);
        assert_eq!(
            clean.overrides[0].path.as_str(),
            "module.mod-serial-drive.block.drive.param.gain"
        );

        // -- Check: Ambient variant — different module, different overrides
        let ambient = loaded.variant(&lsid("v-ambient")).unwrap();
        assert_eq!(ambient.module_refs.len(), 1);
        assert_eq!(ambient.module_refs[0].collection_id, mod_parallel);
        assert_eq!(
            ambient.module_refs[0].variant_id,
            Some(signal_proto::ModuleSnapshotId::from_uuid(seed_id("lush")))
        );
        assert_eq!(ambient.overrides.len(), 2);
        assert_eq!(
            ambient.overrides[0].path.as_str(),
            "module.mod-parallel-time.block.delay.param.time"
        );
        // Second override is a bypass
        match &ambient.overrides[1].op {
            signal_proto::overrides::OverrideOp::Bypass(b) => assert!(b),
            other => panic!("expected Bypass, got {:?}", other),
        }

        // -- Check: switching between variants changes which module is active
        assert_ne!(
            clean.module_refs[0].collection_id,
            ambient.module_refs[0].collection_id,
        );
        Ok(())
    }
}

// endregion: --- Tests
