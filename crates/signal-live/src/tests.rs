use super::*;
use signal_proto::{seed_id, ModuleType};
use signal_storage::{
    runtime_seed_bundle, BlockRepoLive, Database, EngineRepoLive, LayerRepoLive, ModuleRepoLive,
    ProfileRepoLive, RackRepoLive, RigRepoLive, SetlistRepoLive, SongRepoLive,
};
use std::time::{Duration, Instant};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;


async fn seeded_service() -> Result<
    SignalLive<
        BlockRepoLive,
        ModuleRepoLive,
        LayerRepoLive,
        EngineRepoLive,
        RigRepoLive,
        ProfileRepoLive,
        SongRepoLive,
        SetlistRepoLive,
    >,
> {
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
    let scene_template_repo = SceneTemplateRepoLive::new(db.clone());
    scene_template_repo.init_schema().await?;
    let rack_repo = RackRepoLive::new(db);
    rack_repo.init_schema().await?;
    Ok(SignalLive::new(
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
    ))
}

// region: --- get_block / set_block

#[tokio::test]
async fn test_live_get_block_returns_seeded_state() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec
    let block = svc.get_block(BlockType::Amp).await?;

    // -- Check
    assert!(!block.parameters().is_empty());
    Ok(())
}

#[tokio::test]
async fn test_live_get_block_returns_default_for_empty_repo() -> Result<()> {
    // -- Setup & Fixtures
    let db = Database::connect("sqlite::memory:").await?;
    let block_repo = BlockRepoLive::new(db.clone());
    block_repo.init_schema().await?;
    let module_repo = ModuleRepoLive::new(db.clone());
    module_repo.init_schema().await?;
    let layer_repo = LayerRepoLive::new(db.clone());
    layer_repo.init_schema().await?;
    let engine_repo = EngineRepoLive::new(db.clone());
    engine_repo.init_schema().await?;
    let rig_repo = RigRepoLive::new(db.clone());
    rig_repo.init_schema().await?;
    let profile_repo = ProfileRepoLive::new(db.clone());
    profile_repo.init_schema().await?;
    let song_repo = SongRepoLive::new(db.clone());
    song_repo.init_schema().await?;
    let setlist_repo = SetlistRepoLive::new(db.clone());
    setlist_repo.init_schema().await?;
    let scene_template_repo = SceneTemplateRepoLive::new(db.clone());
    scene_template_repo.init_schema().await?;
    let rack_repo = RackRepoLive::new(db);
    rack_repo.init_schema().await?;
    let svc = SignalLive::new(
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
    );

    // -- Exec
    let block = svc.get_block(BlockType::Amp).await?;

    // -- Check
    assert_eq!(block, Block::default());
    Ok(())
}

#[tokio::test]
async fn test_live_set_block_persists_and_returns() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;
    let new_block = Block::new(0.1, 0.2, 0.3);

    // -- Exec
    let returned = svc
        .set_block(BlockType::Drive, new_block.clone())
        .await?;

    // -- Check
    assert_eq!(returned, new_block);
    let loaded = svc.get_block(BlockType::Drive).await?;
    assert_eq!(loaded, new_block);
    Ok(())
}

// endregion: --- get_block / set_block

// region: --- Setlist operations

#[tokio::test]
async fn test_live_list_setlists_returns_demo_setlist() -> Result<()> {
    let svc = seeded_service().await?;

    let setlists = svc.list_setlists(.await?;

    assert!(setlists.len() >= 2, "expected at least 2 setlists, got {}", setlists.len());
    let worship = setlists
        .iter()
        .find(|s| s.name == "Worship Set")
        .expect("worship set");
    assert_eq!(worship.entries.len(), 2);
    let commercial = setlists
        .iter()
        .find(|s| s.name == "Commercial Music")
        .expect("commercial music");
    assert!(commercial.entries.len() >= 3, "expected at least 3 entries in Commercial Music, got {}", commercial.entries.len());
    Ok(())
}

#[tokio::test]
async fn test_live_load_setlist_entry_returns_dummy_song_entry() -> Result<()> {
    let svc = seeded_service().await?;

    let entry = svc
        .load_setlist_entry(
            &cx,
            signal_proto::setlist::SetlistId::from(seed_id("commercial-music")),
            signal_proto::setlist::SetlistEntryId::from(seed_id("commercial-thriller")),
        )
        .await?;

    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.name, "Thriller");
    assert_eq!(entry.song_id.as_str(), seed_id("thriller-song").to_string());
    Ok(())
}

// endregion: --- Setlist operations

// region: --- Block collections (list / load)

#[tokio::test]
async fn test_live_list_collections_returns_seeded_presets() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec
    let amp_collections = svc.list_block_presets(BlockType::Amp).await?;
    let drive_collections = svc.list_block_presets(BlockType::Drive).await?;

    // -- Check
    assert!(amp_collections.len() >= 6, "expected at least 6 amp presets, got {}", amp_collections.len());
    assert!(drive_collections.len() >= 7, "expected at least 7 drive presets, got {}", drive_collections.len());
    Ok(())
}

#[tokio::test]
async fn test_live_list_collections_empty_repo() -> Result<()> {
    // -- Setup & Fixtures
    let db = Database::connect("sqlite::memory:").await?;
    let block_repo = BlockRepoLive::new(db.clone());
    block_repo.init_schema().await?;
    let module_repo = ModuleRepoLive::new(db.clone());
    module_repo.init_schema().await?;
    let layer_repo = LayerRepoLive::new(db.clone());
    layer_repo.init_schema().await?;
    let engine_repo = EngineRepoLive::new(db.clone());
    engine_repo.init_schema().await?;
    let rig_repo = RigRepoLive::new(db.clone());
    rig_repo.init_schema().await?;
    let profile_repo = ProfileRepoLive::new(db.clone());
    profile_repo.init_schema().await?;
    let song_repo = SongRepoLive::new(db.clone());
    song_repo.init_schema().await?;
    let setlist_repo = SetlistRepoLive::new(db.clone());
    setlist_repo.init_schema().await?;
    let scene_template_repo = SceneTemplateRepoLive::new(db.clone());
    scene_template_repo.init_schema().await?;
    let rack_repo = RackRepoLive::new(db);
    rack_repo.init_schema().await?;
    let svc = SignalLive::new(
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
    );

    // -- Exec
    let collections = svc.list_block_presets(BlockType::Amp).await?;

    // -- Check
    assert!(collections.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_live_load_default_variant_applies_block() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;
    let preset_id = PresetId::from_uuid(seed_id("amp-twin"));

    // -- Exec: load the default variant (triggers side-effect)
    let snapshot = svc
        .load_block_preset(BlockType::Amp, preset_id)
        .await?;

    // -- Check: variant returned
    assert!(snapshot.is_some());
    let snapshot = snapshot.unwrap();
    assert_eq!(
        snapshot.id(),
        &SnapshotId::from_uuid(seed_id("amp-twin-default"))
    );

    // -- Check: current block was updated to match the loaded variant
    let current = svc.get_block(BlockType::Amp).await?;
    assert_eq!(current, snapshot.block());
    Ok(())
}

#[tokio::test]
async fn test_live_load_specific_variant_applies_block() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;
    let preset_id = PresetId::from_uuid(seed_id("amp-twin"));
    let snapshot_id = SnapshotId::from_uuid(seed_id("amp-twin-surf"));

    // -- Exec
    let snapshot = svc
        .load_block_preset_snapshot(BlockType::Amp, preset_id, snapshot_id.clone())
        .await?;

    // -- Check: correct variant returned
    assert!(snapshot.is_some());
    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot.id(), &snapshot_id);

    // -- Check: current block updated
    let current = svc.get_block(BlockType::Amp).await?;
    assert_eq!(current, snapshot.block());
    Ok(())
}

#[tokio::test]
async fn test_live_load_nonexistent_collection_returns_none() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec
    let result = svc
        .load_block_preset(BlockType::Amp, PresetId::new())
        .await?;

    // -- Check
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn test_live_load_nonexistent_variant_returns_none() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec
    let result = svc
        .load_block_preset_snapshot(
            &cx,
            BlockType::Amp,
            PresetId::from_uuid(seed_id("amp-twin")),
            SnapshotId::new(),
        )
        .await?;

    // -- Check
    assert!(result.is_none());
    Ok(())
}

// endregion: --- Block collections (list / load)

// region: --- Module collections (list / load)

#[tokio::test]
async fn test_live_list_module_collections() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec
    let module_collections = svc.list_module_presets(.await?;

    // -- Check
    assert!(module_collections.len() >= 23, "expected at least 23 module collections, got {}", module_collections.len());
    let mut names: Vec<&str> = module_collections.iter().map(|c| c.name()).collect();
    names.sort();
    assert!(names.contains(&"Drive Duo"));
    assert!(names.contains(&"Full Drive Stack"));
    assert!(names.contains(&"Parallel Time"));
    assert!(names.contains(&"Source"));
    assert!(names.contains(&"Rescue"));
    Ok(())
}

#[tokio::test]
async fn test_live_load_module_default_variant() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;
    let preset_id = ModulePresetId::from_uuid(seed_id("drive-full-stack"));

    // -- Exec
    let snapshot = svc.load_module_preset(preset_id).await?;

    // -- Check
    assert!(snapshot.is_some());
    let snapshot = snapshot.unwrap();
    assert_eq!(
        snapshot.id(),
        &ModuleSnapshotId::from_uuid(seed_id("drive-full-stack-default"))
    );
    assert_eq!(snapshot.module().blocks().len(), 4);
    Ok(())
}

#[tokio::test]
async fn test_live_load_module_specific_variant() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;
    let preset_id = ModulePresetId::from_uuid(seed_id("drive-full-stack"));
    let snapshot_id = ModuleSnapshotId::from_uuid(seed_id("drive-full-stack-push"));

    // -- Exec
    let snapshot = svc
        .load_module_preset_snapshot(preset_id, snapshot_id.clone())
        .await?;

    // -- Check
    assert!(snapshot.is_some());
    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot.id(), &snapshot_id);
    assert_eq!(snapshot.name(), "Push");
    Ok(())
}

#[tokio::test]
async fn test_live_load_nonexistent_module_collection() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec
    let result = svc.load_module_preset(ModulePresetId::new()).await?;

    // -- Check
    assert!(result.is_none());
    Ok(())
}

// endregion: --- Module collections (list / load)

// region: --- Resolver determinism

#[tokio::test]
async fn test_live_load_variant_then_different_variant_updates_block() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec: load "surf" variant
    let surf = svc
        .load_block_preset_snapshot(
            &cx,
            BlockType::Amp,
            PresetId::from_uuid(seed_id("amp-twin")),
            SnapshotId::from_uuid(seed_id("amp-twin-surf")),
        )
        .await?
        .unwrap();

    let block_after_surf = svc.get_block(BlockType::Amp).await?;
    assert_eq!(block_after_surf, surf.block());

    // -- Exec: load "jazz" variant (should overwrite)
    let jazz = svc
        .load_block_preset_snapshot(
            &cx,
            BlockType::Amp,
            PresetId::from_uuid(seed_id("amp-twin")),
            SnapshotId::from_uuid(seed_id("amp-twin-jazz")),
        )
        .await?
        .unwrap();

    // -- Check: current block reflects the most recently loaded variant
    let block_after_jazz = svc.get_block(BlockType::Amp).await?;
    assert_eq!(block_after_jazz, jazz.block());
    assert_ne!(block_after_jazz, surf.block());
    Ok(())
}

#[tokio::test]
async fn test_live_cross_collection_load_updates_correct_block_type() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec: load an amp variant
    let amp_before = svc.get_block(BlockType::Amp).await?;
    let _drive = svc
        .load_block_preset(
            &cx,
            BlockType::Drive,
            PresetId::from_uuid(seed_id("drive-level")),
        )
        .await?;

    // -- Check: amp block was not affected by loading a drive variant
    let amp_after = svc.get_block(BlockType::Amp).await?;
    assert_eq!(amp_before, amp_after);
    Ok(())
}

// endregion: --- Resolver determinism

// region: --- Layer service

#[tokio::test]
async fn test_live_list_layers_returns_seeded() -> Result<()> {
    let svc = seeded_service().await?;

    let layers = svc.list_layers(.await?;
    assert!(layers.len() >= 12, "expected at least 12 layers, got {}", layers.len());
    assert!(layers.iter().any(|l| l.name == "Keys Core"));
    assert!(layers.iter().any(|l| l.name == "Guitar Main"));
    assert!(layers.iter().any(|l| l.name == "Vocal Main"));
    Ok(())
}

#[tokio::test]
async fn test_live_load_layer_by_id() -> Result<()> {
    let svc = seeded_service().await?;

    let layer = svc
        .load_layer(LayerId::from_uuid(seed_id("keys-layer-core")))
        .await?;
    assert!(layer.is_some());
    let layer = layer.unwrap();
    assert_eq!(layer.variants.len(), 2);
    Ok(())
}

#[tokio::test]
async fn test_live_load_layer_missing_returns_none() -> Result<()> {
    let svc = seeded_service().await?;

    let layer = svc.load_layer(LayerId::new()).await?;
    assert!(layer.is_none());
    Ok(())
}

#[tokio::test]
async fn test_live_save_and_delete_layer() -> Result<()> {
    let svc = seeded_service().await?;

    let variant = LayerSnapshot::new(seed_id("test-v1"), "Test Default");
    let layer = Layer::new(
        seed_id("test-layer"),
        "Test Layer",
        signal_proto::EngineType::Guitar,
        variant,
    );
    svc.save_layer(layer).await?;

    let loaded = svc
        .load_layer(LayerId::from_uuid(seed_id("test-layer")))
        .await?;
    assert!(loaded.is_some());

    svc.delete_layer(LayerId::from_uuid(seed_id("test-layer")))
        .await?;
    let after_delete = svc
        .load_layer(LayerId::from_uuid(seed_id("test-layer")))
        .await?;
    assert!(after_delete.is_none());
    Ok(())
}

#[tokio::test]
async fn test_live_load_layer_variant() -> Result<()> {
    let svc = seeded_service().await?;

    let variant = svc
        .load_layer_variant(
            &cx,
            LayerId::from_uuid(seed_id("synth-layer-osc")),
            LayerSnapshotId::from_uuid(seed_id("synth-layer-osc-alt")),
        )
        .await?;
    assert!(variant.is_some());
    let variant = variant.unwrap();
    assert_eq!(variant.name, "Alt");
    assert_eq!(variant.block_refs.len(), 3);
    Ok(())
}

// endregion: --- Layer service

// region: --- Engine service

#[tokio::test]
async fn test_live_list_engines_seeded() -> Result<()> {
    let svc = seeded_service().await?;

    let engines = svc.list_engines(.await?;
    assert_eq!(engines.len(), 6);
    let synth = engines
        .iter()
        .find(|e| e.name == "Synth Engine")
        .expect("expected seeded synth engine");
    assert_eq!(synth.variants.len(), 2);
    Ok(())
}

#[tokio::test]
async fn test_live_save_load_delete_engine() -> Result<()> {
    use signal_proto::engine::{EngineScene, LayerSelection};

    let svc = seeded_service().await?;

    let scene =
        EngineScene::new(seed_id("scene-1"), "Default Scene").with_layer(LayerSelection::new(
            seed_id("keys-layer-core"),
            seed_id("keys-layer-core-default"),
        ));
    let engine = Engine::new(
        seed_id("engine-1"),
        "Keys Engine Test",
        signal_proto::EngineType::Keys,
        vec![LayerId::from_uuid(seed_id("keys-layer-core"))],
        scene,
    );

    svc.save_engine(engine).await?;

    let loaded = svc
        .load_engine(EngineId::from_uuid(seed_id("engine-1")))
        .await?;
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.name, "Keys Engine Test");
    assert_eq!(loaded.layer_ids.len(), 1);
    assert_eq!(loaded.variants.len(), 1);

    let engines = svc.list_engines(.await?;
    assert_eq!(engines.len(), 7); // 6 seeded + 1 just saved

    svc.delete_engine(EngineId::from_uuid(seed_id("engine-1")))
        .await?;
    let after_delete = svc
        .load_engine(EngineId::from_uuid(seed_id("engine-1")))
        .await?;
    assert!(after_delete.is_none());
    Ok(())
}

#[tokio::test]
async fn test_live_load_engine_variant() -> Result<()> {
    use signal_proto::engine::{EngineScene, LayerSelection};

    let svc = seeded_service().await?;

    let scene = EngineScene::new(seed_id("scene-clean"), "Clean").with_layer(LayerSelection::new(
        seed_id("keys-layer-core"),
        seed_id("keys-layer-core-default"),
    ));
    let mut engine = Engine::new(
        seed_id("engine-2"),
        "Keys Engine 2",
        signal_proto::EngineType::Keys,
        vec![LayerId::from_uuid(seed_id("keys-layer-core"))],
        scene,
    );
    engine.add_variant(
        EngineScene::new(seed_id("scene-heavy"), "Heavy").with_layer(LayerSelection::new(
            seed_id("keys-layer-core"),
            seed_id("keys-layer-core-bright"),
        )),
    );
    svc.save_engine(engine).await?;

    let variant = svc
        .load_engine_variant(
            &cx,
            EngineId::from_uuid(seed_id("engine-2")),
            EngineSceneId::from_uuid(seed_id("scene-heavy")),
        )
        .await?;
    assert!(variant.is_some());
    let variant = variant.unwrap();
    assert_eq!(variant.name, "Heavy");
    assert_eq!(variant.layer_selections.len(), 1);
    assert_eq!(
        variant.layer_selections[0].variant_id,
        LayerSnapshotId::from_uuid(seed_id("keys-layer-core-bright"))
    );
    Ok(())
}

// endregion: --- Engine service

// region: --- Preset (rig) service

#[tokio::test]
async fn test_live_list_presets_all_seeded() -> Result<()> {
    let svc = seeded_service().await?;

    let rigs = svc.list_rigs(.await?;
    assert_eq!(rigs.len(), 3);
    assert!(rigs.iter().all(|r| r.name == "MegaRig"));
    let keys_rig = rigs
        .iter()
        .find(|r| r.rig_type.unwrap().as_str() == "keys")
        .expect("expected seeded keys megarig");
    assert_eq!(keys_rig.variants.len(), 4);
    Ok(())
}

#[tokio::test]
async fn test_live_save_load_delete_preset() -> Result<()> {
    use signal_proto::engine::EngineId;
    use signal_proto::rig::{EngineSelection, RigScene};

    let svc = seeded_service().await?;

    let scene = RigScene::new(seed_id("rs-default"), "Default Scene").with_engine(
        EngineSelection::new(seed_id("engine-1"), seed_id("scene-1")),
    );
    let rig = Rig::new(
        seed_id("rig-1"),
        "Guitar Rig",
        vec![EngineId::from_uuid(seed_id("engine-1"))],
        scene,
    )
    .with_rig_type("guitar");

    svc.save_rig(rig).await?;

    let loaded = svc
        .load_rig(RigId::from_uuid(seed_id("rig-1")))
        .await?;
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.name, "Guitar Rig");
    assert_eq!(loaded.engine_ids.len(), 1);
    assert_eq!(loaded.variants.len(), 1);
    assert_eq!(loaded.rig_type.unwrap().as_str(), "guitar");

    let rigs = svc.list_rigs(.await?;
    assert_eq!(rigs.len(), 4); // 3 seeded + 1 just saved

    svc.delete_rig(RigId::from_uuid(seed_id("rig-1")))
        .await?;
    let after_delete = svc
        .load_rig(RigId::from_uuid(seed_id("rig-1")))
        .await?;
    assert!(after_delete.is_none());
    Ok(())
}

#[tokio::test]
async fn test_live_load_preset_variant() -> Result<()> {
    use signal_proto::engine::EngineId;
    use signal_proto::rig::{EngineSelection, RigScene};

    let svc = seeded_service().await?;

    let scene1 = RigScene::new(seed_id("rs-clean"), "Clean").with_engine(EngineSelection::new(
        seed_id("engine-1"),
        seed_id("scene-clean"),
    ));
    let mut rig = Rig::new(
        seed_id("rig-2"),
        "Guitar Rig 2",
        vec![EngineId::from_uuid(seed_id("engine-1"))],
        scene1,
    );
    rig.add_variant(
        RigScene::new(seed_id("rs-heavy"), "Heavy").with_engine(EngineSelection::new(
            seed_id("engine-1"),
            seed_id("scene-heavy"),
        )),
    );
    svc.save_rig(rig).await?;

    let variant = svc
        .load_rig_variant(
            &cx,
            RigId::from_uuid(seed_id("rig-2")),
            RigSceneId::from_uuid(seed_id("rs-heavy")),
        )
        .await?;
    assert!(variant.is_some());
    let variant = variant.unwrap();
    assert_eq!(variant.name, "Heavy");
    assert_eq!(variant.engine_selections.len(), 1);
    Ok(())
}

// endregion: --- Preset (rig) service

// region: --- Profile service

#[tokio::test]
async fn test_live_list_profiles_seeded() -> Result<()> {
    let svc = seeded_service().await?;

    let profiles = svc.list_profiles(.await?;
    assert_eq!(profiles.len(), 5);
    let keys = profiles
        .iter()
        .find(|p| p.name == "Keys Feature")
        .expect("keys feature profile");
    assert_eq!(keys.patches.len(), 4);
    Ok(())
}

#[tokio::test]
async fn test_live_save_load_delete_profile() -> Result<()> {
    use signal_proto::profile::Patch;

    let svc = seeded_service().await?;

    let patch = Patch::from_rig_scene(
        seed_id("p-clean"),
        "Clean",
        seed_id("rig-1"),
        seed_id("rs-clean"),
    );
    let mut profile = Profile::new(seed_id("profile-1"), "Worship", patch);
    profile.add_patch(Patch::from_rig_scene(
        seed_id("p-lead"),
        "Lead",
        seed_id("rig-1"),
        seed_id("rs-lead"),
    ));

    svc.save_profile(profile).await?;

    let loaded = svc
        .load_profile(ProfileId::from_uuid(seed_id("profile-1")))
        .await?;
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.name, "Worship");
    assert_eq!(loaded.patches.len(), 2);

    let profiles = svc.list_profiles(.await?;
    assert_eq!(profiles.len(), 6); // 5 seeded + 1 just saved

    svc.delete_profile(ProfileId::from_uuid(seed_id("profile-1")))
        .await?;
    let after_delete = svc
        .load_profile(ProfileId::from_uuid(seed_id("profile-1")))
        .await?;
    assert!(after_delete.is_none());
    Ok(())
}

#[tokio::test]
async fn test_live_load_profile_variant() -> Result<()> {
    use signal_proto::profile::{Patch, PatchTarget};

    let svc = seeded_service().await?;

    let patch1 = Patch::from_rig_scene(
        seed_id("p-clean"),
        "Clean",
        seed_id("rig-1"),
        seed_id("rs-clean"),
    );
    let mut profile = Profile::new(seed_id("profile-2"), "Blues", patch1);
    profile.add_patch(Patch::from_rig_scene(
        seed_id("p-crunch"),
        "Crunch",
        seed_id("rig-1"),
        seed_id("rs-crunch"),
    ));
    svc.save_profile(profile).await?;

    let variant = svc
        .load_profile_variant(
            &cx,
            ProfileId::from_uuid(seed_id("profile-2")),
            PatchId::from_uuid(seed_id("p-crunch")),
        )
        .await?;
    assert!(variant.is_some());
    let variant = variant.unwrap();
    assert_eq!(variant.name, "Crunch");
    assert_eq!(
        variant.target,
        PatchTarget::RigScene {
            rig_id: RigId::from_uuid(seed_id("rig-1")),
            scene_id: RigSceneId::from_uuid(seed_id("rs-crunch")),
        }
    );
    Ok(())
}

// endregion: --- Profile service

// region: --- Song service

#[tokio::test]
async fn test_live_list_songs_seeded() -> Result<()> {
    let svc = seeded_service().await?;

    let songs = svc.list_songs(.await?;
    assert!(songs.len() >= 3, "expected at least 3 songs, got {}", songs.len());
    let feature = songs
        .iter()
        .find(|s| s.name == "Feature-Demo Song")
        .expect("feature song exists");
    assert_eq!(feature.sections.len(), 4);
    assert_eq!(feature.artist.as_deref(), Some("Signal2"));
    assert!(songs.iter().any(|s| s.name == "Dummy Song"));
    Ok(())
}

#[tokio::test]
async fn test_live_save_load_delete_song() -> Result<()> {
    use signal_proto::song::Section;

    let svc = seeded_service().await?;

    let verse = Section::from_patch(seed_id("sec-verse"), "Verse", seed_id("patch-clean"));
    let chorus = Section::from_patch(seed_id("sec-chorus"), "Chorus", seed_id("patch-lead"));
    let mut song = Song::new(seed_id("song-1"), "Amazing Grace", verse).with_artist("Traditional");
    song.add_section(chorus);

    svc.save_song(song).await?;

    let loaded = svc
        .load_song(SongId::from_uuid(seed_id("song-1")))
        .await?;
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.name, "Amazing Grace");
    assert_eq!(loaded.artist.as_deref(), Some("Traditional"));
    assert_eq!(loaded.sections.len(), 2);

    let songs = svc.list_songs(.await?;
    assert!(songs.len() >= 4, "expected at least 4 songs (seeded + 1 saved), got {}", songs.len());

    svc.delete_song(SongId::from_uuid(seed_id("song-1")))
        .await?;
    let after_delete = svc
        .load_song(SongId::from_uuid(seed_id("song-1")))
        .await?;
    assert!(after_delete.is_none());
    Ok(())
}

#[tokio::test]
async fn test_live_load_song_variant() -> Result<()> {
    use signal_proto::song::{Section, SectionSource};

    let svc = seeded_service().await?;

    let verse = Section::from_patch(seed_id("sec-verse"), "Verse", seed_id("patch-clean"));
    let bridge = Section::from_rig_scene(
        seed_id("sec-bridge"),
        "Bridge",
        seed_id("rig-1"),
        seed_id("rs-ambient"),
    );
    let mut song = Song::new(seed_id("song-2"), "Instrumental", verse);
    song.add_section(bridge);
    svc.save_song(song).await?;

    let variant = svc
        .load_song_variant(
            &cx,
            SongId::from_uuid(seed_id("song-2")),
            SectionId::from_uuid(seed_id("sec-bridge")),
        )
        .await?;
    assert!(variant.is_some());
    let variant = variant.unwrap();
    assert_eq!(variant.name, "Bridge");
    match &variant.source {
        SectionSource::RigScene { rig_id, scene_id } => {
            assert_eq!(*rig_id, RigId::from_uuid(seed_id("rig-1")));
            assert_eq!(*scene_id, RigSceneId::from_uuid(seed_id("rs-ambient")));
        }
        _ => panic!("expected RigScene source"),
    }
    Ok(())
}

// endregion: --- Song service

// region: --- Browser service

#[tokio::test]
async fn test_live_browser_index_and_query() -> Result<()> {
    let svc = seeded_service().await?;

    let index: BrowserIndex = svc.browser_index(.await?;
    assert!(!index.entries().is_empty());
    assert!(index
        .entries()
        .iter()
        .any(|e| matches!(e.node.kind, BrowserEntityKind::SetlistCollection)));
    assert!(index
        .entries()
        .iter()
        .any(|e| matches!(e.node.kind, BrowserEntityKind::SetlistVariant)));

    let hits: Vec<BrowserHit> = svc
        .browse(
            &cx,
            BrowserQuery {
                include: vec!["tone:clean".to_string()],
                ..BrowserQuery::default()
            },
        )
        .await?;
    assert!(!hits.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_live_browser_query_strict_filters() -> Result<()> {
    let svc = seeded_service().await?;

    let setlist_hits: Vec<BrowserHit> = svc
        .browse(
            &cx,
            BrowserQuery {
                kinds: vec![BrowserEntityKind::SetlistCollection],
                text: Some("worship".to_string()),
                ..BrowserQuery::default()
            },
        )
        .await?;
    assert_eq!(setlist_hits.len(), 1);
    assert!(matches!(
        setlist_hits[0].node.kind,
        BrowserEntityKind::SetlistCollection
    ));

    let strict_keys_hits: Vec<BrowserHit> = svc
        .browse(
            &cx,
            BrowserQuery {
                rig_type: Some(signal_proto::rig::RigType::Keys),
                strict_rig_type: true,
                ..BrowserQuery::default()
            },
        )
        .await?;
    assert!(!strict_keys_hits.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_live_browser_index_load_time_smoke() -> Result<()> {
    let svc = seeded_service().await?;

    let started = Instant::now();
    let index: BrowserIndex = svc.browser_index(.await?;
    let elapsed = started.elapsed();

    assert!(!index.entries().is_empty());
    assert!(
        elapsed < Duration::from_secs(5),
        "browser index build exceeded smoke budget: {:?}",
        elapsed
    );
    Ok(())
}

#[tokio::test]
#[ignore = "performance benchmark; run manually to profile browser index build time"]
async fn test_live_browser_index_load_time_benchmark() -> Result<()> {
    let svc = seeded_service().await?;

    let iterations: usize = std::env::var("SIGNAL2_BROWSER_INDEX_BENCH_ITERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(25);
    let max_budget_ms: Option<u64> = std::env::var("SIGNAL2_BROWSER_INDEX_BENCH_MAX_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());

    let mut runs = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let index: BrowserIndex = svc.browser_index(.await?;
        runs.push(started.elapsed());
        assert!(!index.entries().is_empty());
    }

    let mut micros: Vec<u128> = runs.iter().map(|d| d.as_micros()).collect();
    micros.sort_unstable();
    let p95_idx = ((micros.len().saturating_sub(1)) * 95) / 100;
    let p95 = micros[p95_idx];
    let max = micros.last().copied().unwrap_or(0);
    let avg = micros.iter().sum::<u128>() / micros.len() as u128;

    println!(
        "browser-index benchmark: iterations={}, avg={}us, p95={}us, max={}us",
        iterations, avg, p95, max
    );

    if let Some(max_budget_ms) = max_budget_ms {
        let max_budget_us = (max_budget_ms as u128) * 1_000;
        assert!(
            p95 <= max_budget_us,
            "browser index p95 {}us exceeded configured budget {}us ({}ms)",
            p95,
            max_budget_us,
            max_budget_ms
        );
    }
    Ok(())
}

// endregion: --- Browser service

// region: --- Resolver service

#[tokio::test]
async fn test_live_resolve_rig_scene_keys_megarig() -> Result<()> {
    let svc = seeded_service().await?;

    let graph: ResolvedGraph = svc
        .resolve_target(
            &cx,
            ResolveTarget::RigScene {
                rig_id: RigId::from_uuid(seed_id("keys-megarig")),
                scene_id: RigSceneId::from_uuid(seed_id("keys-megarig-default")),
            },
        )
        .await
        .expect("resolve rig scene");

    assert_eq!(graph.rig_id.as_str(), seed_id("keys-megarig").to_string());
    assert!(!graph.engines.is_empty());
    assert!(!graph.effective_overrides.is_empty());
    assert!(graph
        .effective_overrides
        .iter()
        .any(|ov| matches!(ov.op, signal_proto::overrides::NodeOverrideOp::Set(_))));
    Ok(())
}

#[tokio::test]
async fn test_live_keys_megarig_load_time_smoke() -> Result<()> {
    let svc = seeded_service().await?;

    let started = Instant::now();
    let graph: ResolvedGraph = svc
        .resolve_target(
            &cx,
            ResolveTarget::RigScene {
                rig_id: RigId::from_uuid(seed_id("keys-megarig")),
                scene_id: RigSceneId::from_uuid(seed_id("keys-megarig-default")),
            },
        )
        .await
        .expect("resolve keys megarig");
    let elapsed = started.elapsed();

    assert_eq!(graph.rig_id.as_str(), seed_id("keys-megarig").to_string());
    assert!(
        elapsed < Duration::from_secs(5),
        "keys megarig load exceeded smoke budget: {:?}",
        elapsed
    );
    Ok(())
}

#[tokio::test]
#[ignore = "performance benchmark; run manually to profile Keys MegaRig load time"]
async fn test_live_keys_megarig_load_time_benchmark() -> Result<()> {
    let svc = seeded_service().await?;

    let iterations: usize = std::env::var("SIGNAL2_KEYS_MEGARIG_BENCH_ITERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(50);
    let max_budget_ms: Option<u64> = std::env::var("SIGNAL2_KEYS_MEGARIG_BENCH_MAX_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());

    let mut runs = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let graph: ResolvedGraph = svc
            .resolve_target(
                &cx,
                ResolveTarget::RigScene {
                    rig_id: RigId::from_uuid(seed_id("keys-megarig")),
                    scene_id: RigSceneId::from_uuid(seed_id("keys-megarig-default")),
                },
            )
            .await
            .expect("resolve keys megarig");
        runs.push(started.elapsed());
        assert_eq!(graph.rig_id.as_str(), seed_id("keys-megarig").to_string());
    }

    let mut micros: Vec<u128> = runs.iter().map(|d| d.as_micros()).collect();
    micros.sort_unstable();
    let p95_idx = ((micros.len().saturating_sub(1)) * 95) / 100;
    let p95 = micros[p95_idx];
    let max = micros.last().copied().unwrap_or(0);
    let avg = micros.iter().sum::<u128>() / micros.len() as u128;

    println!(
        "keys-megarig load benchmark: iterations={}, avg={}us, p95={}us, max={}us",
        iterations, avg, p95, max
    );

    if let Some(max_budget_ms) = max_budget_ms {
        let max_budget_us = (max_budget_ms as u128) * 1_000;
        assert!(
            p95 <= max_budget_us,
            "keys megarig p95 {}us exceeded configured budget {}us ({}ms)",
            p95,
            max_budget_us,
            max_budget_ms
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_live_resolve_song_section_from_patch() -> Result<()> {
    let svc = seeded_service().await?;

    let graph: ResolvedGraph = svc
        .resolve_target(
            &cx,
            ResolveTarget::SongSection {
                song_id: SongId::from_uuid(seed_id("feature-demo-song")),
                section_id: SectionId::from_uuid(seed_id("feature-demo-verse")),
            },
        )
        .await
        .expect("resolve song section");

    assert_eq!(graph.rig_id.as_str(), seed_id("keys-megarig").to_string());
    assert!(!graph.engines.is_empty());
    assert!(!graph.effective_overrides.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_live_resolve_applies_replace_ref_engine_scene() -> Result<()> {
    let svc = seeded_service().await?;

    let graph: ResolvedGraph = svc
        .resolve_target(
            &cx,
            ResolveTarget::SongSection {
                song_id: SongId::from_uuid(seed_id("feature-demo-song")),
                section_id: SectionId::from_uuid(seed_id("feature-demo-intro")),
            },
        )
        .await
        .expect("resolve song intro");

    let synth_engine = graph
        .engines
        .iter()
        .find(|e| e.engine_id.as_str() == seed_id("synth-engine").to_string())
        .expect("synth engine present");
    assert_eq!(
        synth_engine.engine_scene_id.as_str(),
        seed_id("synth-engine-scene-b").to_string()
    );
    Ok(())
}

#[tokio::test]
async fn test_live_resolve_fails_on_missing_replace_ref_module_variant() -> Result<()> {
    use signal_proto::overrides::{NodeOverrideOp, NodePath, Override};
    use signal_proto::song::Section;

    let svc = seeded_service().await?;

    let bad = Section::from_rig_scene(
        seed_id("bad-replace-ref-section"),
        "Bad ReplaceRef",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-default"),
    )
    .with_override(Override {
        path: NodePath::engine("synth-engine")
            .with_layer("synth-layer-motion")
            .with_module("time-parallel"),
        op: NodeOverrideOp::ReplaceRef("does-not-exist".to_string()),
    });

    let song = Song::new(seed_id("bad-replace-ref-song"), "Bad ReplaceRef Song", bad);
    svc.save_song(song).await?;

    let resolved: core::result::Result<ResolvedGraph, ResolveError> = svc
        .resolve_target(
            &cx,
            ResolveTarget::SongSection {
                song_id: SongId::from_uuid(seed_id("bad-replace-ref-song")),
                section_id: SectionId::from_uuid(seed_id("bad-replace-ref-section")),
            },
        )
        .await;

    assert!(resolved.is_err());
    let err = resolved.err().expect("expected resolve error");
    assert!(matches!(err, ResolveError::InvalidReference(_)));
    Ok(())
}

#[tokio::test]
async fn test_live_resolve_fails_on_missing_replace_ref_engine_scene() -> Result<()> {
    use signal_proto::overrides::{NodeOverrideOp, NodePath, Override};
    use signal_proto::song::Section;

    let svc = seeded_service().await?;

    let bad = Section::from_rig_scene(
        seed_id("bad-replace-ref-engine-section"),
        "Bad Engine ReplaceRef",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-default"),
    )
    .with_override(Override {
        path: NodePath::engine("synth-engine"),
        op: NodeOverrideOp::ReplaceRef("does-not-exist-engine-scene".to_string()),
    });

    let song = Song::new(
        seed_id("bad-replace-ref-engine-song"),
        "Bad Engine ReplaceRef Song",
        bad,
    );
    svc.save_song(song).await?;

    let resolved: core::result::Result<ResolvedGraph, ResolveError> = svc
        .resolve_target(
            &cx,
            ResolveTarget::SongSection {
                song_id: SongId::from_uuid(seed_id("bad-replace-ref-engine-song")),
                section_id: SectionId::from_uuid(seed_id("bad-replace-ref-engine-section")),
            },
        )
        .await;

    assert!(resolved.is_err());
    let err = resolved.err().expect("expected resolve error");
    assert!(matches!(err, ResolveError::InvalidReference(_)));
    Ok(())
}

#[tokio::test]
async fn test_live_resolve_fails_on_missing_replace_ref_layer_variant() -> Result<()> {
    use signal_proto::overrides::{NodeOverrideOp, NodePath, Override};
    use signal_proto::song::Section;

    let svc = seeded_service().await?;

    let bad = Section::from_rig_scene(
        seed_id("bad-replace-ref-layer-section"),
        "Bad Layer ReplaceRef",
        seed_id("keys-megarig"),
        seed_id("keys-megarig-default"),
    )
    .with_override(Override {
        path: NodePath::engine("synth-engine").with_layer("synth-layer-motion"),
        op: NodeOverrideOp::ReplaceRef("does-not-exist-layer-variant".to_string()),
    });

    let song = Song::new(
        seed_id("bad-replace-ref-layer-song"),
        "Bad Layer ReplaceRef Song",
        bad,
    );
    svc.save_song(song).await?;

    let resolved: core::result::Result<ResolvedGraph, ResolveError> = svc
        .resolve_target(
            &cx,
            ResolveTarget::SongSection {
                song_id: SongId::from_uuid(seed_id("bad-replace-ref-layer-song")),
                section_id: SectionId::from_uuid(seed_id("bad-replace-ref-layer-section")),
            },
        )
        .await;

    assert!(resolved.is_err());
    let err = resolved.err().expect("expected resolve error");
    assert!(matches!(err, ResolveError::InvalidReference(_)));
    Ok(())
}

// endregion: --- Resolver service

// region: --- Block / Module resolution (daw_block_ops)

#[tokio::test]
async fn test_resolve_block_load_eq_proq4() -> Result<()> {
    let svc = seeded_service().await?;
    let preset_id = PresetId::from(seed_id("eq-proq4"));

    let resolved = svc
        .resolve_block_load(BlockType::Eq, &preset_id, None)
        .await
        .expect("resolve_block_load should succeed");

    assert_eq!(resolved.plugin_name, "CLAP: Pro-Q 4 (FabFilter)");
    assert_eq!(resolved.block.parameters().len(), 6);
    assert!(
        resolved.display_name.contains("EQ"),
        "display_name should contain 'EQ', got '{}'",
        resolved.display_name
    );
    assert!(
        resolved.display_name.contains("Pro-Q 4"),
        "display_name should contain 'Pro-Q 4', got '{}'",
        resolved.display_name
    );
    assert!(resolved.overrides.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_resolve_block_load_snapshot_idx() -> Result<()> {
    let svc = seeded_service().await?;
    let preset_id = PresetId::from(seed_id("eq-proq4"));

    // Non-default snapshot should have distinct gain values.
    // Find "Surgical Cut" by ID to avoid index ordering issues after DB roundtrip.
    let presets = svc
        .block_repo
        .list_block_collections(BlockType::Eq)
        .await
        .unwrap();
    let proq4 = presets.iter().find(|p| p.name() == "Pro-Q 4").unwrap();
    let surgical_id = proq4
        .snapshots()
        .iter()
        .find(|s| s.name() == "Surgical Cut")
        .map(|s| s.id().clone())
        .expect("Surgical Cut snapshot should exist");

    let resolved = svc
        .resolve_block_load(BlockType::Eq, &preset_id, Some(&surgical_id))
        .await
        .expect("resolve_block_load for Surgical Cut should succeed");

    // Surgical Cut has a narrow mid cut (0.28), different from default (0.50)
    let params = resolved.block.parameters();
    let mid = params.iter().find(|p| p.id() == "mid").unwrap();
    assert!(
        mid.value().get() < 0.35,
        "Surgical Cut mid should be < 0.35, got {}",
        mid.value().get()
    );
    Ok(())
}

#[tokio::test]
async fn test_resolve_block_load_missing_preset() -> Result<()> {
    let svc = seeded_service().await?;
    let bad_id = PresetId::from(seed_id("nonexistent-preset"));

    let result = svc
        .resolve_block_load(BlockType::Eq, &bad_id, None)
        .await;

    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("Preset not found"),
        "error should mention 'Preset not found'"
    );
    Ok(())
}

#[tokio::test]
async fn test_resolve_module_load_3band() -> Result<()> {
    let svc = seeded_service().await?;
    let preset_id = ModulePresetId::from_uuid(seed_id("eq-proq4-3band"));

    let resolved = svc
        .resolve_module_load(ModuleType::Eq, &preset_id, 0)
        .await
        .expect("resolve_module_load should succeed");

    let fx_loads = resolved.fx_loads();
    assert_eq!(fx_loads.len(), 3);
    assert!(
        resolved.display_name.contains("EQ"),
        "module display_name should contain 'EQ', got '{}'",
        resolved.display_name
    );
    assert!(
        resolved.display_name.contains("Pro-Q 4 - 3-Band"),
        "module display_name should contain 'Pro-Q 4 - 3-Band', got '{}'",
        resolved.display_name
    );

    // Each block should resolve to the Pro-Q 4 plugin with distinct snapshots
    for (i, fx_load) in fx_loads.iter().enumerate() {
        assert_eq!(
            fx_load.plugin_name, "CLAP: Pro-Q 4 (FabFilter)",
            "block {} should use Pro-Q 4",
            i
        );
        assert!(
            fx_load.display_name.contains("EQ"),
            "block {} display_name should contain 'EQ', got '{}'",
            i, fx_load.display_name
        );
    }

    // Verify each block's parameters match the corresponding snapshot values.
    // Parameter order: low, low_mid, mid, high_mid, high, output
    let params: Vec<Vec<f32>> = fx_loads
        .iter()
        .map(|fl| fl.block.parameters().iter().map(|p: &signal_proto::BlockParameter| p.value().get()).collect())
        .collect();

    // Block 0: Surgical Cut — proq4_block(0.50, 0.50, 0.28, 0.50, 0.50, 0.50)
    assert!((params[0][2] - 0.28).abs() < 0.001, "Surgical Cut mid should be 0.28, got {}", params[0][2]);

    // Block 1: Hi-Fi — proq4_block(0.58, 0.50, 0.50, 0.52, 0.64, 0.50)
    assert!((params[1][0] - 0.58).abs() < 0.001, "Hi-Fi low should be 0.58, got {}", params[1][0]);
    assert!((params[1][4] - 0.64).abs() < 0.001, "Hi-Fi high should be 0.64, got {}", params[1][4]);

    // Block 2: Warm Analog — proq4_block(0.65, 0.55, 0.50, 0.48, 0.38, 0.50)
    assert!((params[2][0] - 0.65).abs() < 0.001, "Warm Analog low should be 0.65, got {}", params[2][0]);
    assert!((params[2][4] - 0.38).abs() < 0.001, "Warm Analog high should be 0.38, got {}", params[2][4]);

    // All 3 should be distinct from each other
    assert_ne!(params[0], params[1], "Surgical Cut and Hi-Fi should differ");
    assert_ne!(params[1], params[2], "Hi-Fi and Warm Analog should differ");
    Ok(())
}

#[tokio::test]
async fn test_resolve_module_load_missing_source_tag() -> Result<()> {
    let svc = seeded_service().await?;
    let preset_id = ModulePresetId::from_uuid(seed_id("drive-full-stack"));

    // drive-full-stack references block presets (boost-ep, etc.) that lack
    // `source:` metadata tags, so resolution should fail with a clear error.
    let result = svc
        .resolve_module_load(ModuleType::Drive, &preset_id, 0)
        .await;

    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("no source: tag"),
        "error should mention missing source: tag"
    );
    Ok(())
}

#[tokio::test]
async fn test_resolve_module_load_missing() -> Result<()> {
    let svc = seeded_service().await?;
    let bad_id = ModulePresetId::from_uuid(seed_id("nonexistent-module"));

    let result = svc
        .resolve_module_load(ModuleType::Eq, &bad_id, 0)
        .await;

    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("Module preset not found"),
        "error should mention 'Module preset not found'"
    );
    Ok(())
}

// endregion: --- Block / Module resolution (daw_block_ops)

// region: --- Parallel signal chain resolution (daw_block_ops)

/// Verify that `resolve_module_load` preserves `Split` node topology.
///
/// Creates a synthetic module with a serial EQ block followed by a parallel
/// `Split` containing two EQ lanes. EQ presets have `source:` tags so
/// resolution succeeds in-memory without a running DAW.
///
/// Expected resolved topology:
///   chain.nodes = [Fx(eq-reaeq), Split([lane_a: Fx(eq-proq4), lane_b: Fx(eq-reaeq)])]
///
/// The flat `fx_loads()` view should see 3 blocks total.
#[tokio::test]
async fn test_resolve_module_load_parallel_topology() -> Result<()> {
    use crate::daw_block_ops::ResolvedSignalNode;
    use signal_proto::{
        Module, ModuleBlock, ModuleBlockSource, ModulePreset, ModuleSnapshot, PresetId, SignalChain, SignalNode,
    };

    let svc = seeded_service().await?;

    // Build a synthetic module:
    //   Block(reaeq) → Split([Block(pro-q4) | Block(reaeq)])
    // Uses EQ presets which have `source:` metadata tags.
    let reaeq_id = PresetId::from_uuid(seed_id("eq-reaeq"));
    let proq4_id = PresetId::from_uuid(seed_id("eq-proq4"));

    let chain = SignalChain::new(vec![
        SignalNode::Block(ModuleBlock::new(
            "eq-serial",
            "Serial EQ",
            BlockType::Eq,
            ModuleBlockSource::PresetDefault {
                preset_id: reaeq_id.clone(),
                saved_at_version: None,
            },
        )),
        SignalNode::Split {
            lanes: vec![
                SignalChain::serial(vec![ModuleBlock::new(
                    "eq-lane-a",
                    "Pro-Q Lane",
                    BlockType::Eq,
                    ModuleBlockSource::PresetDefault {
                        preset_id: proq4_id.clone(),
                        saved_at_version: None,
                    },
                )]),
                SignalChain::serial(vec![ModuleBlock::new(
                    "eq-lane-b",
                    "ReaEQ Lane",
                    BlockType::Eq,
                    ModuleBlockSource::PresetDefault {
                        preset_id: reaeq_id.clone(),
                        saved_at_version: None,
                    },
                )]),
                SignalChain::new(vec![]), // empty placeholder lane
            ],
        },
    ]);

    let test_preset_id = ModulePresetId::from_uuid(seed_id("test-parallel-eq-module"));
    let module_preset = ModulePreset::new(
        seed_id("test-parallel-eq-module"),
        "Test Parallel EQ",
        ModuleType::Eq,
        ModuleSnapshot::new(
            seed_id("test-parallel-eq-default"),
            "Default",
            Module::from_chain(chain),
        ),
        vec![],
    );
    svc.save_module_collection(module_preset).await?;

    let resolved = svc
        .resolve_module_load(ModuleType::Eq, &test_preset_id, 0)
        .await
        .expect("resolve_module_load should succeed for parallel EQ module");

    // Flat view: 1 serial + 2 in split = 3 blocks total
    let flat = resolved.fx_loads();
    assert_eq!(flat.len(), 3, "expected 3 leaf FX loads (1 serial + 2 in split)");

    // Topology: top-level chain must have exactly 2 nodes: Fx then Split
    let top_nodes = &resolved.chain.nodes;
    assert_eq!(top_nodes.len(), 2, "chain should have 2 top-level nodes (Fx + Split)");

    assert!(
        matches!(top_nodes[0], ResolvedSignalNode::Fx(_)),
        "first top-level node should be Fx"
    );

    match &top_nodes[1] {
        ResolvedSignalNode::Split(lanes) => {
            // Resolve phase preserves all 3 lanes (including empty placeholder).
            assert_eq!(lanes.len(), 3, "Split should have 3 resolved lanes");

            // Lane 0 and 1: each have exactly 1 Fx node.
            for j in 0..2 {
                assert_eq!(lanes[j].nodes.len(), 1, "lane {j} should have 1 Fx node");
                assert!(
                    matches!(lanes[j].nodes[0], ResolvedSignalNode::Fx(_)),
                    "lane {j} node should be Fx"
                );
            }

            // Lane 2: empty placeholder preserved by resolve.
            assert!(lanes[2].nodes.is_empty(), "lane 2 (placeholder) should be empty");
        }
        ResolvedSignalNode::Fx(_) => panic!("second top-level node should be Split, not Fx"),
    }

    // Verify display name includes module type.
    assert!(
        resolved.display_name.contains("EQ"),
        "module display_name should contain 'EQ', got '{}'",
        resolved.display_name
    );

    Ok(())
}

// endregion: --- Parallel signal chain resolution (daw_block_ops)

// region: --- Rig / Layer structure resolution

#[tokio::test]
async fn test_resolve_guitar_layer_structure() -> Result<()> {
    let svc = seeded_service().await?;

    let graph: ResolvedGraph = svc
        .resolve_target(
            &cx,
            ResolveTarget::RigScene {
                rig_id: RigId::from_uuid(seed_id("guitar-megarig")),
                scene_id: RigSceneId::from_uuid(seed_id("guitar-megarig-default")),
            },
        )
        .await
        .expect("resolve guitar megarig default scene");

    // Guitar rig should have at least 1 engine
    assert!(
        !graph.engines.is_empty(),
        "guitar megarig should have at least one engine"
    );

    let engine = &graph.engines[0];
    assert!(
        !engine.layers.is_empty(),
        "guitar engine should have at least one layer"
    );

    // Each layer should have modules and/or standalone blocks
    for layer in &engine.layers {
        let total = layer.modules.len() + layer.standalone_blocks.len();
        assert!(
            total > 0,
            "layer {:?} should have modules or standalone blocks",
            layer.layer_id
        );

        // Every module should have at least one block
        for module in &layer.modules {
            assert!(
                !module.blocks.is_empty(),
                "module {:?} should have at least one block",
                module.source_preset_id
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_resolve_keys_layer_structure() -> Result<()> {
    let svc = seeded_service().await?;

    let graph: ResolvedGraph = svc
        .resolve_target(
            &cx,
            ResolveTarget::RigScene {
                rig_id: RigId::from_uuid(seed_id("keys-megarig")),
                scene_id: RigSceneId::from_uuid(seed_id("keys-megarig-default")),
            },
        )
        .await
        .expect("resolve keys megarig default scene");

    // Keys rig has multiple engines (keys, synth, organ, pad)
    assert!(
        graph.engines.len() >= 2,
        "keys megarig should have multiple engines, got {}",
        graph.engines.len()
    );

    // Collect all layers across all engines
    let total_layers: usize = graph.engines.iter().map(|e| e.layers.len()).sum();
    assert!(
        total_layers >= 2,
        "keys megarig should have at least 2 layers total, got {}",
        total_layers
    );

    Ok(())
}

// endregion: --- Rig / Layer structure resolution
