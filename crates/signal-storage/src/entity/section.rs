use sea_orm::entity::prelude::*;
use signal_proto::song::SectionId;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "sections")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub song_id: String,
    pub position: i32,
    pub name: String,
    pub state_json: String,
    pub metadata_json: String,
}

impl Model {
    pub fn variant_id_branded(&self) -> SectionId {
        SectionId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::song::Entity",
        from = "Column::SongId",
        to = "super::song::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Song,
}

impl Related<super::song::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Song.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
