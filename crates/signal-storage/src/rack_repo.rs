//! Rack repository — data access for Rack collections.

use sea_orm::*;
use signal_proto::fx_send::FxSendBus;
use signal_proto::rack::{Rack, RackId, RackSlot};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait RackRepo: Send + Sync {
    async fn list_racks(&self) -> StorageResult<Vec<Rack>>;
    async fn load_rack(&self, id: &RackId) -> StorageResult<Option<Rack>>;
    async fn save_rack(&self, rack: &Rack) -> StorageResult<()>;
    async fn delete_rack(&self, id: &RackId) -> StorageResult<()>;
}

// endregion: --- Trait

// region: --- RackRepoLive

#[derive(Clone)]
pub struct RackRepoLive {
    db: DatabaseConnection,
}

impl RackRepoLive {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn init_schema(&self) -> StorageResult<()> {
        let backend = self.db.get_database_backend();
        let schema = Schema::new(backend);

        let mut stmt = schema.create_table_from_entity(entity::rack::Entity);
        stmt.if_not_exists();
        self.db.execute(backend.build(&stmt)).await?;

        Ok(())
    }

    fn state_to_json(rack: &Rack) -> StorageResult<String> {
        let state = RackState {
            slots: &rack.slots,
            active_slot: rack.active_slot,
            fx_send_buses: &rack.fx_send_buses,
        };
        serde_json::to_string(&state)
            .map_err(|e| StorageError::Data(format!("failed to serialize rack: {e}")))
    }

    fn state_from_json(json: &str) -> StorageResult<RackStateOwned> {
        serde_json::from_str(json)
            .map_err(|e| StorageError::Data(format!("failed to parse rack json: {e}")))
    }

    fn from_model(model: &entity::rack::Model) -> StorageResult<Rack> {
        let state = Self::state_from_json(&model.state_json)?;
        Ok(Rack {
            id: model.rack_id_branded(),
            name: model.name.clone(),
            slots: state.slots,
            active_slot: state.active_slot,
            fx_send_buses: state.fx_send_buses,
        })
    }
}

// endregion: --- RackRepoLive

// region: --- Serialization types

#[derive(serde::Serialize)]
struct RackState<'a> {
    slots: &'a [RackSlot],
    active_slot: Option<u32>,
    fx_send_buses: &'a [FxSendBus],
}

#[derive(serde::Deserialize)]
struct RackStateOwned {
    slots: Vec<RackSlot>,
    active_slot: Option<u32>,
    fx_send_buses: Vec<FxSendBus>,
}

// endregion: --- Serialization types

// region: --- Trait impl

#[async_trait::async_trait]
impl RackRepo for RackRepoLive {
    async fn list_racks(&self) -> StorageResult<Vec<Rack>> {
        let models = entity::rack::Entity::find()
            .order_by_asc(entity::rack::Column::Id)
            .all(&self.db)
            .await?;

        models.iter().map(Self::from_model).collect()
    }

    async fn load_rack(&self, id: &RackId) -> StorageResult<Option<Rack>> {
        let model = entity::rack::Entity::find_by_id(id.as_str().to_string())
            .one(&self.db)
            .await?;
        match model {
            Some(ref m) => Ok(Some(Self::from_model(m)?)),
            None => Ok(None),
        }
    }

    async fn save_rack(&self, rack: &Rack) -> StorageResult<()> {
        entity::rack::Entity::delete_by_id(rack.id.as_str().to_string())
            .exec(&self.db)
            .await
            .ok();

        entity::rack::Entity::insert(entity::rack::ActiveModel {
            id: Set(rack.id.as_str().to_string()),
            name: Set(rack.name.clone()),
            state_json: Set(Self::state_to_json(rack)?),
        })
        .exec(&self.db)
        .await?;

        Ok(())
    }

    async fn delete_rack(&self, id: &RackId) -> StorageResult<()> {
        entity::rack::Entity::delete_by_id(id.as_str().to_string())
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
    use crate::Database;
    use signal_proto::rig::RigId;
    use signal_proto::seed_id;

    fn rkid(name: &str) -> RackId {
        RackId::from_uuid(seed_id(name))
    }

    async fn test_repo() -> StorageResult<RackRepoLive> {
        let db = Database::connect("sqlite::memory:").await?;
        let repo = RackRepoLive::new(db);
        repo.init_schema().await?;
        Ok(repo)
    }

    fn sample_rack() -> Rack {
        let mut rack = Rack::new(seed_id("rack-1"), "Vocal Rack");
        rack.slots.push(RackSlot {
            position: 0,
            rig_id: RigId::from_uuid(seed_id("rig-lead-vox")),
            active: true,
        });
        rack.slots.push(RackSlot {
            position: 1,
            rig_id: RigId::from_uuid(seed_id("rig-harmony-vox")),
            active: true,
        });
        rack.active_slot = Some(0);
        rack
    }

    #[tokio::test]
    async fn save_load_round_trip() {
        let repo = test_repo().await.unwrap();
        repo.save_rack(&sample_rack()).await.unwrap();

        let loaded = repo.load_rack(&rkid("rack-1")).await.unwrap();
        let loaded = loaded.expect("should find rack");
        assert_eq!(loaded.name, "Vocal Rack");
        assert_eq!(loaded.slots.len(), 2);
        assert_eq!(loaded.active_slot, Some(0));
    }

    #[tokio::test]
    async fn list_racks_returns_all() {
        let repo = test_repo().await.unwrap();
        let r1 = Rack::new(seed_id("r1"), "Rack 1");
        let r2 = Rack::new(seed_id("r2"), "Rack 2");
        repo.save_rack(&r1).await.unwrap();
        repo.save_rack(&r2).await.unwrap();

        let racks = repo.list_racks().await.unwrap();
        assert_eq!(racks.len(), 2);
    }

    #[tokio::test]
    async fn load_missing_returns_none() {
        let repo = test_repo().await.unwrap();
        let loaded = repo.load_rack(&rkid("nonexistent")).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn delete_rack_removes_it() {
        let repo = test_repo().await.unwrap();
        repo.save_rack(&sample_rack()).await.unwrap();
        repo.delete_rack(&rkid("rack-1")).await.unwrap();
        let loaded = repo.load_rack(&rkid("rack-1")).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn fx_send_buses_round_trip() {
        let repo = test_repo().await.unwrap();
        let mut rack = sample_rack();
        rack.fx_send_buses.push(FxSendBus {
            id: signal_proto::fx_send::FxSendBusId::new(),
            name: "AUX".into(),
            sends: vec![],
            sub_category: Some("TIME".into()),
        });
        repo.save_rack(&rack).await.unwrap();

        let loaded = repo.load_rack(&rkid("rack-1")).await.unwrap().unwrap();
        assert_eq!(loaded.fx_send_buses.len(), 1);
        assert_eq!(loaded.fx_send_buses[0].name, "AUX");
        assert_eq!(
            loaded.fx_send_buses[0].sub_category.as_deref(),
            Some("TIME")
        );
    }

    #[tokio::test]
    async fn save_overwrites_existing() {
        let repo = test_repo().await.unwrap();
        repo.save_rack(&sample_rack()).await.unwrap();

        let mut updated = sample_rack();
        updated.name = "Updated Vocal Rack".into();
        updated.slots.push(RackSlot {
            position: 2,
            rig_id: RigId::from_uuid(seed_id("rig-bg-vox")),
            active: false,
        });
        repo.save_rack(&updated).await.unwrap();

        let loaded = repo.load_rack(&rkid("rack-1")).await.unwrap().unwrap();
        assert_eq!(loaded.name, "Updated Vocal Rack");
        assert_eq!(loaded.slots.len(), 3);
    }
}

// endregion: --- Tests
