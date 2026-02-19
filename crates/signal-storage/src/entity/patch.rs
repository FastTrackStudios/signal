use sea_orm::entity::prelude::*;
use signal_proto::profile::PatchId;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "patches")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub profile_id: String,
    pub position: i32,
    pub name: String,
    pub state_json: String,
    pub metadata_json: String,
}

impl Model {
    pub fn variant_id_branded(&self) -> PatchId {
        PatchId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::profile::Entity",
        from = "Column::ProfileId",
        to = "super::profile::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Profile,
}

impl Related<super::profile::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Profile.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
