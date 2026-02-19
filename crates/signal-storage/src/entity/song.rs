use sea_orm::entity::prelude::*;
use signal_proto::song::{SectionId, SongId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "songs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub default_variant_id: String,
    pub metadata_json: String,
}

impl Model {
    pub fn song_id_branded(&self) -> SongId {
        SongId::from(self.id.clone())
    }

    pub fn default_variant_id_branded(&self) -> SectionId {
        SectionId::from(self.default_variant_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::section::Entity")]
    Variants,
}

impl Related<super::section::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Variants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
