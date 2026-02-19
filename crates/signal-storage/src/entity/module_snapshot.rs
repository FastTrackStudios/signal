use sea_orm::entity::prelude::*;
use signal_proto::{ModulePresetId, ModuleSnapshotId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "module_snapshots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub module_preset_id: String,
    pub name: String,
    pub state_json: String,
    #[sea_orm(default_value = "{}")]
    pub metadata_json: String,
    #[sea_orm(default_value = "1")]
    pub version: i32,
}

impl Model {
    pub fn snapshot_id_branded(&self) -> ModuleSnapshotId {
        ModuleSnapshotId::from(self.id.clone())
    }

    pub fn preset_id_branded(&self) -> ModulePresetId {
        ModulePresetId::from(self.module_preset_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::module_preset::Entity",
        from = "Column::ModulePresetId",
        to = "super::module_preset::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Preset,
}

impl Related<super::module_preset::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Preset.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
