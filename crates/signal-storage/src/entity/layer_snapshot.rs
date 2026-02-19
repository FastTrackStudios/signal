use sea_orm::entity::prelude::*;
use signal_proto::layer::LayerSnapshotId;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "layer_snapshots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub layer_id: String,
    pub position: i32,
    pub name: String,
    pub state_json: String,
    pub metadata_json: String,
}

impl Model {
    pub fn variant_id_branded(&self) -> LayerSnapshotId {
        LayerSnapshotId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::layer::Entity",
        from = "Column::LayerId",
        to = "super::layer::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Layer,
}

impl Related<super::layer::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Layer.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
