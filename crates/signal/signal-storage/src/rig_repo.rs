//! Rig repository — data access for Rig collections and RigScene variants.

use sea_orm::*;
use signal_proto::engine::EngineId;
use signal_proto::metadata::Metadata;
use signal_proto::overrides::Override;
use signal_proto::rig::{EngineSelection, Rig, RigId, RigScene, RigSceneId, RigType};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait RigRepo: Send + Sync {
    async fn list_rigs(&self) -> StorageResult<Vec<Rig>>;
    async fn load_rig(&self, id: &RigId) -> StorageResult<Option<Rig>>;
    async fn save_rig(&self, rig: &Rig) -> StorageResult<()>;
    async fn delete_rig(&self, id: &RigId) -> StorageResult<()>;
    async fn load_variant(
        &self,
        rig_id: &RigId,
        variant_id: &RigSceneId,
    ) -> StorageResult<Option<RigScene>>;
}

// endregion: --- Trait

// region: --- RigRepoLive

#[derive(Clone)]
pub struct RigRepoLive {
    db: DatabaseConnection,
}

impl RigRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut rigs = schema.create_table_from_entity(entity::rig::Entity);
        rigs.if_not_exists();
        self.db.execute(backend.build(&rigs)).await?;

        let mut scenes = schema.create_table_from_entity(entity::rig_scene::Entity);
        scenes.if_not_exists();
        self.db.execute(backend.build(&scenes)).await?;

        Ok(())
    }

    fn variant_state_to_json(variant: &RigScene) -> StorageResult<String> {
        let state = SceneState {
            engine_selections: &variant.engine_selections,
            overrides: &variant.overrides,
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize rig scene: {e}")))
    }

    fn variant_state_from_json(json: &str) -> StorageResult<SceneStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse rig scene json: {e}")))
    }

    fn engine_ids_to_json(ids: &[EngineId]) -> StorageResult<String> {
        let strs: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
        serde_json::to_string(&strs)
            .map_err(|e| StorageError::Data(format!("failed to serialize engine_ids: {e}")))
    }

    fn engine_ids_from_json(json: &str) -> StorageResult<Vec<EngineId>> {
        let strs: Vec<String> = serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse engine_ids json: {e}")))?;
        Ok(strs.into_iter().map(EngineId::from).collect())
    }

    fn metadata_to_json(metadata: &Metadata) -> StorageResult<String> {
        serde_json::to_string(metadata)
            .map_err(|e| StorageError::Data(format!("failed to serialize metadata: {e}")))
    }

    fn metadata_from_json(json: &str) -> StorageResult<Metadata> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse metadata json: {e}")))
    }

    fn variant_from_model(model: &entity::rig_scene::Model) -> StorageResult<RigScene> {
        let state = Self::variant_state_from_json(&model.state_json)?;
        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        Ok(RigScene {
            id: model.variant_id_branded(),
            name: model.name.clone(),
            engine_selections: state.engine_selections,
            overrides: state.overrides,
            metadata,
        })
    }

    async fn assemble_rig(&self, model: &entity::rig::Model) -> StorageResult<Rig> {
        let variant_models = entity::rig_scene::Entity::find()
            .filter(entity::rig_scene::Column::RigId.eq(model.id.clone()))
            .order_by_asc(entity::rig_scene::Column::Position)
            .all(&self.db)
            .await?;

        let mut variants = Vec::with_capacity(variant_models.len());
        for vm in &variant_models {
            variants.push(Self::variant_from_model(vm)?);
        }

        let metadata = Self::metadata_from_json(&model.metadata_json)?;
        let engine_ids = Self::engine_ids_from_json(&model.engine_ids_json)?;
        let rig_type = model
            .rig_type_id
            .as_ref()
            .map(|s| RigType::from_str(s).unwrap_or_default());

        let macro_bank = match &model.macro_bank_json {
            Some(json) if !json.is_empty() => serde_json::from_str(json)
                .map_err(|e| StorageError::Data(format!("failed to parse macro_bank json: {e}")))?,
            _ => None,
        };

        Ok(Rig {
            id: model.rig_id_branded(),
            name: model.name.clone(),
            rig_type,
            engine_ids,
            default_variant_id: model.default_variant_id_branded(),
            variants,
            fx_sends: Vec::new(),
            input_track_ref: None,
            macro_bank,
            modulation: None,
            metadata,
        })
    }
}

// endregion: --- RigRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct SceneState<'a> {
    engine_selections: &'a [EngineSelection],
    overrides: &'a [Override],
}

#[derive(serde::Deserialize)]
struct SceneStateOwned {
    engine_selections: Vec<EngineSelection>,
    overrides: Vec<Override>,
}

// endregion: --- Serialization types

// region: --- Trait impl

#[async_trait::async_trait]
impl RigRepo for RigRepoLive {
    async fn list_rigs(&self) -> StorageResult<Vec<Rig>> {
        let models = entity::rig::Entity::find()
            .order_by_asc(entity::rig::Column::Id)
            .all(&self.db)
            .await?;

        let mut out = Vec::with_capacity(models.len());
        for model in &models {
            out.push(self.assemble_rig(model).await?);
        }
        Ok(out)
    }

    async fn load_rig(&self, id: &RigId) -> StorageResult<Option<Rig>> {
        let model = entity::rig::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(self.assemble_rig(m).await?)),
            None => Ok(None),
        }
    }

    async fn save_rig(&self, rig: &Rig) -> StorageResult<()> {
        entity::rig::Entity::delete_by_id(rig.id.as_str().to_string())
            .exec(&self.db)
            .await
            .ok();

        let macro_bank_json = rig
            .macro_bank
            .as_ref()
            .map(|mb| {
                serde_json::to_string(mb)
                    .map_err(|e| StorageError::Data(format!("failed to serialize macro_bank: {e}")))
            })
            .transpose()?;

        entity::rig::Entity::insert(entity::rig::ActiveModel {
            id: Set(rig.id.as_str().to_string()),
            name: Set(rig.name.clone()),
            rig_type_id: Set(rig.rig_type.as_ref().map(|t| t.as_str().to_string())),
            engine_ids_json: Set(Self::engine_ids_to_json(&rig.engine_ids)?),
            default_variant_id: Set(rig.default_variant_id.as_str().to_string()),
            macro_bank_json: Set(macro_bank_json),
            metadata_json: Set(Self::metadata_to_json(&rig.metadata)?),
        })
        .exec(&self.db)
        .await?;

        for (position, variant) in rig.variants.iter().enumerate() {
            entity::rig_scene::Entity::insert(entity::rig_scene::ActiveModel {
                id: Set(variant.id.as_str().to_string()),
                rig_id: Set(rig.id.as_str().to_string()),
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

    async fn delete_rig(&self, id: &RigId) -> StorageResult<()> {
        entity::rig::Entity::delete_by_id(id.as_str().to_string())
            .exec(&self.db)
            .await?;
        Ok(())
    }

    async fn load_variant(
        &self,
        rig_id: &RigId,
        variant_id: &RigSceneId,
    ) -> StorageResult<Option<RigScene>> {
        let model = entity::rig_scene::Entity::find_by_id(variant_id.as_str().to_string())
            .filter(entity::rig_scene::Column::RigId.eq(rig_id.as_str().to_string()))
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

    fn rid(name: &str) -> RigId {
        RigId::from_uuid(seed_id(name))
    }
    fn rsid(name: &str) -> RigSceneId {
        RigSceneId::from_uuid(seed_id(name))
    }

    async fn test_repo() -> Result<RigRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = RigRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    fn sample_rig() -> Rig {
        let scene1 = RigScene::new(seed_id("rs1"), "Default Scene")
            .with_engine(EngineSelection::new(seed_id("engine-1"), seed_id("s1")));
        let scene2 = RigScene::new(seed_id("rs2"), "Alt Scene")
            .with_engine(EngineSelection::new(seed_id("engine-1"), seed_id("s2")));

        let mut rig = Rig::new(
            seed_id("rig-1"),
            "Guitar Rig",
            vec![EngineId::from_uuid(seed_id("engine-1"))],
            scene1,
        )
        .with_rig_type("guitar");
        rig.add_variant(scene2);
        rig
    }

    #[tokio::test]
    async fn save_load_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_rig(&sample_rig()).await?;

        let loaded = repo.load_rig(&rid("rig-1")).await?;
        let loaded = loaded.expect("should find rig");
        assert_eq!(loaded.name, "Guitar Rig");
        assert_eq!(loaded.variants.len(), 2);
        assert_eq!(loaded.rig_type.unwrap().as_str(), "guitar");
        assert_eq!(loaded.engine_ids.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn list_rigs_returns_all() -> Result<()> {
        let repo = test_repo().await?;
        let r1 = Rig::new(
            seed_id("r1"),
            "Rig 1",
            vec![EngineId::from_uuid(seed_id("e1"))],
            RigScene::new(seed_id("rs1"), "Default"),
        );
        let r2 = Rig::new(
            seed_id("r2"),
            "Rig 2",
            vec![EngineId::from_uuid(seed_id("e2"))],
            RigScene::new(seed_id("rs2"), "Default"),
        );
        repo.save_rig(&r1).await?;
        repo.save_rig(&r2).await?;

        let rigs = repo.list_rigs().await?;
        assert_eq!(rigs.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn load_missing_returns_none() -> Result<()> {
        let repo = test_repo().await?;
        let loaded = repo.load_rig(&rid("nonexistent")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn delete_rig_removes_it() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_rig(&sample_rig()).await?;
        repo.delete_rig(&rid("rig-1")).await?;
        let loaded = repo.load_rig(&rid("rig-1")).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_variant_by_id() -> Result<()> {
        let repo = test_repo().await?;
        repo.save_rig(&sample_rig()).await?;

        let variant = repo.load_variant(&rid("rig-1"), &rsid("rs2")).await?;
        let variant = variant.expect("should find variant");
        assert_eq!(variant.name, "Alt Scene");
        assert_eq!(variant.engine_selections.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn rig_type_none_round_trip() -> Result<()> {
        let repo = test_repo().await?;
        let rig = Rig::new(
            seed_id("rig-no-type"),
            "Untyped",
            vec![],
            RigScene::new(seed_id("rs1"), "Default"),
        );
        repo.save_rig(&rig).await?;

        let loaded = repo.load_rig(&rid("rig-no-type")).await?.unwrap();
        assert!(loaded.rig_type.is_none());
        Ok(())
    }
}

// endregion: --- Tests
