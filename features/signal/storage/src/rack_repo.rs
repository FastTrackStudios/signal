//! Rack repository — data access for Rack collections.

use sea_orm::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ColIdx, ConnectionTrait, EntityTrait, Iden,
    IdenStatic, Iterable, QueryOrder, QueryTrait, Schema, Set, StatementBuilder,
};
use signal_proto::fx_send::FxSendBus;
use signal_proto::rack::{Rack, RackId, RackSlot};

use crate::entity;
use crate::{DatabaseConnection, StorageError, StorageResult};

// region: --- Trait

#[async_trait::async_trait]
pub trait RackRepo: Send + Sync + 'static {
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

// endregion: --- Tests
