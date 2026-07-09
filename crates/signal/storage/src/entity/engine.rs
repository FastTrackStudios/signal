use sea_orm::entity::prelude::*;
use signal_proto::engine::{EngineId, EngineSceneId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "engines")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub engine_type: String,
    pub layer_ids_json: String,
    pub default_variant_id: String,
    pub metadata_json: String,
}

impl Model {
    pub fn engine_id_branded(&self) -> EngineId {
        EngineId::from(self.id.clone())
    }

    pub fn default_variant_id_branded(&self) -> EngineSceneId {
        EngineSceneId::from(self.default_variant_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::engine_scene::Entity")]
    Variants,
}

impl Related<super::engine_scene::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Variants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
