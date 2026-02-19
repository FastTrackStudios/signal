//! Public Signal crate.
//!
//! Re-exports protocol types and controller APIs for consumers.

#[cfg(feature = "daw")]
pub mod reaper_applier;

pub use signal_controller::ops;
pub use signal_controller::SignalController;

/// Ergonomic alias: `let signal = Signal::new(service);`
pub type Signal = SignalController;
pub use signal_live::engine::{
    block_to_snapshot, find_param_index, graph_state_chunks, graph_to_snapshot,
    live_params_into_block, param_name_matches, DawParamValue, DawParameterSnapshot, DawStateChunk,
    LiveParam, MorphDiffEntry, MorphEngine, MorphParamChange,
};
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

pub fn bootstrap_in_memory_controller() -> SignalController {
    let runtime = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
    let db = runtime.block_on(async {
        let seeds = runtime_seed_bundle();
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("failed to connect in-memory sqlite");

        let block_repo = BlockRepoLive::new(db.clone());
        block_repo
            .init_schema()
            .await
            .expect("failed to initialize block schema");
        block_repo
            .reseed_defaults(&seeds.block_collections)
            .await
            .expect("failed to reseed block collections");

        let module_repo = ModuleRepoLive::new(db.clone());
        module_repo
            .init_schema()
            .await
            .expect("failed to initialize module schema");
        module_repo
            .reseed_defaults(&seeds.module_collections)
            .await
            .expect("failed to reseed module collections");

        let layer_repo = LayerRepoLive::new(db.clone());
        layer_repo
            .init_schema()
            .await
            .expect("failed to initialize layer schema");
        for layer in seeds.layers {
            layer_repo
                .save_layer(&layer)
                .await
                .expect("failed to seed layer");
        }

        let engine_repo = EngineRepoLive::new(db.clone());
        engine_repo
            .init_schema()
            .await
            .expect("failed to initialize engine schema");
        for engine in seeds.engines {
            engine_repo
                .save_engine(&engine)
                .await
                .expect("failed to seed engine");
        }

        let rig_repo = RigRepoLive::new(db.clone());
        rig_repo
            .init_schema()
            .await
            .expect("failed to initialize rig schema");
        for rig in seeds.rigs {
            rig_repo.save_rig(&rig).await.expect("failed to seed rig");
        }

        let profile_repo = ProfileRepoLive::new(db.clone());
        profile_repo
            .init_schema()
            .await
            .expect("failed to initialize profile schema");
        for profile in seeds.profiles {
            profile_repo
                .save_profile(&profile)
                .await
                .expect("failed to seed profile");
        }

        let song_repo = SongRepoLive::new(db.clone());
        song_repo
            .init_schema()
            .await
            .expect("failed to initialize song schema");
        for song in seeds.songs {
            song_repo
                .save_song(&song)
                .await
                .expect("failed to seed song");
        }

        let setlist_repo = SetlistRepoLive::new(db.clone());
        setlist_repo
            .init_schema()
            .await
            .expect("failed to initialize setlist schema");
        for setlist in seeds.setlists {
            setlist_repo
                .save_setlist(&setlist)
                .await
                .expect("failed to seed setlist");
        }

        db
    });
    let service = Arc::new(SignalLive::from_db(db));
    SignalController::new(service)
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
    }

    let service = Arc::new(SignalLive::from_db(db));
    Ok(SignalController::new(service))
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
pub fn mock_guitar() -> SignalController {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for bass-type development and testing.
pub fn mock_bass() -> SignalController {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for keys-type development and testing.
pub fn mock_keys() -> SignalController {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for drums-type development and testing.
pub fn mock_drums() -> SignalController {
    bootstrap_in_memory_controller()
}

/// Create a mock controller for vocals-type development and testing.
pub fn mock_vocals() -> SignalController {
    bootstrap_in_memory_controller()
}

// endregion: --- Mock constructors
