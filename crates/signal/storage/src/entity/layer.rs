use sea_orm::entity::prelude::*;
use signal_proto::layer::{LayerId, LayerSnapshotId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "layers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub engine_type: String,
    pub default_variant_id: String,
    pub metadata_json: String,
}

impl Model {
    pub fn layer_id_branded(&self) -> LayerId {
        LayerId::from(self.id.clone())
    }

    pub fn default_variant_id_branded(&self) -> LayerSnapshotId {
        LayerSnapshotId::from(self.default_variant_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::layer_snapshot::Entity")]
    Variants,
}

impl Related<super::layer_snapshot::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Variants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
