use sea_orm::entity::prelude::*;
use signal_proto::setlist::{SetlistEntryId, SetlistId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "setlists")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub default_entry_id: String,
    pub metadata_json: String,
}

impl Model {
    pub fn setlist_id_branded(&self) -> SetlistId {
        SetlistId::from(self.id.clone())
    }

    pub fn default_entry_id_branded(&self) -> SetlistEntryId {
        SetlistEntryId::from(self.default_entry_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::setlist_entry::Entity")]
    Entries,
}

impl Related<super::setlist_entry::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Entries.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
