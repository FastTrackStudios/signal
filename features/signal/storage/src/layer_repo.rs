//! Layer repository — data access for Layer collections and `LayerSnapshot` variants.

use sea_orm::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ColIdx, ColumnTrait, ConnectionTrait,
    EntityTrait, Iden, IdenStatic, Iterable, QueryFilter, QueryOrder, QueryTrait, Schema, Set,
    StatementBuilder,
};
use signal_proto::EngineType;
use signal_proto::layer::{
    BlockRef, Layer, LayerId, LayerRef, LayerSnapshot, LayerSnapshotId, ModuleRef, PluginRef,
};
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

/// Data-access trait for Layer collections.
#[async_trait::async_trait]
pub trait LayerRepo: Send + Sync + 'static {
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

// endregion: --- Tests
