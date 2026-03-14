//! Signal storage layer -- SQLite persistence for the signal domain, built on SeaORM.
//!
//! Provides repository traits and their live implementations for every signal
//! entity (blocks, modules, layers, engines, rigs, profiles, songs, setlists,
//! scene templates, racks). Each repository follows the pattern of a trait
//! (e.g. `BlockRepo`) paired with a `*Live` implementation backed by a
//! `DatabaseConnection`.
//!
//! # Architecture position
//!
//! ```text
//! signal-proto (domain types)
//!       |
//!       v
//! signal-storage (this crate -- persistence)
//!       |
//!       v
//! signal-live (runtime services)
//! ```
//!
//! **Depends on**: `signal-proto`
//!
//! **Depended on by**: `signal-live`, `signal` (facade)
//!
//! # Key types
//!
//! - **Repository traits**: [`BlockRepo`], [`ModuleRepo`], [`LayerRepo`], [`EngineRepo`],
//!   [`RigRepo`], [`ProfileRepo`], [`SongRepo`], [`SetlistRepo`], [`SceneTemplateRepo`],
//!   [`RackRepo`]
//! - **Live implementations**: [`BlockRepoLive`], [`ModuleRepoLive`], [`LayerRepoLive`], etc.
//! - **Import/export**: [`ExportBundle`], [`ImportOptions`], [`ImportResult`], [`ConflictStrategy`]
//! - **Seed data**: [`SeedBundle`], [`runtime_seed_bundle`] for default content
//! - **Error types**: [`StorageError`], [`StorageResult`]

pub mod block_repo;
pub mod daw_snapshot_repo;
pub mod engine_repo;
pub mod entity;
pub mod import_export;
pub mod layer_repo;
pub mod module_repo;
pub mod profile_repo;
pub mod rack_repo;
pub mod rig_repo;
pub mod scene_template_repo;
pub mod seed_data;
pub mod setlist_repo;
pub mod song_repo;

pub use block_repo::{BlockRepo, BlockRepoLive};
pub use daw_snapshot_repo::{
    DawSnapshotRepo, DawSnapshotRepoLive, StoredChunkSnapshot, StoredParamSnapshot,
};
pub use engine_repo::{EngineRepo, EngineRepoLive};
pub use import_export::{ConflictStrategy, ExportBundle, ImportOptions, ImportResult};
pub use layer_repo::{LayerRepo, LayerRepoLive};
pub use module_repo::{ModuleRepo, ModuleRepoLive};
pub use profile_repo::{ProfileRepo, ProfileRepoLive};
pub use rack_repo::{RackRepo, RackRepoLive};
pub use rig_repo::{RigRepo, RigRepoLive};
pub use scene_template_repo::{SceneTemplateRepo, SceneTemplateRepoLive};
pub use sea_orm::{Database, DatabaseConnection, DbErr};
pub use seed_data::{
    default_block_collections, default_module_collections, default_seed_engines,
    default_seed_layers, default_seed_profiles, default_seed_rigs, default_seed_setlists,
    default_seed_songs, runtime_seed_bundle, SeedBundle,
};
pub use setlist_repo::{SetlistRepo, SetlistRepoLive};
pub use song_repo::{SongRepo, SongRepoLive};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Db(#[from] DbErr),
    #[error("data error: {0}")]
    Data(String),
}

pub type StorageResult<T> = Result<T, StorageError>;
