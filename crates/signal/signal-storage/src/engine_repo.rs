//! Engine repository — data access for Engine collections and EngineScene variants.

use sea_orm::*;
use signal_proto::engine::{Engine, EngineId, EngineScene, EngineSceneId, LayerSelection};
use signal_proto::layer::LayerId;
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::EngineType;

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait EngineRepo: Send + Sync {
    async fn list_engines(&self) -> StorageResult<Vec<Engine>>;
    async fn load_engine(&self, id: &EngineId) -> StorageResult<Option<Engine>>;
    async fn save_engine(&self, engine: &Engine) -> StorageResult<()>;
    async fn delete_engine(&self, id: &EngineId) -> StorageResult<()>;
    async fn load_variant(
        &self,
        engine_id: &EngineId,
        variant_id: &EngineSceneId,
    ) -> StorageResult<Option<EngineScene>>;
}

// endregion: --- Trait

// region: --- EngineRepoLive

#[derive(Clone)]
pub struct EngineRepoLive {
    db: DatabaseConnection,
}

impl EngineRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut engines = schema.create_table_from_entity(entity::engine::Entity);
        engines.if_not_exists();
        self.db.execute(backend.build(&engines)).await?;

        let mut scenes = schema.create_table_from_entity(entity::engine_scene::Entity);
        scenes.if_not_exists();
        self.db.execute(backend.build(&scenes)).await?;

        Ok(())
    }

    fn variant_state_to_json(variant: &EngineScene) -> StorageResult<String> {
        let state = SceneState {
            layer_selections: &variant.layer_selections,
            overrides: &variant.overrides,
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize engine scene: {e}")))
    }

    fn variant_state_from_json(json: &str) -> StorageResult<SceneStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse engine scene json: {e}")))
    }

    fn layer_ids_to_json(ids: &[LayerId]) -> StorageResult<String> {
        let strs: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
        serde_json::to_string(&strs)
            .map_err(|e| StorageError::Data(format!("failed to serialize layer_ids: {e}")))
    }

    fn layer_ids_from_json(json: &str) -> StorageResult<Vec<LayerId>> {
        let strs: Vec<String> = serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse layer_ids json: {e}")))?;
        Ok(strs.into_iter().map(LayerId::from).collect())
    }

    fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
        serde_json::to_string(metadata)
            .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
    }

    fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
    }

    fn variant_from_model(model: &entity::engine_scene::Model) -> StorageResult<EngineScene> {
        let state = Self::variant_state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(EngineScene {
            id: model.variant_id_branded(),
            name: model.name.clone(),
            layer_selections: state.layer_selections,
            overrides: state.overrides,
            metadata,
        })
    }

    async fn assemble_engine(&self, model: &entity::engine::Model) -> StorageResult<Engine> {
        let variant_models = entity::engine_scene::Entity::find()
            .filter(entity::engine_scene::Column::EngineId.eq(model.id.clone()))
            .order_by_asc(entity::engine_scene::Column::Position)
            .all(&self.db)
            .await?;

        let mut variants = Vec::with_capacity(variant_models.len());
        for vm in &variant_models {
            variants.push(Self::variant_from_model(vm)?);
        }

        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        let layer_ids = Self::layer_ids_from_json(&model.layer_ids_json)?;

        Ok(Engine {
            id: model.engine_id_branded(),
            name: model.name.clone(),
            engine_type: EngineType::from_str(&model.engine_type).unwrap_or_default(),
            layer_ids,
            default_variant_id: model.default_variant_id_branded(),
            variants,
            fx_sends: Vec::new(),
            input_track_ref: None,
            macro_bank: None,
            modulation: None,
            metadata,
        })
    }
}

// endregion: --- EngineRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct SceneState<'a> {
    layer_selections: &'a [LayerSelection],
    overrides: &'a [Override],
}

#[derive(serde::Deserialize)]
struct SceneStateOwned {
    layer_selections: Vec<LayerSelection>,
    overrides: Vec<Override>,
}

// endregion: --- Serialization types

// region: --- Trait impl

#[async_trait::async_trait]
impl EngineRepo for EngineRepoLive {
    async fn list_engines(&self) -> StorageResult<Vec<Engine>> {
        let models = entity::engine::Entity::find()
            .order_by_asc(entity::engine::Column::Id)
            .all(&self.db)
            .await?;

        let mut out = Vec::with_capacity(models.len());
        for model in &models {
            out.push(self.assemble_engine(model).await?);
        }
        Ok(out)
    }

    async fn load_engine(&self, id: &EngineId) -> StorageResult<Option<Engine>> {
        let model = entity::engine::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(self.assemble_engine(m).await?)),
            None => Ok(None),
        }
    }

    async fn save_engine(&self, engine: &Engine) -> StorageResult<()> {
        entity::engine::Entity::delete_by_id(engine.id.as_str().to_string())
            .exec(&self.db)
            .await
            .ok();

        entity::engine::Entity::insert(entity::engine::ActiveModel {
            id: Set(engine.id.as_str().to_string()),
            name: Set(engine.name.clone()),
            engine_type: Set(engine.engine_type.as_str().to_string()),
            layer_ids_json: Set(Self::layer_ids_to_json(&engine.layer_ids)?),
            default_variant_id: Set(engine.default_variant_id.as_str().to_string()),
            metadata_json: Set(Self::metadata_to_json(&engine.metadata)?),
        })
        .exec(&self.db)
        .await?;

        for (position, variant) in engine.variants.iter().enumerate() {
            entity::engine_scene::Entity::insert(entity::engine_scene::ActiveModel {
                id: Set(variant.id.as_str().to_string()),
                engine_id: Set(engine.id.as_str().to_string()),
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

    async fn delete_engine(&self, id: &EngineId) -> StorageResult<()> {
        entity::engine::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn load_variant(
        &self,
        engine_id: &EngineId,
        variant_id: &EngineSceneId,
    ) -> StorageResult<Option<EngineScene>> {
        let model = entity::engine_scene::Entity::find_by_id(variant_id.as_str().to_string())
            .filter(entity::engine_scene::Column::EngineId.eq(engine_id.as_str().to_string()))
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
    use signal_proto::seed_id;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    fn eid(name: &str) -> EngineId {
        EngineId::from_uuid(seed_id(name))
    }
    fn esid(name: &str) -> EngineSceneId {
        EngineSceneId::from_uuid(seed_id(name))
    }

    async fn test_repo() -> Result<EngineRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = EngineRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    fn sample_engine() -> Engine {
        let scene1 = EngineScene::new(seed_id("s1"), "Default Scene")
            .with_layer(LayerSelection::new(seed_id("layer-1"), seed_id("v1")));
        let scene2 = EngineScene::new(seed_id("s2"), "Alt Scene")
            .with_layer(LayerSelection::new(seed_id("layer-1"), seed_id("v2")));

        let mut engine = Engine::new(
            seed_id("engine-1"),
            "Guitar Engine",
            EngineType::Guitar,
            vec![LayerId::from_uuid(seed_id("layer-1"))],
            scene1,
        );
        engine.add_variant(scene2);
        engine
    }

    #[tokio::test]
    async fn save_load_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        let engine = sample_engine();
        repo.save_engine(&engine).await?;

        let loaded = repo.load_engine(&eid("engine-1")).await?;
        let loaded = loaded.expect("should find engine");
        assert_eq!(loaded.name, "Guitar Engine");
        assert_eq!(loaded.variants.len(), 2);
        assert_eq!(loaded.layer_ids.len(), 1);
        assert_eq!(loaded.layer_ids[0], LayerId::from_uuid(seed_id("layer-1")));
        Ok(())
    }

    #[tokio::test]
    async fn list_engines_returns_all() -> Result<()> {
        let repo = test_repo().await?;
        let e1 = Engine::new(
            seed_id("e1"),
            "Engine 1",
            EngineType::Guitar,
            vec![LayerId::from_uuid(seed_id("l1"))],
            EngineScene::new(seed_id("s1"), "Default"),
        );
        let e2 = Engine::new(
            seed_id("e2"),
            "Engine 2",
            EngineType::Guitar,
            vec![LayerId::from_uuid(seed_id("l2"))],
            EngineScene::new(seed_id("s2"), "Default"),
        );
        repo.save_engine(&e1).await?;
        repo.save_engine(&e2).await?;

        let engines = repo.list_engines().await?;
        assert_eq!(engines.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn load_missing_returns_none() -> Result<()> {
        let repo = test_repo().await?;
        let loaded = repo.load_engine(&eid("nonexistent")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn delete_engine_removes_it() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_engine(&sample_engine()).await?;
        repo.delete_engine(&eid("engine-1")).await?;
        let loaded = repo.load_engine(&eid("engine-1")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_variant_by_id() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_engine(&sample_engine()).await?;

        let variant = repo.load_variant(&eid("engine-1"), &esid("s2")).await?;
        let variant = variant.expect("should find variant");
        assert_eq!(variant.name, "Alt Scene");
        assert_eq!(variant.layer_selections.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn save_overwrites_existing() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_engine(&sample_engine()).await?;

        let updated = Engine::new(
            seed_id("engine-1"),
            "Renamed",
            EngineType::Guitar,
            vec![
                LayerId::from_uuid(seed_id("layer-1")),
                LayerId::from_uuid(seed_id("layer-2")),
            ],
            EngineScene::new(seed_id("s1"), "Only Scene"),
        );
        repo.save_engine(&updated).await?;

        let loaded = repo.load_engine(&eid("engine-1")).await?.unwrap();
        assert_eq!(loaded.name, "Renamed");
        assert_eq!(loaded.variants.len(), 1);
        assert_eq!(loaded.layer_ids.len(), 2);
        Ok(())
    }
}

// endregion: --- Tests
