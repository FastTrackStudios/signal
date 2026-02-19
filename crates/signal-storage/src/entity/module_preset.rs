use sea_orm::entity::prelude::*;
use signal_proto::{ModulePresetId, ModuleSnapshotId, ModuleType};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "module_presets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub module_type: String,
    pub default_snapshot_id: String,
    #[sea_orm(default_value = "{}")]
    pub metadata_json: String,
}

impl Model {
    pub fn preset_id_branded(&self) -> ModulePresetId {
        ModulePresetId::from(self.id.clone())
    }

    pub fn default_snapshot_id_branded(&self) -> ModuleSnapshotId {
        ModuleSnapshotId::from(self.default_snapshot_id.clone())
    }

    pub fn module_type_branded(&self) -> ModuleType {
        ModuleType::from_str(&self.module_type).unwrap_or_default()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::module_snapshot::Entity")]
    Snapshots,
}

impl Related<super::module_snapshot::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Snapshots.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
