use sea_orm::entity::prelude::*;
use signal_proto::setlist::SetlistEntryId;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "setlist_entries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub setlist_id: String,
    pub position: i32,
    pub name: String,
    pub state_json: String,
    pub metadata_json: String,
}

impl Model {
    pub fn entry_id_branded(&self) -> SetlistEntryId {
        SetlistEntryId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::setlist::Entity",
        from = "Column::SetlistId",
        to = "super::setlist::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Setlist,
}

impl Related<super::setlist::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Setlist.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
