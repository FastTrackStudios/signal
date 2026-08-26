use super::*;
use signal_proto::{seed_id, ModuleType};
use signal_storage::{
    runtime_seed_bundle, BlockRepoLive, Database, EngineRepoLive, LayerRepoLive, ModuleRepoLive,
    ProfileRepoLive, RackRepoLive, RigRepoLive, SceneTemplateRepoLive, SetlistRepoLive,
    SongRepoLive,
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
        SceneTemplateRepoLive,
        RackRepoLive,
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
    let returned = svc.set_block(BlockType::Drive, new_block.clone()).await?;

    // -- Check
    assert_eq!(returned, new_block);
    let loaded = svc.get_block(BlockType::Drive).await?;
    assert_eq!(loaded, new_block);
    Ok(())
}

// endregion: --- get_block / set_block

// region: --- Block collections (list / load)

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
async fn test_live_cross_collection_load_updates_correct_block_type() -> Result<()> {
    // -- Setup & Fixtures
    let svc = seeded_service().await?;

    // -- Exec: load an amp variant
    let amp_before = svc.get_block(BlockType::Amp).await?;
    let _drive = svc
        .load_block_preset(
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

// endregion: --- Layer service

// region: --- Preset (rig) service

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
async fn test_live_browser_index_load_time_smoke() -> Result<()> {
    let svc = seeded_service().await?;

    let started = Instant::now();
    let index: BrowserIndex = svc.browser_index().await?;
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
        let index: BrowserIndex = svc.browser_index().await?;
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

// region: --- Block / Module resolution (daw_block_ops)

#[tokio::test]
async fn test_resolve_block_load_missing_preset() -> Result<()> {
    let svc = seeded_service().await?;
    let bad_id = PresetId::from(seed_id("nonexistent-preset"));

    let result = svc.resolve_block_load(BlockType::Eq, &bad_id, None).await;

    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("Preset not found"),
        "error should mention 'Preset not found'"
    );
    Ok(())
}

#[tokio::test]
async fn test_resolve_module_load_missing() -> Result<()> {
    let svc = seeded_service().await?;
    let bad_id = ModulePresetId::from_uuid(seed_id("nonexistent-module"));

    let result = svc.resolve_module_load(ModuleType::Eq, &bad_id, 0).await;

    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("Module preset not found"),
        "error should mention 'Module preset not found'"
    );
    Ok(())
}

// endregion: --- Block / Module resolution (daw_block_ops)
