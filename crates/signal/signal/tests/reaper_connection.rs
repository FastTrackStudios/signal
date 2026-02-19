//! Integration tests that connect to a running REAPER instance via Unix socket.
//!
//! These tests require the FTS REAPER extension to be loaded and listening
//! on `/tmp/fts-control.sock`. Run with:
//!
//!   cargo xtask reaper-test

use reaper_test::reaper_test;

// ---------------------------------------------------------------------------
// Track listing
// ---------------------------------------------------------------------------

#[reaper_test]
async fn connect_and_read_tracks(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks().all().await?;

    println!("Found {} tracks in current project:", tracks.len());
    for track in &tracks {
        println!(
            "  [{}] {:?} (muted={}, solo={}, vol={:.2}, pan={:.2})",
            track.index, track.name, track.muted, track.soloed, track.volume, track.pan,
        );
    }

    // After remove_all, project may be empty — that's fine for this test
    Ok(())
}

// ---------------------------------------------------------------------------
// FX enumeration
// ---------------------------------------------------------------------------

#[reaper_test]
async fn enumerate_fx_on_first_track(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // Add a track with an FX so we have something to enumerate
    let track = project.tracks().add("FX Enum Test", None).await?;
    let chain = track.fx_chain();
    chain.add("ReaEQ").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let fx_list = chain.all().await?;
    let count = chain.count().await?;

    println!("Track has {} FX (count={})", fx_list.len(), count);
    for fx in &fx_list {
        println!(
            "  [{}] {} ({}) enabled={} params={}",
            fx.index, fx.name, fx.plugin_name, fx.enabled, fx.parameter_count,
        );
    }

    assert_eq!(fx_list.len() as u32, count);
    assert!(count >= 1, "should have at least the ReaEQ we added");
    Ok(())
}

// ---------------------------------------------------------------------------
// Parameter read/write
// ---------------------------------------------------------------------------

#[reaper_test]
async fn read_and_write_fx_parameters(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project.tracks().add("Param Test", None).await?;
    let fx_handle = track.fx_chain().add("ReaEQ").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let info = fx_handle.info().await?;
    println!("FX: {} ({})", info.name, info.plugin_name);

    // Read all parameters
    let params = fx_handle.parameters().await?;
    println!("  {} parameters:", params.len());
    for p in params.iter().take(10) {
        println!(
            "    [{}] {} = {:.4} ({})",
            p.index, p.name, p.value, p.formatted
        );
    }
    if params.len() > 10 {
        println!("    ... and {} more", params.len() - 10);
    }

    // Round-trip: read param 0, set it, verify, restore
    if !params.is_empty() {
        let param = fx_handle.param(0);
        let original = param.get().await?;
        println!("\n  Param 0 original value: {:.4}", original);

        let nudged = if original > 0.5 {
            original - 0.01
        } else {
            original + 0.01
        };
        param.set(nudged).await?;

        let readback = param.get().await?;
        println!("  Param 0 after set({:.4}): {:.4}", nudged, readback);
        assert!(
            (readback - nudged).abs() < 0.02,
            "parameter should be close to set value"
        );

        param.set(original).await?;
        let restored = param.get().await?;
        println!("  Param 0 restored: {:.4}", restored);
        assert!(
            (restored - original).abs() < 0.001,
            "parameter should be restored to original"
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// VST state chunk get/set
// ---------------------------------------------------------------------------

#[reaper_test]
async fn get_and_set_vst_state_chunk(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project.tracks().add("Chunk Test", None).await?;
    let fx_handle = track.fx_chain().add("ReaEQ").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let info = fx_handle.info().await?;
    println!("FX: {} ({})", info.name, info.plugin_name);

    let encoded = fx_handle.state_chunk_encoded().await?;
    match &encoded {
        Some(s) => println!("  State chunk (base64): {} bytes", s.len()),
        None => println!("  No state chunk available"),
    }

    let binary = fx_handle.state_chunk().await?;
    match &binary {
        Some(b) => println!("  State chunk (binary): {} bytes", b.len()),
        None => println!("  No binary state chunk available"),
    }

    if let Some(chunk) = encoded {
        assert!(!chunk.is_empty(), "encoded state chunk should not be empty");
        fx_handle.set_state_chunk_encoded(chunk.clone()).await?;
        println!("  State chunk restored successfully (encoded)");

        let after = fx_handle
            .state_chunk_encoded()
            .await?
            .expect("state should still be available after restore");
        assert_eq!(
            chunk.len(),
            after.len(),
            "state chunk size should be stable after round-trip"
        );
        println!(
            "  Round-trip verified: chunk size stable ({} bytes)",
            after.len()
        );
    }

    if let Some(chunk) = binary {
        assert!(!chunk.is_empty(), "binary state chunk should not be empty");
        fx_handle.set_state_chunk(chunk.clone()).await?;
        println!("  State chunk restored successfully (binary)");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Clone FX, tweak params, diff against source
// ---------------------------------------------------------------------------

#[reaper_test]
async fn clone_fx_tweak_and_diff(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    // -- Source track with ReaEQ
    let src_track = project.tracks().add("Source Track", None).await?;
    let src_fx = src_track.fx_chain().add("ReaEQ").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let src_info = src_fx.info().await?;
    println!(
        "Source FX: {} ({}) -- {} params",
        src_info.name, src_info.plugin_name, src_info.parameter_count
    );

    let src_chunk = src_fx
        .state_chunk_encoded()
        .await?
        .ok_or_else(|| eyre::eyre!("Source FX has no state chunk"))?;
    let src_params = src_fx.parameters().await?;

    // -- Create a second track and clone the FX
    let dst_track = project.tracks().add("FX Diff Test", None).await?;
    let dst_fx = dst_track.fx_chain().add(&src_info.plugin_name).await?;
    dst_fx.set_state_chunk_encoded(src_chunk.clone()).await?;
    println!("Loaded and cloned state onto new track");

    // Verify identical before tweaking
    let dst_chunk_before = dst_fx
        .state_chunk_encoded()
        .await?
        .ok_or_else(|| eyre::eyre!("Dst FX has no state chunk after restore"))?;
    assert_eq!(
        src_chunk.len(),
        dst_chunk_before.len(),
        "state chunks should be the same size right after clone"
    );

    // -- Tweak up to 3 params on dst
    let dst_params_before = dst_fx.parameters().await?;
    let tweak_count = dst_params_before.len().min(3);
    let mut tweaks: Vec<(u32, f64, f64)> = Vec::new();

    for p in dst_params_before.iter().take(tweak_count) {
        let nudged = if p.value > 0.5 {
            p.value - 0.15
        } else {
            p.value + 0.15
        };
        let nudged = nudged.clamp(0.0, 1.0);
        dst_fx.param(p.index).set(nudged).await?;
        tweaks.push((p.index, p.value, nudged));
        println!(
            "  Tweaked param {} '{}': {:.4} -> {:.4}",
            p.index, p.name, p.value, nudged
        );
    }

    // -- Diff: parameter level
    let dst_params_after = dst_fx.parameters().await?;
    println!("\n-- Parameter-level diff");
    let mut diffs = 0usize;
    for (src_p, dst_p) in src_params.iter().zip(dst_params_after.iter()) {
        let delta = (src_p.value - dst_p.value).abs();
        if delta > 0.001 {
            println!(
                "  param {} '{}'\n    src: {} (raw {:.4})\n    dst: {} (raw {:.4})",
                src_p.index, src_p.name, src_p.formatted, src_p.value, dst_p.formatted, dst_p.value,
            );
            diffs += 1;
        }
    }
    if diffs == 0 {
        println!("  No parameter differences detected");
    } else {
        println!("  {} parameter(s) differ", diffs);
    }

    // Tweaked params on dst must differ from src
    for (idx, original, nudged) in &tweaks {
        let dst_val = dst_fx.param(*idx).get().await?;
        let src_val = src_fx.param(*idx).get().await?;
        assert!(
            (dst_val - nudged).abs() < 0.02,
            "param {} on dst should be near {:.4}, got {:.4}",
            idx,
            nudged,
            dst_val
        );
        assert!(
            (src_val - original).abs() < 0.001,
            "param {} on src should be unchanged ({:.4}), got {:.4}",
            idx,
            original,
            src_val
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Full chain state capture/restore
// ---------------------------------------------------------------------------

#[reaper_test]
async fn capture_and_restore_full_chain_state(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project.tracks().add("Chain State Test", None).await?;
    let chain = track.fx_chain();

    // Add a couple FX so we have a chain to capture
    // (both ReaEQ — ReaComp doesn't support vst_chunk_encoded via named config params)
    chain.add("ReaEQ").await?;
    chain.add("ReaEQ").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let fx_count = chain.count().await?;
    println!("Track has {} FX in chain", fx_count);
    assert!(fx_count >= 2, "should have at least our 2 FX");

    let state = chain.state().await?;
    println!("  Captured {} FX state chunks:", state.len());
    for chunk in &state {
        println!(
            "    [{}] {} (guid={}, {} bytes)",
            chunk.fx_index,
            chunk.plugin_name,
            &chunk.fx_guid[..8.min(chunk.fx_guid.len())],
            chunk.encoded_chunk.len(),
        );
    }

    assert_eq!(
        state.len() as u32,
        fx_count,
        "chain state should have one entry per FX"
    );

    chain.restore_state(state).await?;
    println!("  Full chain state restored successfully");

    let after_count = chain.count().await?;
    assert_eq!(
        fx_count, after_count,
        "FX count should be unchanged after restore"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshot system: save/recall multiple named snapshots
// ---------------------------------------------------------------------------

#[reaper_test]
async fn fx_snapshot_save_and_recall(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project.tracks().add("Snapshot Test", None).await?;
    let chain = track.fx_chain();
    let fx = chain.add("ReaEQ").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let info = fx.info().await?;
    println!("FX under test: {} ({})", info.name, info.plugin_name);

    let params = fx.parameters().await?;
    let tweak_count = params.len().min(3);
    if tweak_count == 0 {
        println!("  Skipping -- FX has no parameters");
        return Ok(());
    }

    // -- Capture initial state for restore at the end
    let initial_state = chain.state().await?;
    println!("Captured initial chain state ({} FX)", initial_state.len());

    // -- Build 5 snapshots at evenly-spaced parameter offsets
    let offsets: &[f64] = &[0.1, 0.25, 0.5, 0.75, 0.9];
    let snapshot_labels = ["A", "B", "C", "D", "E"];

    struct Snapshot {
        label: &'static str,
        state: Vec<daw_proto::FxStateChunk>,
        param_values: Vec<(u32, f64, String)>,
    }

    let mut snapshots: Vec<Snapshot> = Vec::new();

    for (offset, label) in offsets.iter().zip(snapshot_labels.iter()) {
        for p in params.iter().take(tweak_count) {
            fx.param(p.index).set(*offset).await?;
        }

        let param_values: Vec<(u32, f64, String)> = {
            let current = fx.parameters().await?;
            current
                .iter()
                .take(tweak_count)
                .map(|p| (p.index, p.value, p.formatted.clone()))
                .collect()
        };

        let state = chain.state().await?;
        println!("Snapshot {}: offset={:.2}", label, offset);

        snapshots.push(Snapshot {
            label,
            state,
            param_values,
        });
    }

    // -- Recall each snapshot and verify
    println!("\n-- Recalling snapshots");
    for snap in &snapshots {
        chain.restore_state(snap.state.clone()).await?;

        let current = fx.parameters().await?;
        println!("  Recalled snapshot {}:", snap.label);

        let mut all_match = true;
        for (idx, _expected_raw, expected_fmt) in &snap.param_values {
            let actual = current
                .iter()
                .find(|p| p.index == *idx)
                .ok_or_else(|| eyre::eyre!("param {} missing after recall", idx))?;
            let matched = actual.formatted == *expected_fmt;
            if !matched {
                println!(
                    "    param {} '{}': expected {}  got {}  MISMATCH",
                    idx, actual.name, expected_fmt, actual.formatted,
                );
                all_match = false;
            }
        }

        assert!(
            all_match,
            "snapshot {} recall produced unexpected param values",
            snap.label
        );
    }

    // -- Verify snapshots are distinct
    let distinct_chunks: std::collections::HashSet<&str> = snapshots
        .iter()
        .filter_map(|s| s.state.first().map(|c| c.encoded_chunk.as_str()))
        .collect();
    assert!(
        distinct_chunks.len() > 1,
        "all snapshots produced identical state chunks"
    );
    println!(
        "\n{} distinct chunk states across {} snapshots",
        distinct_chunks.len(),
        snapshots.len()
    );

    // -- Restore original state
    chain.restore_state(initial_state).await?;
    println!("Restored original state");

    Ok(())
}

// ---------------------------------------------------------------------------
// Preset harvesting: cycle through all factory presets, capture name + state
// ---------------------------------------------------------------------------

#[reaper_test]
async fn harvest_factory_presets(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project.tracks().add("Preset Harvest", None).await?;
    let chain = track.fx_chain();

    // ReaComp has factory presets; ReaEQ has zero.
    let fx = chain.add("ReaComp").await?;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let info = fx.info().await?;
    println!("FX: {} ({})", info.name, info.plugin_name);

    // Query initial preset state
    let initial = fx
        .preset_index()
        .await?
        .ok_or_else(|| eyre::eyre!("get_preset_index returned None"))?;
    println!(
        "  Initial: index={:?}, count={}, name={:?}",
        initial.index, initial.count, initial.name
    );

    if initial.count == 0 {
        println!("  Plugin has 0 factory presets — skipping harvest");
        return Ok(());
    }

    // Set to preset 0 as a known starting point
    fx.set_preset(0).await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Harvest: cycle forward through every preset, capturing name + state chunk
    struct HarvestedPreset {
        index: u32,
        name: String,
        chunk_len: usize,
    }
    let mut harvested: Vec<HarvestedPreset> = Vec::new();

    for i in 0..initial.count {
        fx.set_preset(i).await?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let preset_info = fx
            .preset_index()
            .await?
            .ok_or_else(|| eyre::eyre!("get_preset_index returned None at index {}", i))?;

        let name = preset_info
            .name
            .unwrap_or_else(|| format!("(unnamed #{})", i));

        let chunk = fx.state_chunk_encoded().await?.unwrap_or_default();

        println!("  [{}] {:?} — {} bytes state", i, name, chunk.len());

        harvested.push(HarvestedPreset {
            index: i,
            name,
            chunk_len: chunk.len(),
        });
    }

    println!(
        "\nHarvested {} / {} presets",
        harvested.len(),
        initial.count
    );

    // Verify we got presets
    assert!(
        !harvested.is_empty(),
        "should have harvested at least 1 preset"
    );
    assert_eq!(
        harvested.len() as u32,
        initial.count,
        "should have harvested all reported presets"
    );

    // Verify presets have state data
    for hp in &harvested {
        assert!(
            hp.chunk_len > 0,
            "preset [{}] '{}' should have non-empty state chunk",
            hp.index,
            hp.name
        );
    }

    // Verify we can navigate back to preset 0 and the name matches
    fx.set_preset(0).await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let final_info = fx.preset_index().await?.expect("preset_index after reset");
    assert_eq!(
        final_info.index,
        Some(0),
        "should be back at preset 0 after cycling"
    );

    println!("PASS — all {} factory presets harvested", harvested.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// Final cleanup — run after all other tests to remove leftover tracks/tabs
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn final_cleanup(ctx: &ReaperTestContext) -> eyre::Result<()> {
    println!("\n=== final cleanup ===");
    reaper_test::cleanup_all_projects(&ctx.daw).await?;
    println!("PASS");
    Ok(())
}
