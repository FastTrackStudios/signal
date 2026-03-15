//! REAPER integration tests: Load block + module presets onto real DAW tracks.
//!
//! Tests the full resolve → add FX → apply params → rename pipeline against
//! a running REAPER instance with the FabFilter Pro-Q 4 CLAP plugin.
//!
//! The Pro-Q 4 seed presets use real CLAP parameter names (via `daw_name`),
//! so the param-by-param application works against the actual plugin.
//!
//! Run with:
//!   cargo xtask reaper-test signal_load

use std::time::Duration;

use reaper_test::reaper_test;
use signal::{seed_id, BlockType, ModulePresetId, ModuleType, PresetId};
use signal_proto::plugin_block::TrackRole;

/// Small sleep to let REAPER/CLAP process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Ensure REAPER's audio engine is running (required for CLAP param changes).
async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// ---------------------------------------------------------------------------
// Test: Full load of a single EQ block preset (resolve + add + params + rename)
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn signal_load_block_to_track(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Bootstrap in-memory signal controller
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let svc = signal.service();

    // Create a test track
    let track = project.tracks().add("Block Load Test", None).await?;
    settle().await;

    // Full pipeline: resolve → add FX → apply params → rename
    let result = svc
        .load_block_to_track(
            BlockType::Eq,
            &PresetId::from(seed_id("eq-proq4")),
            None, // default snapshot
            &track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    settle().await;

    // Verify: FX was added
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(fx_count, 1, "should have exactly 1 FX on the track");

    // Verify: display name follows Signal naming convention
    assert!(
        result.display_name.contains("EQ"),
        "display_name should contain 'EQ', got '{}'",
        result.display_name
    );
    assert!(
        result.display_name.contains("Pro-Q 4"),
        "display_name should contain 'Pro-Q 4', got '{}'",
        result.display_name
    );

    // Verify: FX was renamed in REAPER
    let fx = track
        .fx_chain()
        .by_index(0)
        .await?
        .ok_or_else(|| eyre::eyre!("FX not found at index 0"))?;
    let info = fx.info().await?;
    assert!(
        info.name.contains("EQ"),
        "REAPER FX name should contain 'EQ', got '{}'",
        info.name
    );

    // Verify: FX GUID was returned
    assert!(!result.fx_guid.is_empty(), "fx guid should not be empty");

    ctx.log("signal_load_block_to_track: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Load two modules onto the same track, each inside its own FX container
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn signal_load_two_modules_to_track(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Bootstrap in-memory signal controller
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let svc = signal.service();

    // Create a test track
    let track = project.tracks().add("Two Modules Test", None).await?;
    settle().await;

    // Load first module: EQ 3-Band (3 blocks: Surgical Cut, Hi-Fi, Warm Analog)
    let result_1 = svc
        .load_module_to_track(
            ModuleType::Eq,
            &ModulePresetId::from_uuid(seed_id("eq-proq4-3band")),
            0,
            &track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    settle().await;

    // Load second module: EQ 4-Band Full (4 blocks: Surgical Cut, Warm Analog, Hi-Fi, Bright Mix)
    let result_2 = svc
        .load_module_to_track(
            ModuleType::Eq,
            &ModulePresetId::from_uuid(seed_id("eq-proq4-4band")),
            0,
            &track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;

    settle().await;

    // Verify: module 1 loaded 3 FX, module 2 loaded 4 FX
    assert_eq!(result_1.loaded_fx.len(), 3, "module 1 should have 3 FX");
    assert_eq!(result_2.loaded_fx.len(), 4, "module 2 should have 4 FX");

    // Verify: FX tree has 2 top-level containers
    let tree = track.fx_chain().tree().await?;
    assert_eq!(
        tree.nodes.len(),
        2,
        "should have 2 top-level containers, got {}",
        tree.nodes.len()
    );

    // Verify: first container is 3-Band with 3 children
    match &tree.nodes[0].kind {
        daw_control::FxNodeKind::Container { name, children, .. } => {
            assert!(
                name.contains("3-Band"),
                "container 1 name should contain '3-Band', got '{name}'"
            );
            assert_eq!(children.len(), 3, "container 1 should have 3 children");
        }
        _ => panic!("first top-level node should be a container"),
    }

    // Verify: second container is 4-Band Full with 4 children
    match &tree.nodes[1].kind {
        daw_control::FxNodeKind::Container { name, children, .. } => {
            assert!(
                name.contains("4-Band"),
                "container 2 name should contain '4-Band', got '{name}'"
            );
            assert_eq!(children.len(), 4, "container 2 should have 4 children");
        }
        _ => panic!("second top-level node should be a container"),
    }

    ctx.log("signal_load_two_modules_to_track: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Layer structure — track with modules + standalone block, [L] prefix
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn signal_load_layer_to_track(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Bootstrap in-memory signal controller
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let svc = signal.service();

    // Create a track named as a layer — this IS the layer in REAPER terms
    let layer_name = TrackRole::Layer {
        name: "Guitar Main".to_string(),
    }
    .display_name();
    let track = project.tracks().add(&layer_name, None).await?;
    settle().await;

    // Load module 1: EQ 3-Band (3 blocks in a container)
    let mod1 = svc
        .load_module_to_track(
            ModuleType::Eq,
            &ModulePresetId::from_uuid(seed_id("eq-proq4-3band")),
            0,
            &track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    settle().await;

    // Load module 2: EQ 4-Band Full (4 blocks in a container)
    let mod2 = svc
        .load_module_to_track(
            ModuleType::Eq,
            &ModulePresetId::from_uuid(seed_id("eq-proq4-4band")),
            0,
            &track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    settle().await;

    // Load standalone block: EQ Pro-Q 4
    let blk = svc
        .load_block_to_track(
            BlockType::Eq,
            &PresetId::from(seed_id("eq-proq4")),
            None,
            &track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    settle().await;

    // Verify: module results
    assert_eq!(mod1.loaded_fx.len(), 3, "module 1 should have 3 FX");
    assert_eq!(mod2.loaded_fx.len(), 4, "module 2 should have 4 FX");
    assert!(
        blk.display_name.contains("[B]"),
        "standalone block should have [B] prefix, got '{}'",
        blk.display_name
    );

    // Verify: FX tree has 3 top-level nodes (2 module containers + 1 standalone block)
    let tree = track.fx_chain().tree().await?;
    assert_eq!(
        tree.nodes.len(),
        3,
        "should have 3 top-level nodes (2 modules + 1 block), got {}",
        tree.nodes.len()
    );

    // Node 0: Module container [M] with 3 children (3-Band)
    match &tree.nodes[0].kind {
        daw_control::FxNodeKind::Container { name, children, .. } => {
            assert!(
                name.contains("[M]"),
                "module container should have [M] prefix, got '{name}'"
            );
            assert!(
                name.contains("3-Band"),
                "container 1 should be 3-Band, got '{name}'"
            );
            assert_eq!(children.len(), 3, "3-Band module should have 3 children");
        }
        _ => panic!("node 0 should be a container"),
    }

    // Node 1: Module container [M] with 4 children (4-Band)
    match &tree.nodes[1].kind {
        daw_control::FxNodeKind::Container { name, children, .. } => {
            assert!(
                name.contains("[M]"),
                "module container should have [M] prefix, got '{name}'"
            );
            assert!(
                name.contains("4-Band"),
                "container 2 should be 4-Band, got '{name}'"
            );
            assert_eq!(children.len(), 4, "4-Band module should have 4 children");
        }
        _ => panic!("node 1 should be a container"),
    }

    // Node 2: Standalone block [B] (leaf FX plugin)
    match &tree.nodes[2].kind {
        daw_control::FxNodeKind::Plugin(fx) => {
            assert!(
                fx.name.contains("[B]"),
                "standalone block should have [B] prefix, got '{}'",
                fx.name
            );
            assert!(
                fx.name.contains("Pro-Q 4"),
                "standalone block should reference Pro-Q 4, got '{}'",
                fx.name
            );
        }
        _ => panic!("node 2 should be a plugin (standalone block)"),
    }

    ctx.log("signal_load_layer_to_track: PASS");
    Ok(())
}
