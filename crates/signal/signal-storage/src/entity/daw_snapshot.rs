use sea_orm::entity::prelude::*;

/// Stored DAW parameter snapshot — captured plugin parameter state for recall.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "daw_param_snapshots")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Which rig/scene this snapshot belongs to.
    pub owner_id: String,
    /// Human-readable label (e.g., "Clean Scene - Amp params").
    pub name: String,
    /// JSON-serialized `DawParameterSnapshot` (HashMap<String, DawParamValue>).
    pub params_json: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Stored DAW state chunk — captured binary plugin state for recall.
pub mod chunk {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
    #[sea_orm(table_name = "daw_chunk_snapshots")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        /// Which rig/scene this chunk belongs to.
        pub owner_id: String,
        /// FX plugin identifier.
        pub fx_id: String,
        /// Plugin name for human reference.
        pub plugin_name: String,
        /// Base64-encoded binary chunk data.
        pub chunk_data_b64: String,
        pub created_at: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
