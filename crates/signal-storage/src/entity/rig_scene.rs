use sea_orm::entity::prelude::*;
use signal_proto::rig::RigSceneId;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "rig_scenes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub rig_id: String,
    pub position: i32,
    pub name: String,
    pub state_json: String,
    pub metadata_json: String,
}

impl Model {
    pub fn variant_id_branded(&self) -> RigSceneId {
        RigSceneId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::rig::Entity",
        from = "Column::RigId",
        to = "super::rig::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Rig,
}

impl Related<super::rig::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Rig.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
