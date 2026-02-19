use sea_orm::entity::prelude::*;
use signal_proto::{PresetId, SnapshotId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "presets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub block_type: String,
    pub name: String,
    pub default_snapshot_id: String,
    #[sea_orm(default_value = "{}")]
    pub metadata_json: String,
}

impl Model {
    pub fn preset_id_branded(&self) -> PresetId {
        PresetId::from(self.id.clone())
    }

    pub fn default_snapshot_id_branded(&self) -> SnapshotId {
        SnapshotId::from(self.default_snapshot_id.clone())
    }
}

impl From<Model> for (PresetId, SnapshotId) {
    fn from(model: Model) -> Self {
        (
            model.preset_id_branded(),
            model.default_snapshot_id_branded(),
        )
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::snapshot::Entity")]
    Snapshots,
}

impl Related<super::snapshot::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Snapshots.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
