use sea_orm::entity::prelude::*;
use signal_proto::scene_template::SceneTemplateId;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "scene_templates")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub state_json: String,
    pub metadata_json: String,
    pub sort_order: i32,
}

impl Model {
    pub fn id_branded(&self) -> SceneTemplateId {
        SceneTemplateId::from(self.id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
