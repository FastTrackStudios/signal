//! Signal facade crate -- the public API surface for the signal domain.
//!
//! This crate re-exports types from every layer of the signal stack so that
//! consumers only need a single `signal` dependency. It also provides bootstrap
//! functions for constructing a fully-wired [`SignalController`] with a database
//! and seeded default data.
//!
//! # Re-export strategy
//!
//! - `signal-proto` is re-exported via glob (`pub use signal_proto::*`) so all
//!   domain types, IDs, and service traits are available at the `signal::` path.
//! - `signal-storage` repo traits, error types, and seed helpers are explicitly
//!   re-exported.
//! - `signal-live` engine utilities (morph engine, DAW param snapshots, macro
//!   setup) are explicitly re-exported.
//! - `signal-controller` provides [`SignalController`] (aliased as [`Signal`])
//!   and the `ops` namespace.
//!
//! # Bootstrap functions
//!
//! - [`bootstrap_in_memory_controller`] / [`bootstrap_in_memory_controller_async`] --
//!   creates a controller with an in-memory SQLite database pre-seeded with
//!   default content. Useful for tests and development.
//! - [`connect_db`] -- connects to a file-based SQLite database with schema
//!   initialization (no seed data).
//! - [`connect_db_seeded`] -- connects to a file-based SQLite database, seeds
//!   default data on first run, and refreshes RfxChain presets from disk on
//!   every startup.
//! - [`mock_guitar`], [`mock_bass`], etc. -- convenience constructors for
//!   instrument-specific mock controllers.
//!
//! # Architecture position
//!
//! ```text
//! signal-proto -> signal-storage -> signal-live -> signal-controller
//!                                                       |
//!                                                       v
//!                                                signal (this crate)
//! ```
//!
//! **Depends on**: `signal-proto`, `signal-storage`, `signal-live`,
//! `signal-controller`, `nam-manager`. Optionally `daw-control`, `daw-proto`
//! (behind the `daw` feature).
//!
//! **Depended on by**: `signal-ui`, application crates (`fts-control-desktop`)

#[cfg(feature = "daw")]
pub mod reaper_applier;

#[cfg(feature = "daw")]
pub mod rig_scene_manager;

pub use signal_controller::ops;
pub use signal_controller::SignalController;

/// Ergonomic alias: `let signal = Signal::new(service);`
pub type Signal = SignalController;
pub use signal_live::engine::{
    block_to_snapshot, find_param_index, graph_state_chunks, graph_to_snapshot,
    live_params_into_block, param_name_matches, DawParamValue, DawParameterSnapshot, DawStateChunk,
    LiveParam, MorphDiffEntry, MorphEngine, MorphParamChange,
};
pub use signal_live::daw_block_ops::{
    LoadBlockResult, LoadModuleResult, ResolvedFxLoad, ResolvedModuleLoad,
};
pub use signal_live::macro_setup::{self, LiveMacroBinding, MacroSetupResult};
pub use signal_live::macro_registry;
pub use signal_live::SignalLive;
pub use signal_proto::*;
pub use signal_storage::{
    default_block_collections, default_module_collections, default_seed_engines,
    default_seed_layers, default_seed_profiles, default_seed_rigs, default_seed_setlists,
    default_seed_songs, runtime_seed_bundle, BlockRepo, BlockRepoLive, Database,
    DatabaseConnection, DbErr, EngineRepo, EngineRepoLive, LayerRepo, LayerRepoLive, ModuleRepo,
    ModuleRepoLive, ProfileRepo, ProfileRepoLive, RackRepo, RackRepoLive, RigRepo, RigRepoLive,
    SceneTemplateRepo, SceneTemplateRepoLive, SetlistRepo, SetlistRepoLive, SongRepo, SongRepoLive,
    StorageError, StorageResult,
};
use std::sync::Arc;

pub async fn bootstrap_in_memory_controller_async() -> Result<SignalController, StorageError> {
    let db = Database::connect("sqlite::memory:").await?;
    let seeds = runtime_seed_bundle();

    let block_repo = BlockRepoLive::new(db.clone());
    block_repo.init_schema().await?;
    block_repo.reseed_defaults(&seeds.block_collections).await?;

    let module_repo = ModuleRepoLive::new(db.clone());
    module_repo.init_schema().await?;
    module_repo
        .reseed_defaults(&seeds.module_collections)
        .await?;

    let layer_repo = LayerRepoLive::new(db.clone());
    layer_repo.init_schema().await?;
    for layer in seeds.layers {
        layer_repo.save_layer(&layer).await?;
    }

    let engine_repo = EngineRepoLive::new(db.clone());
    engine_repo.init_schema().await?;
    for engine in seeds.engines {
        engine_repo.save_engine(&engine).await?;
    }

    let rig_repo = RigRepoLive::new(db.clone());
    rig_repo.init_schema().await?;
    for rig in seeds.rigs {
        rig_repo.save_rig(&rig).await?;
    }

    let profile_repo = ProfileRepoLive::new(db.clone());
    profile_repo.init_schema().await?;
    for profile in seeds.profiles {
        profile_repo.save_profile(&profile).await?;
    }

    let song_repo = SongRepoLive::new(db.clone());
    song_repo.init_schema().await?;
    for song in seeds.songs {
        song_repo.save_song(&song).await?;
    }
    let setlist_repo = SetlistRepoLive::new(db.clone());
    setlist_repo.init_schema().await?;
    for setlist in seeds.setlists {
        setlist_repo.save_setlist(&setlist).await?;
    }
    let scene_template_repo = signal_storage::SceneTemplateRepoLive::new(db.clone());
    scene_template_repo.init_schema().await?;

    let rack_repo = RackRepoLive::new(db);
    rack_repo.init_schema().await?;

    let service = Arc::new(SignalLive::new(
        Arc::new(block_repo),
        Arc::new(module_repo),
        Arc::new(layer_repo),
        Arc::new(engine_repo),
        Arc::new(rig_repo),
        Arc::new(profile_repo),
        Arc::new(song_repo),
        Arc::new(setlist_repo),
        Arc::new(scene_template_repo),
        Arc::new(rack_repo),
    ));
    Ok(SignalController::new(service))
}

pub fn bootstrap_in_memory_controller() -> Result<SignalController, StorageError> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| StorageError::Data(format!("failed to build tokio runtime: {e}")))?;
    runtime.block_on(bootstrap_in_memory_controller_async())
}

// region: --- DB connection factory

/// Connect to a file-based SQLite database and return a controller with initialized schemas.
///
/// Creates the database file if it doesn't exist. All table schemas are created
/// with `IF NOT EXISTS` so this is safe to call on existing databases.
pub async fn connect_db(path: &str) -> Result<SignalController, StorageError> {
    let url = format!("sqlite:{}?mode=rwc", path);
    let db = Database::connect(&url).await?;
    init_all_schemas(&db).await?;
    let service = Arc::new(SignalLive::from_db(db));
    Ok(SignalController::new(service))
}

/// Connect to a database and seed it with default data if empty.
///
/// RfxChain-based presets from `~/Music/FastTrackStudio/Library/presets/`
/// are always refreshed from disk on every startup, so swapping `.RfxChain`
/// files takes effect immediately without deleting the database.
pub async fn connect_db_seeded(path: &str) -> Result<SignalController, StorageError> {
    let url = format!("sqlite:{}?mode=rwc", path);
    let db = Database::connect(&url).await?;
    init_all_schemas(&db).await?;

    // Only seed if the block table is empty (first run).
    let block_repo = BlockRepoLive::new(db.clone());
    let existing = block_repo
        .list_block_collections(signal_proto::BlockType::Amp)
        .await?;
    if existing.is_empty() {
        let seeds = runtime_seed_bundle();
        block_repo.reseed_defaults(&seeds.block_collections).await?;
        let module_repo = ModuleRepoLive::new(db.clone());
        module_repo
            .reseed_defaults(&seeds.module_collections)
            .await?;
        let layer_repo = LayerRepoLive::new(db.clone());
        for layer in seeds.layers {
            layer_repo.save_layer(&layer).await?;
        }
        let engine_repo = EngineRepoLive::new(db.clone());
        for engine in seeds.engines {
            engine_repo.save_engine(&engine).await?;
        }
        let rig_repo = RigRepoLive::new(db.clone());
        for rig in seeds.rigs {
            rig_repo.save_rig(&rig).await?;
        }
        let profile_repo = ProfileRepoLive::new(db.clone());
        for profile in seeds.profiles {
            profile_repo.save_profile(&profile).await?;
        }
        let song_repo = SongRepoLive::new(db.clone());
        for song in seeds.songs {
            song_repo.save_song(&song).await?;
        }
        let setlist_repo = SetlistRepoLive::new(db.clone());
        for setlist in seeds.setlists {
            setlist_repo.save_setlist(&setlist).await?;
        }
    } else {
        // Always refresh RfxChain presets from disk so file swaps take
        // effect without deleting the database.
        refresh_rfxchain_presets(&block_repo).await?;
    }

    let service = Arc::new(SignalLive::from_db(db));
    Ok(SignalController::new(service))
}

/// Re-import all `.RfxChain`-based presets from the library directory.
///
/// Uses `save_block_collection` which deletes-then-inserts, so swapped
/// files on disk are picked up on every app launch.
async fn refresh_rfxchain_presets(block_repo: &BlockRepoLive) -> Result<(), StorageError> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let library_path = std::path::PathBuf::from(home).join("Music/FastTrackStudio/Library");
    let rfx_presets =
        signal_storage::seed_data::catalog_import::rfxchain_block_collections(&library_path);
    for preset in rfx_presets {
        block_repo.save_block_collection(preset).await?;
    }
    Ok(())
}

/// Initialize all table schemas on a database connection.
async fn init_all_schemas(db: &DatabaseConnection) -> Result<(), StorageError> {
    BlockRepoLive::new(db.clone()).init_schema().await?;
    ModuleRepoLive::new(db.clone()).init_schema().await?;
    LayerRepoLive::new(db.clone()).init_schema().await?;
    EngineRepoLive::new(db.clone()).init_schema().await?;
    RigRepoLive::new(db.clone()).init_schema().await?;
    ProfileRepoLive::new(db.clone()).init_schema().await?;
    SongRepoLive::new(db.clone()).init_schema().await?;
    SetlistRepoLive::new(db.clone()).init_schema().await?;
    SceneTemplateRepoLive::new(db.clone()).init_schema().await?;
    RackRepoLive::new(db.clone()).init_schema().await?;
    Ok(())
}

// endregion: --- DB connection factory

// region: --- Mock constructors

/// Create a mock controller for guitar-type development and testing.
pub fn mock_guitar() -> Result<SignalController, StorageError> {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for bass-type development and testing.
pub fn mock_bass() -> Result<SignalController, StorageError> {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for keys-type development and testing.
pub fn mock_keys() -> Result<SignalController, StorageError> {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for drums-type development and testing.
pub fn mock_drums() -> Result<SignalController, StorageError> {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for vocals-type development and testing.
pub fn mock_vocals() -> Result<SignalController, StorageError> {
    bootstrap_in_memory_controller()
}

// endregion: --- Mock constructors
