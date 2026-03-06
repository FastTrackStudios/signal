use sea_orm::entity::prelude::*;
use signal_proto::rig::{RigId, RigSceneId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "rigs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub rig_type_id: Option<String>,
    pub engine_ids_json: String,
    pub default_variant_id: String,
    pub macro_bank_json: Option<String>,
    pub metadata_json: String,
}

impl Model {
    pub fn rig_id_branded(&self) -> RigId {
        RigId::from(self.id.clone())
    }

    pub fn default_variant_id_branded(&self) -> RigSceneId {
        RigSceneId::from(self.default_variant_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::rig_scene::Entity")]
    Variants,
}

impl Related<super::rig_scene::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Variants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
