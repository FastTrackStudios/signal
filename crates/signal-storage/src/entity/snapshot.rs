use sea_orm::entity::prelude::*;
use signal_proto::{PresetId, SnapshotId};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "snapshots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub preset_id: String,
    pub name: String,
    pub state_json: String,
    #[sea_orm(default_value = "{}")]
    pub metadata_json: String,
    #[sea_orm(default_value = "1")]
    pub version: i32,
    /// Binary plugin state (e.g. JUCE preset `.bin`), base64-encoded.
    /// NULL when no binary state is available.
    #[sea_orm(column_type = "Text", nullable)]
    pub state_data_b64: Option<String>,
}

impl Model {
    pub fn snapshot_id_branded(&self) -> SnapshotId {
        SnapshotId::from(self.id.clone())
    }

    pub fn preset_id_branded(&self) -> PresetId {
        PresetId::from(self.preset_id.clone())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::preset::Entity",
        from = "Column::PresetId",
        to = "super::preset::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Preset,
}

impl Related<super::preset::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Preset.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
