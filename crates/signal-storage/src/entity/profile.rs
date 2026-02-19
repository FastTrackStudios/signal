use sea_orm::entity::prelude::*;
use signal_proto::profile::{PatchId, ProfileId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "profiles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub default_variant_id: String,
    pub metadata_json: String,
}

impl Model {
    pub fn profile_id_branded(&self) -> ProfileId {
        ProfileId::from(self.id.clone())
    }

    pub fn default_variant_id_branded(&self) -> PatchId {
        PatchId::from(self.default_variant_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::patch::Entity")]
    Variants,
}

impl Related<super::patch::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Variants.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
