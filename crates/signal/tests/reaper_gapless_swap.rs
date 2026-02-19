//! REAPER integration tests for gapless FX switching.
//!
//! Tests the FX pin mapping API and GaplessSwapEngine against live FX chains.
//!
//! Run with:
//!
//!   cargo xtask reaper-test

mod daw_helpers;

use daw_proto::{FxNodeId, FxPinMappings};
use eyre::Result;
use reaper_test::{reaper_test, ReaperTestContext};
use signal_live::engine::{GaplessSwapEngine, SwapConfig, SwapResult};
use std::time::Duration;

// =========================================================================
// Helpers
// =========================================================================

/// Get a track that has at least one FX loaded.
#[allow(dead_code)]
async fn get_track_with_fx(ctx: &ReaperTestContext) -> Result<daw_control::TrackHandle> {
    let project = ctx.project().clone();
    let tracks = project.tracks().all().await?;

    for track_info in &tracks {
        let track = project
            .tracks()
            .by_index(track_info.index)
            .await?
            .ok_or_else(|| eyre::eyre!("Track not found at index {}", track_info.index))?;
        let fx_count = track.fx_chain().count().await?;
        if fx_count > 0 {
            println!(
                "  Using track [{}] '{}' with {} FX",
                track_info.index, track_info.name, fx_count
            );
            return Ok(track);
        }
    }
    Err(eyre::eyre!("No track with FX found in project"))
}

/// Create a clean test track with a single ReaEQ for Group A tests.
async fn create_test_track_with_eq(
    ctx: &ReaperTestContext,
) -> Result<(daw_control::TrackHandle, daw_control::FxHandle)> {
    let project = ctx.project().clone();
    let track = project.tracks().add("PinMappingTest", None).await?;
    let fx = track.fx_chain().add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("  Created test track with ReaEQ ({})", fx.guid());
    Ok((track, fx))
}

// =========================================================================
// Group A: FX Channel Config Read + Pin Mapping Silence/Restore
// =========================================================================

#[reaper_test]
async fn read_channel_config(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: read_channel_config ---");
    let (_track, fx) = create_test_track_with_eq(ctx).await?;
    let info = fx.info().await?;

    let config = fx.channel_config().await?;
    println!(
        "  FX '{}' channel_config: count={} mode={} flags={}",
        info.name, config.channel_count, config.channel_mode, config.supported_flags
    );

    assert!(
        config.channel_count <= 128,
        "channel_count should be reasonable, got {}",
        config.channel_count
    );

    Ok(())
}

#[reaper_test]
async fn silence_fx_via_pin_mappings(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: silence_fx_via_pin_mappings ---");
    let (_track, fx) = create_test_track_with_eq(ctx).await?;

    let saved = fx.silence_output().await?;
    println!(
        "  Silenced FX, saved {} output pin mappings",
        saved.output_pins.len()
    );

    assert!(
        !saved.output_pins.is_empty(),
        "should have saved non-zero pin mappings before zeroing"
    );

    for &(pin, low, high) in &saved.output_pins {
        println!("    pin {}: low=0x{:08x} high=0x{:08x}", pin, low, high);
    }

    Ok(())
}

#[reaper_test]
async fn restore_output_after_silence(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: restore_output_after_silence ---");
    let (_track, fx) = create_test_track_with_eq(ctx).await?;

    let saved = fx.silence_output().await?;
    println!("  Silenced, saved {} pins", saved.output_pins.len());

    let second_silence = fx.silence_output().await?;
    assert!(
        second_silence.output_pins.is_empty(),
        "second silence should find no non-zero pins (already zeroed), got {} pins",
        second_silence.output_pins.len()
    );

    fx.restore_output(saved.clone()).await?;
    println!("  Restored original pin mappings");

    let re_read = fx.silence_output().await?;
    println!(
        "  Re-silenced, got {} pins (expected {})",
        re_read.output_pins.len(),
        saved.output_pins.len()
    );
    assert_eq!(
        re_read.output_pins.len(),
        saved.output_pins.len(),
        "should get back the same number of pin mappings after restore"
    );

    fx.restore_output(re_read).await?;
    Ok(())
}

#[reaper_test]
async fn restore_default_stereo(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: restore_default_stereo ---");
    let (_track, fx) = create_test_track_with_eq(ctx).await?;

    let _saved = fx.silence_output().await?;

    fx.restore_output(FxPinMappings::default()).await?;
    println!("  Restored default stereo pass-through");

    let stereo_pins = fx.silence_output().await?;
    println!(
        "  After default restore, got {} output pins",
        stereo_pins.output_pins.len()
    );
    assert!(
        stereo_pins.output_pins.len() >= 2,
        "default stereo should have at least 2 output pin mappings, got {}",
        stereo_pins.output_pins.len()
    );

    fx.restore_output(stereo_pins).await?;
    Ok(())
}

// =========================================================================
// Group B: Single Block Swap
// =========================================================================

#[reaper_test]
async fn swap_block_same_plugin(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: swap_block_same_plugin ---");
    let project = ctx.project().clone();
    let track = project.tracks().add("SwapTest", None).await?;
    let chain = track.fx_chain();

    let old_fx = chain.add("ReaEQ").await?;
    let old_guid = old_fx.guid().to_string();
    println!("  Added old FX: ReaEQ ({})", old_guid);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let engine = GaplessSwapEngine::new();
    let result = engine.swap_block(&chain, &old_fx, "ReaEQ").await;

    match &result {
        SwapResult::Success {
            new_fx_guid,
            old_fx_guid,
        } => {
            println!("  Swap succeeded: old={} new={}", old_fx_guid, new_fx_guid);
            assert_eq!(old_fx_guid, &old_guid);
            assert_ne!(
                new_fx_guid, old_fx_guid,
                "new FX should have different GUID"
            );

            let new_fx = chain
                .by_guid(new_fx_guid)
                .await?
                .ok_or_else(|| eyre::eyre!("new FX not found by GUID"))?;

            let saved = new_fx.silence_output().await?;
            assert!(
                !saved.output_pins.is_empty(),
                "new FX should have active output pins"
            );
            new_fx.restore_output(saved).await?;

            new_fx.remove().await?;
        }
        SwapResult::LoadTimeout { fx_name } => panic!("Swap timed out loading '{}'", fx_name),
        SwapResult::Failed(msg) => panic!("Swap failed: {}", msg),
    }

    Ok(())
}

#[reaper_test]
async fn swap_block_different_plugin(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: swap_block_different_plugin ---");
    let project = ctx.project().clone();
    let track = project.tracks().add("SwapDiffTest", None).await?;
    let chain = track.fx_chain();

    let old_fx = chain.add("ReaEQ").await?;
    println!("  Added old FX: ReaEQ ({})", old_fx.guid());
    tokio::time::sleep(Duration::from_millis(500)).await;

    let engine = GaplessSwapEngine::new();
    let result = engine.swap_block(&chain, &old_fx, "ReaComp").await;

    match &result {
        SwapResult::Success {
            new_fx_guid,
            old_fx_guid,
        } => {
            println!(
                "  Swap ReaEQ->ReaComp succeeded: old={} new={}",
                old_fx_guid, new_fx_guid
            );

            let new_fx = chain
                .by_guid(new_fx_guid)
                .await?
                .ok_or_else(|| eyre::eyre!("new FX not found"))?;
            let info = new_fx.info().await?;
            println!("  New FX: {} ({})", info.name, info.plugin_name);
            assert!(
                info.name.contains("ReaComp") || info.plugin_name.contains("ReaComp"),
                "new FX should be ReaComp, got: {}",
                info.name
            );

            new_fx.remove().await?;
        }
        SwapResult::LoadTimeout { fx_name } => panic!("Load timeout: {}", fx_name),
        SwapResult::Failed(msg) => panic!("Failed: {}", msg),
    }

    Ok(())
}

#[reaper_test]
async fn swap_block_with_state_chunk(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: swap_block_with_state_chunk ---");
    let project = ctx.project().clone();
    let track = project.tracks().add("SwapChunkTest", None).await?;
    let chain = track.fx_chain();

    let old_fx = chain.add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let chunk = old_fx
        .state_chunk_encoded()
        .await?
        .ok_or_else(|| eyre::eyre!("no state chunk from old FX"))?;
    println!("  Captured state chunk ({} bytes encoded)", chunk.len());

    let engine = GaplessSwapEngine::new();
    let result = engine
        .swap_block_with_chunk(&chain, &old_fx, "ReaEQ", &chunk)
        .await;

    match &result {
        SwapResult::Success {
            new_fx_guid,
            old_fx_guid,
        } => {
            println!(
                "  Swap with chunk succeeded: old={} new={}",
                old_fx_guid, new_fx_guid
            );

            let new_fx = chain
                .by_guid(new_fx_guid)
                .await?
                .ok_or_else(|| eyre::eyre!("new FX not found"))?;

            let saved = new_fx.silence_output().await?;
            assert!(
                !saved.output_pins.is_empty(),
                "new FX should have active output pins"
            );
            new_fx.restore_output(saved).await?;

            new_fx.remove().await?;
        }
        SwapResult::LoadTimeout { fx_name } => panic!("Load timeout: {}", fx_name),
        SwapResult::Failed(msg) => panic!("Failed: {}", msg),
    }

    Ok(())
}

#[reaper_test]
async fn swap_block_timeout(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: swap_block_timeout ---");
    let project = ctx.project().clone();
    let track = project.tracks().add("SwapTimeoutTest", None).await?;
    let chain = track.fx_chain();

    let old_fx = chain.add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let engine = GaplessSwapEngine::with_config(SwapConfig {
        load_timeout: Duration::from_secs(2),
        remove_old: false,
        poll_interval: Duration::from_millis(100),
    });

    let result = engine
        .swap_block(&chain, &old_fx, "NonExistentPlugin12345")
        .await;

    match &result {
        SwapResult::LoadTimeout { fx_name } => {
            println!("  Got expected LoadTimeout for '{}'", fx_name);
            assert_eq!(fx_name, "NonExistentPlugin12345");
        }
        SwapResult::Success { .. } => panic!("Should not have succeeded with fake plugin"),
        SwapResult::Failed(msg) => {
            println!("  Got Failed (acceptable): {}", msg);
        }
    }

    Ok(())
}

// =========================================================================
// Group C: Pin Mapping Preservation After Swap
// =========================================================================

#[reaper_test]
async fn swap_preserves_active_output(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: swap_preserves_active_output ---");
    let project = ctx.project().clone();
    let track = project.tracks().add("PreserveOutputTest", None).await?;
    let chain = track.fx_chain();

    let old_fx = chain.add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let engine = GaplessSwapEngine::with_config(SwapConfig {
        remove_old: false,
        ..Default::default()
    });

    let result = engine.swap_block(&chain, &old_fx, "ReaComp").await;

    match &result {
        SwapResult::Success {
            new_fx_guid,
            old_fx_guid: _,
        } => {
            let new_fx = chain
                .by_guid(new_fx_guid)
                .await?
                .ok_or_else(|| eyre::eyre!("new FX not found"))?;

            let new_pins = new_fx.silence_output().await?;
            assert!(
                !new_pins.output_pins.is_empty(),
                "new FX should have active output pin mappings"
            );
            new_fx.restore_output(new_pins).await?;

            let old_pins = old_fx.silence_output().await?;
            assert!(
                old_pins.output_pins.is_empty(),
                "old FX should have zeroed output pins (already silenced), got {} pins",
                old_pins.output_pins.len()
            );

            new_fx.remove().await?;
            old_fx.remove().await?;
        }
        other => panic!("Expected Success, got: {:?}", other),
    }

    Ok(())
}

// =========================================================================
// Group D: Container Module Swap (SKIPPED — known empty-container stride bug)
// =========================================================================

// scenario_swap_container_module is skipped: container_item.0 returns None
// for empty containers, requiring stride-based addressing which is unreliable.

// =========================================================================
// Group E: Full Engine Integration
// =========================================================================

#[reaper_test]
async fn engine_full_swap_round_trip(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: engine_full_swap_round_trip ---");
    let project = ctx.project().clone();

    let track = project.tracks().add("GaplessSwapTest", None).await?;
    let chain = track.fx_chain();

    let old_fx = chain.add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!(
        "  Initial FX: {} ({})",
        old_fx.info().await?.name,
        old_fx.guid()
    );

    let engine = GaplessSwapEngine::new();
    let result = engine.swap_block(&chain, &old_fx, "ReaComp").await;

    match &result {
        SwapResult::Success {
            new_fx_guid,
            old_fx_guid,
        } => {
            println!(
                "  Full round-trip: ReaEQ({}) -> ReaComp({})",
                old_fx_guid, new_fx_guid
            );

            let fx_list = chain.all().await?;
            println!("  Chain now has {} FX:", fx_list.len());
            for fx in &fx_list {
                println!("    [{}] {} ({})", fx.index, fx.name, fx.guid);
            }

            assert!(
                fx_list
                    .iter()
                    .any(|f| f.name.contains("ReaComp") || f.plugin_name.contains("ReaComp")),
                "chain should contain ReaComp after swap"
            );

            let remaining = chain
                .by_guid(new_fx_guid)
                .await?
                .ok_or_else(|| eyre::eyre!("swapped FX not found"))?;
            let saved = remaining.silence_output().await?;
            assert!(
                !saved.output_pins.is_empty(),
                "swapped FX should have active output pins"
            );
            remaining.restore_output(saved).await?;
        }
        SwapResult::LoadTimeout { fx_name } => panic!("Load timeout: {}", fx_name),
        SwapResult::Failed(msg) => panic!("Failed: {}", msg),
    }

    Ok(())
}

#[reaper_test]
async fn engine_sequential_swaps(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: engine_sequential_swaps ---");
    let project = ctx.project().clone();

    let track = project.tracks().add("SequentialSwapTest", None).await?;
    let chain = track.fx_chain();

    let fx1 = chain.add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("  Step 1: ReaEQ loaded ({})", fx1.guid());

    let engine = GaplessSwapEngine::new();

    // Swap 1: ReaEQ -> ReaComp
    let result1 = engine.swap_block(&chain, &fx1, "ReaComp").await;
    let fx2_guid = match &result1 {
        SwapResult::Success { new_fx_guid, .. } => {
            println!("  Step 2: Swapped to ReaComp ({})", new_fx_guid);
            new_fx_guid.clone()
        }
        other => panic!("Swap 1 failed: {:?}", other),
    };

    // Swap 2: ReaComp -> ReaDelay
    let fx2 = chain
        .by_guid(&fx2_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("ReaComp not found after swap 1"))?;

    let result2 = engine.swap_block(&chain, &fx2, "ReaDelay").await;
    match &result2 {
        SwapResult::Success {
            new_fx_guid,
            old_fx_guid,
        } => {
            println!(
                "  Step 3: Swapped ReaComp({}) -> ReaDelay({})",
                old_fx_guid, new_fx_guid
            );

            let fx_list = chain.all().await?;
            println!("  Final chain ({} FX):", fx_list.len());
            for fx in &fx_list {
                println!("    [{}] {}", fx.index, fx.name);
            }

            assert!(
                fx_list
                    .iter()
                    .any(|f| f.name.contains("ReaDelay") || f.plugin_name.contains("ReaDelay")),
                "chain should contain ReaDelay after second swap"
            );
        }
        other => panic!("Swap 2 failed: {:?}", other),
    }

    Ok(())
}

// =========================================================================
// Group F: Intra-Container Block Swap (GUITAR Rig)
// =========================================================================

/// The INPUT Module container on the GUITAR Rig track (top-level index 0).
const INPUT_MODULE_CONTAINER_ID: &str = "container:0";
/// The gate block (ReaGate) inside the INPUT Module.
const GATE_BLOCK_GUID: &str = "89382D99-B838-B645-BD18-A4FDF91F14FE";

#[reaper_test]
async fn swap_gate_block_in_container(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: swap_gate_block_in_container ---");
    ctx.load_template("testing-stockjs-guitar-rig").await?;
    let track = ctx.track_by_name("GUITAR Rig").await?;
    let chain = track.fx_chain();
    let container_id = FxNodeId::from(INPUT_MODULE_CONTAINER_ID);

    // Dump the FX tree for diagnostics.
    let tree = chain.tree().await?;
    println!("  FX tree for GUITAR Rig:");
    for (depth, node) in tree.iter_depth_first() {
        println!(
            "    {} id={} kind={:?}",
            "  ".repeat(depth),
            node.id,
            node.kind
        );
    }

    // Dump flat chain for GUID comparison.
    let all_fx = chain.all().await?;
    println!("  Flat FX chain ({} items):", all_fx.len());
    for fx in &all_fx {
        println!("    [{}] '{}' guid={}", fx.index, fx.name, fx.guid);
    }

    // Find the existing gate block inside the INPUT Module.
    let old_gate = chain.by_guid(GATE_BLOCK_GUID).await?.ok_or_else(|| {
        eyre::eyre!(
            "Gate block not found in INPUT Module (guid={})",
            GATE_BLOCK_GUID
        )
    })?;
    let old_gate_info = old_gate.info().await?;
    println!(
        "  Found gate block: '{}' ({})",
        old_gate_info.name,
        old_gate.guid()
    );

    // Add a new ReaGate to the top-level chain, then move it into the container.
    let new_gate = chain.add("ReaGate").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let new_gate_guid = new_gate.guid().to_string();
    println!("  Added new ReaGate ({})", new_gate_guid);

    // Silence the new gate via pin mappings before moving it in.
    let _new_gate_saved = new_gate.silence_output().await?;
    println!("  Silenced new ReaGate output pins");

    // Move into the INPUT Module container at child position 1 (after the old gate).
    let new_gate_node_id = FxNodeId::from_guid(&new_gate_guid);
    chain
        .move_to_container(&new_gate_node_id, &container_id, 1)
        .await?;
    println!("  Moved new ReaGate into INPUT Module at child 1");

    // Re-acquire handle after move (GUID is stable across moves).
    let new_gate = chain
        .by_guid(&new_gate_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("new ReaGate not found after move"))?;

    // Gapless swap: activate new (restore default stereo pins), silence old.
    new_gate.restore_output(FxPinMappings::default()).await?;
    let old_gate_saved = old_gate.silence_output().await?;

    println!("  Swapped: new gate active, old gate silenced");

    // Verify the new gate has output and old doesn't.
    let new_pins = new_gate.silence_output().await?;
    assert!(
        !new_pins.output_pins.is_empty(),
        "new gate should have active output"
    );
    new_gate.restore_output(new_pins).await?;

    let old_pins = old_gate.silence_output().await?;
    assert!(
        old_pins.output_pins.is_empty(),
        "old gate should already be silenced"
    );

    // Restore: reactivate old, remove new.
    old_gate.restore_output(old_gate_saved).await?;
    new_gate.remove().await?;
    println!("  Restored original gate block, removed new one");

    Ok(())
}

#[reaper_test]
async fn swap_eq_blocks_in_container(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n--- scenario: swap_eq_blocks_in_container ---");
    ctx.load_template("testing-stockjs-guitar-rig").await?;
    let track = ctx.track_by_name("GUITAR Rig").await?;
    let chain = track.fx_chain();
    let container_id = FxNodeId::from(INPUT_MODULE_CONTAINER_ID);

    // Add EQ A: bass-heavy (boost low frequencies).
    let eq_a = chain.add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let eq_a_guid = eq_a.guid().to_string();

    let eq_a_params = eq_a.parameters().await?;
    println!(
        "  EQ A has {} params. Setting bass-heavy curve...",
        eq_a_params.len()
    );
    eq_a.param(1).set(0.05).await?; // Band 1 Frequency: low
    eq_a.param(2).set(0.85).await?; // Band 1 Gain: strong boost

    // Move EQ A into the INPUT Module.
    let eq_a_node_id = FxNodeId::from_guid(&eq_a_guid);
    chain
        .move_to_container(&eq_a_node_id, &container_id, 1)
        .await?;
    println!("  EQ A (bass-heavy) moved into INPUT Module");

    let eq_a = chain
        .by_guid(&eq_a_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("EQ A not found after move"))?;

    // Add EQ B: treble-heavy (boost high frequencies), start silenced.
    let eq_b = chain.add("ReaEQ").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let eq_b_guid = eq_b.guid().to_string();

    eq_b.param(1).set(0.95).await?; // Band 1 Frequency: high
    eq_b.param(2).set(0.85).await?; // Band 1 Gain: strong boost

    let _eq_b_saved = eq_b.silence_output().await?;

    let eq_b_node_id = FxNodeId::from_guid(&eq_b_guid);
    chain
        .move_to_container(&eq_b_node_id, &container_id, 2)
        .await?;
    println!("  EQ B (treble-heavy) moved into INPUT Module (silenced)");

    let eq_b = chain
        .by_guid(&eq_b_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("EQ B not found after move"))?;

    // Verify initial state: EQ A active, EQ B silenced.
    let a_pins = eq_a.silence_output().await?;
    assert!(
        !a_pins.output_pins.is_empty(),
        "EQ A should start with active output"
    );
    eq_a.restore_output(a_pins.clone()).await?;

    let b_check = eq_b.silence_output().await?;
    assert!(b_check.output_pins.is_empty(), "EQ B should start silenced");
    println!("  Initial state verified: EQ A active, EQ B silenced");

    // Swap 1: A -> B (activate B, silence A).
    eq_b.restore_output(FxPinMappings::default()).await?;
    let eq_a_saved = eq_a.silence_output().await?;
    println!("  Swap 1: activated EQ B, silenced EQ A");

    // Verify swap.
    let b_active = eq_b.silence_output().await?;
    assert!(
        !b_active.output_pins.is_empty(),
        "EQ B should be active after swap"
    );
    eq_b.restore_output(b_active).await?;

    let a_silenced = eq_a.silence_output().await?;
    assert!(
        a_silenced.output_pins.is_empty(),
        "EQ A should be silenced after swap"
    );

    // Verify parameter state survived the swap.
    let b_freq = eq_b.param(1).get().await?;
    println!("  EQ B freq param after swap: {:.3}", b_freq);
    assert!(
        b_freq > 0.8,
        "EQ B should retain high-freq setting, got {}",
        b_freq
    );

    // Swap 2: B -> A (swap back).
    eq_a.restore_output(eq_a_saved).await?;
    let _eq_b_saved2 = eq_b.silence_output().await?;
    println!("  Swap 2: activated EQ A, silenced EQ B");

    // Verify EQ A still has its bass-heavy settings.
    let a_freq = eq_a.param(1).get().await?;
    println!("  EQ A freq param after swap-back: {:.3}", a_freq);
    assert!(
        a_freq < 0.2,
        "EQ A should retain low-freq setting, got {}",
        a_freq
    );

    // Cleanup: remove both test EQs.
    eq_a.remove().await?;
    eq_b.remove().await?;
    println!("  Cleaned up both test EQs from INPUT Module");

    Ok(())
}
