use sea_orm::entity::prelude::*;
use signal_proto::engine::EngineSceneId;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "engine_scenes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub engine_id: String,
    pub position: i32,
    pub name: String,
    pub state_json: String,
    pub metadata_json: String,
}

impl Model {
    pub fn variant_id_branded(&self) -> EngineSceneId {
        EngineSceneId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::engine::Entity",
        from = "Column::EngineId",
        to = "super::engine::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Engine,
}

impl Related<super::engine::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Engine.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
