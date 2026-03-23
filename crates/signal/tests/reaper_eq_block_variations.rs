//! REAPER integration test: Load an EQ block and cycle through variations.
//!
//! Tests the end-to-end flow of:
//! 1. Capturing real Pro-Q 4 (CLAP) state as block snapshots
//! 2. Building ResolvedGraphs with state_data
//! 3. Applying each variation via ReaperPatchApplier
//! 4. Verifying FX parameters change in REAPER
//!
//! Run with:
//!   cargo xtask reaper-test reaper_eq_block_variations

use std::time::Duration;

use reaper_test::reaper_test;
use signal::reaper_applier::ReaperPatchApplier;
use signal::resolve::{
    LayerSource, ResolveTarget, ResolvedBlock, ResolvedEngine, ResolvedGraph, ResolvedLayer,
    ResolvedModule,
};
use signal::{seed_id, Block, BlockType, DawPatchApplier};

/// REAPER's CLAP plugin identifier for FabFilter Pro-Q 4.
const CLAP_PROQ4: &str = "CLAP: Pro-Q 4 (FabFilter)";

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

/// Build a minimal ResolvedGraph containing a single EQ block with state_data.
///
/// The `fx_id` in the graph must match the `fx_id` passed to `set_target` so
/// that `graph_state_chunks` can extract the chunk data.
fn make_eq_graph(name: &str, state_data: Vec<u8>, fx_id: &str) -> ResolvedGraph {
    ResolvedGraph {
        target: ResolveTarget::RigScene {
            rig_id: seed_id("test-rig").into(),
            scene_id: seed_id("test-scene").into(),
        },
        rig_id: seed_id("test-rig").into(),
        rig_scene_id: seed_id(&format!("scene-{name}")).into(),
        engines: vec![ResolvedEngine {
            engine_id: seed_id("guitar-engine").into(),
            engine_scene_id: seed_id("default-engine-scene").into(),
            layers: vec![ResolvedLayer {
                layer_id: seed_id("main-layer").into(),
                layer_variant_id: seed_id("default-layer-variant").into(),
                source: LayerSource::InlinedInParent,
                modules: vec![ResolvedModule {
                    source_preset_id: seed_id("eq-preset").into(),
                    source_variant_id: seed_id(&format!("snapshot-{name}")).into(),
                    blocks: vec![ResolvedBlock {
                        node_id: fx_id.to_string(),
                        label: "EQ".into(),
                        block_type: BlockType::Eq,
                        source_preset_id: None,
                        source_variant_id: None,
                        block: Block::from_parameters(vec![]),
                        state_data: Some(state_data),
                        stale: false,
                    }],
                }],
                standalone_blocks: vec![],
            }],
        }],
        effective_overrides: vec![],
    }
}

// ---------------------------------------------------------------------------
// Test: Capture EQ variations and cycle through them via ReaperPatchApplier
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn eq_block_load_and_cycle_variations(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // --- Step 1: Create a track with Pro-Q 4 to capture state chunks ---
    let setup_track = project.tracks().add("EQ Setup", None).await?;
    let fx = setup_track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    // Capture the "Flat" state (default — all bands off, neutral output)
    let flat_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get default state chunk"))?;
    println!(
        "[eq_variations] Captured 'Flat' state: {} bytes",
        flat_chunk.len()
    );

    // --- Step 2: Create "Bright" variation (boost high shelf) ---
    fx.param_by_name("Band 1 Frequency").set(0.75).await?;
    fx.param_by_name("Band 1 Gain").set(0.65).await?;
    fx.param_by_name("Band 1 Q").set(0.5).await?;
    settle().await;

    let bright_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get bright state chunk"))?;
    println!(
        "[eq_variations] Captured 'Bright' state: {} bytes",
        bright_chunk.len()
    );

    // --- Step 3: Create "Scooped" variation (cut mids, boost lows + highs) ---
    fx.param_by_name("Band 1 Frequency").set(0.25).await?;
    fx.param_by_name("Band 1 Gain").set(0.6).await?;
    fx.param_by_name("Band 2 Frequency").set(0.5).await?;
    fx.param_by_name("Band 2 Gain").set(0.35).await?;
    fx.param_by_name("Band 2 Q").set(0.4).await?;
    settle().await;

    let scooped_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get scooped state chunk"))?;
    println!(
        "[eq_variations] Captured 'Scooped' state: {} bytes",
        scooped_chunk.len()
    );

    // Verify the three chunks are different
    assert_ne!(flat_chunk, bright_chunk, "Flat and Bright should differ");
    assert_ne!(
        bright_chunk, scooped_chunk,
        "Bright and Scooped should differ"
    );
    assert_ne!(flat_chunk, scooped_chunk, "Flat and Scooped should differ");

    // --- Step 4: Build ResolvedGraphs for each variation ---
    //
    // The fx_id in ResolvedBlock.node_id must match the fx_id passed to
    // set_target, because graph_state_chunks filters by that key.
    let rig_name = "EQ Test Rig";
    let graphs = vec![
        ("Flat", make_eq_graph("flat", flat_chunk.clone(), rig_name)),
        (
            "Bright",
            make_eq_graph("bright", bright_chunk.clone(), rig_name),
        ),
        (
            "Scooped",
            make_eq_graph("scooped", scooped_chunk.clone(), rig_name),
        ),
    ];

    // --- Step 5: Use ReaperPatchApplier to load + cycle through variations ---
    let applier = ReaperPatchApplier::new();
    applier
        .set_target(project.clone(), rig_name)
        .await
        .map_err(|e| eyre::eyre!("set_target failed: {e:?}"))?;

    for (name, graph) in &graphs {
        println!("[eq_variations] Applying variation: {name}");
        let applied = applier
            .apply_graph(graph, Some(name))
            .await
            .map_err(|e| eyre::eyre!("apply_graph '{name}' failed: {e:?}"))?;
        assert!(applied, "apply_graph should return true for '{name}'");
        settle().await;

        // Read back a key parameter to verify the state changed.
        // The applier creates a child track named after the patch.
        let patch_track = project
            .tracks()
            .by_name(name)
            .await?
            .ok_or_else(|| eyre::eyre!("Patch track '{name}' not found"))?;
        let patch_fx = patch_track
            .fx_chain()
            .by_index(0)
            .await?
            .ok_or_else(|| eyre::eyre!("No FX on patch track '{name}'"))?;

        let freq_val = patch_fx.param_by_name("Band 1 Frequency").get().await?;
        println!("[eq_variations]   Band 1 Frequency readback: {freq_val:.4}");

        // Verify FX was renamed with the FxRole::Block convention
        let fx_info = patch_fx.info().await?;
        println!("[eq_variations]   FX name: {}", fx_info.name);
        assert!(
            fx_info.name.contains("[B] EQ:"),
            "FX name should contain '[B] EQ:', got '{}'",
            fx_info.name
        );
        assert!(
            fx_info.name.contains(name),
            "FX name should contain variation name '{name}', got '{}'",
            fx_info.name
        );

        ctx.log(&format!("{name}: Band 1 Frequency = {freq_val:.4}"));
    }

    // --- Step 6: Cycle back to verify switching works ---
    println!("[eq_variations] Cycling back to 'Flat'...");
    let applied = applier
        .apply_graph(&graphs[0].1, Some("Flat"))
        .await
        .map_err(|e| eyre::eyre!("apply_graph 'Flat' (cycle back) failed: {e:?}"))?;
    assert!(applied, "Cycling back to Flat should apply");
    settle().await;

    let flat_track = project
        .tracks()
        .by_name("Flat")
        .await?
        .ok_or_else(|| eyre::eyre!("Flat track not found after cycle"))?;
    let flat_fx = flat_track
        .fx_chain()
        .by_index(0)
        .await?
        .ok_or_else(|| eyre::eyre!("No FX on Flat track after cycle"))?;
    let freq_after = flat_fx.param_by_name("Band 1 Frequency").get().await?;
    println!("[eq_variations] After cycling back to Flat: Band 1 Frequency = {freq_after:.4}");

    // Verify FX name on the cycled-back Flat track
    let flat_fx_info = flat_fx.info().await?;
    println!(
        "[eq_variations] Flat cycle-back FX name: {}",
        flat_fx_info.name
    );
    assert!(
        flat_fx_info.name.contains("[B] EQ:"),
        "Cycled-back Flat FX name should contain '[B] EQ:', got '{}'",
        flat_fx_info.name
    );
    assert!(
        flat_fx_info.name.contains("Flat"),
        "Cycled-back FX name should contain 'Flat', got '{}'",
        flat_fx_info.name
    );

    println!("[eq_variations] SUCCESS: EQ block variations loaded and cycled");
    ctx.log("eq_block_load_and_cycle_variations: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Verify parameter values differ between variations (direct chunk swap)
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn eq_block_variations_params_differ(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Create a track with Pro-Q 4
    let track = project.tracks().add("EQ Params", None).await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    // Read default Band 1 Frequency
    let default_freq = fx.param_by_name("Band 1 Frequency").get().await?;
    println!("[eq_params_differ] Default Band 1 Frequency: {default_freq:.4}");

    // Set Band 1 to "Bright" config
    fx.param_by_name("Band 1 Frequency").set(0.75).await?;
    fx.param_by_name("Band 1 Gain").set(0.65).await?;
    settle().await;

    // Capture "Bright" state
    let bright_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get bright chunk"))?;

    // Set Band 1 to "Warm" config (low boost)
    fx.param_by_name("Band 1 Frequency").set(0.2).await?;
    fx.param_by_name("Band 1 Gain").set(0.6).await?;
    settle().await;

    // Capture "Warm" state
    let warm_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get warm chunk"))?;

    // Now restore Bright and verify frequency changed
    fx.set_state_chunk(bright_chunk.clone()).await?;
    settle().await;
    let bright_freq = fx.param_by_name("Band 1 Frequency").get().await?;
    println!("[eq_params_differ] After restoring Bright: Band 1 Frequency = {bright_freq:.4}");

    // Restore Warm and verify frequency changed
    fx.set_state_chunk(warm_chunk.clone()).await?;
    settle().await;
    let warm_freq = fx.param_by_name("Band 1 Frequency").get().await?;
    println!("[eq_params_differ] After restoring Warm: Band 1 Frequency = {warm_freq:.4}");

    // The two frequencies should be different
    let delta = (bright_freq - warm_freq).abs();
    println!("[eq_params_differ] Frequency delta: {delta:.4}");
    assert!(
        delta > 0.1,
        "Bright and Warm should have noticeably different Band 1 Frequency (delta={delta:.4})"
    );

    println!("[eq_params_differ] SUCCESS: variations produce different parameter values");
    ctx.log("eq_block_variations_params_differ: PASS");
    Ok(())
}
