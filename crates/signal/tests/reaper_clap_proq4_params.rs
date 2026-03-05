//! REAPER integration test: CLAP FabFilter Pro-Q 4 parameter manipulation.
//!
//! Tests loading a CLAP Pro-Q 4 instance and exercising every parameter control
//! path: by-index set/get, by-name set/get, and state chunk save/restore.
//!
//! Run with:
//!   cargo xtask reaper-test reaper_clap_proq4_params

use std::time::Duration;

use reaper_test::reaper_test;

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

// ---------------------------------------------------------------------------
// 1. Basic load & enumerate parameters
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_load_and_enumerate_params(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 Enumerate", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    let params = fx.parameters().await?;
    println!("[proq4] Loaded with {} parameters", params.len());

    // Pro-Q 4 should have a large number of parameters (bands * params-per-band + globals)
    assert!(
        params.len() > 50,
        "Expected Pro-Q 4 to have >50 params, got {}",
        params.len()
    );

    // Print first 30 for reference
    for p in params.iter().take(30) {
        println!(
            "  [{:3}] {:<30} = {:.6}  ({})",
            p.index, p.name, p.value, p.formatted
        );
    }

    // Verify some known parameter names exist
    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
    let expected_names = [
        "Band 1 Frequency",
        "Band 1 Gain",
        "Band 1 Q",
        "Band 1 Shape",
        "Band 1 Enabled",
        "Output Level",
    ];
    for expected in &expected_names {
        assert!(
            names.contains(expected),
            "Expected parameter '{}' not found. Available: {:?}",
            expected,
            &names[..names.len().min(20)]
        );
    }

    ctx.log(&format!("Enumerated {} params successfully", params.len()));
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Set parameter by index — verify readback
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_set_param_by_index(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 ByIndex", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    // Find "Output Level" param index
    let params = fx.parameters().await?;
    let output_level = params
        .iter()
        .find(|p| p.name == "Output Level")
        .ok_or_else(|| eyre::eyre!("'Output Level' param not found"))?;

    let original_value = output_level.value;
    let target_value = if original_value < 0.5 { 0.75 } else { 0.25 };

    println!(
        "[proq4] Output Level (idx {}): original={:.6}, setting to {:.6}",
        output_level.index, original_value, target_value
    );

    // Set by index
    fx.param(output_level.index).set(target_value).await?;
    settle().await;

    // Read back
    let readback = fx.param(output_level.index).get().await?;
    println!("[proq4] Readback after set: {:.6}", readback);

    let delta = (readback - target_value).abs();
    if delta < 0.01 {
        println!("[proq4] SUCCESS: param set by index works (delta={:.6})", delta);
    } else {
        println!(
            "[proq4] FAIL: param set by index did NOT take effect. Expected {:.6}, got {:.6} (delta={:.6})",
            target_value, readback, delta
        );
    }

    ctx.log(&format!(
        "set_by_index: target={:.6} readback={:.6} delta={:.6} ok={}",
        target_value, readback, delta, delta < 0.01
    ));

    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Set parameter by name — verify readback
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_set_param_by_name(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 ByName", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    let test_params = [
        ("Output Level", 0.3),
        ("Band 1 Frequency", 0.7),
        ("Band 1 Gain", 0.4),
        ("Band 1 Q", 0.6),
    ];

    for (name, target) in &test_params {
        // Read original
        let original = fx.param_by_name(name).get().await?;

        // Set
        fx.param_by_name(name).set(*target).await?;
        settle().await;

        // Read back
        let readback = fx.param_by_name(name).get().await?;
        let delta = (readback - target).abs();

        let status = if delta < 0.01 { "OK" } else { "FAIL" };
        println!(
            "[proq4] {}: original={:.4} target={:.4} readback={:.4} delta={:.4} [{}]",
            name, original, target, readback, delta, status
        );

        ctx.log(&format!(
            "set_by_name({}): original={:.6} target={:.6} readback={:.6} delta={:.6} ok={}",
            name, original, target, readback, delta, delta < 0.01
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Bulk parameter set — set many params, then verify all at once
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_bulk_param_set(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 Bulk", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    // Set a batch of band 1 parameters
    let targets: Vec<(&str, f64)> = vec![
        ("Band 1 Enabled", 1.0),
        ("Band 1 Frequency", 0.5),
        ("Band 1 Gain", 0.6),
        ("Band 1 Q", 0.4),
        ("Band 1 Shape", 0.0), // Bell = 0
    ];

    println!("[proq4] Setting {} params in bulk...", targets.len());
    for (name, value) in &targets {
        fx.param_by_name(name).set(*value).await?;
    }

    // Single settle after all sets
    settle().await;

    // Read back all
    let mut pass_count = 0;
    let mut fail_count = 0;
    for (name, target) in &targets {
        let readback = fx.param_by_name(name).get().await?;
        let delta = (readback - target).abs();
        if delta < 0.02 {
            pass_count += 1;
            println!("[proq4]   {}: {:.4} -> {:.4} OK", name, target, readback);
        } else {
            fail_count += 1;
            println!(
                "[proq4]   {}: expected {:.4}, got {:.4} FAIL (delta={:.4})",
                name, target, readback, delta
            );
        }
    }

    println!(
        "[proq4] Bulk set result: {}/{} passed, {} failed",
        pass_count,
        targets.len(),
        fail_count
    );

    ctx.log(&format!(
        "bulk_set: {}/{} passed",
        pass_count,
        targets.len()
    ));

    Ok(())
}

// ---------------------------------------------------------------------------
// 5. State chunk save/restore — the known-working path
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_state_chunk_roundtrip(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 StateChunk", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    // CLAP plugins need extra time to initialize in the audio engine
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Capture default state
    let default_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get default state chunk — tag_chunk() returned None for CLAP plugin"))?;
    println!(
        "[proq4] Default state chunk: {} bytes",
        default_chunk.len()
    );
    assert!(
        default_chunk.len() > 10,
        "State chunk suspiciously small: {} bytes",
        default_chunk.len()
    );

    // Modify some params (by index to see if at least something changes)
    let params = fx.parameters().await?;
    let output_idx = params
        .iter()
        .find(|p| p.name == "Output Level")
        .map(|p| p.index)
        .unwrap_or(0);

    fx.param(output_idx).set(0.2).await?;
    settle().await;

    // Capture modified state
    let modified_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get modified state chunk"))?;
    println!(
        "[proq4] Modified state chunk: {} bytes",
        modified_chunk.len()
    );

    // Check if state actually changed
    let chunks_differ = default_chunk != modified_chunk;
    println!(
        "[proq4] State chunks differ after param set: {}",
        chunks_differ
    );

    // Restore original state
    fx.set_state_chunk(default_chunk.clone()).await?;
    settle().await;

    // Read back state after restore
    let restored_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("Failed to get restored state chunk"))?;

    let restore_matches = default_chunk == restored_chunk;
    println!(
        "[proq4] State restored to original: {}",
        restore_matches
    );

    if restore_matches {
        println!("[proq4] SUCCESS: state chunk roundtrip works");
    } else {
        println!("[proq4] WARN: restored chunk differs from original ({} vs {} bytes)",
            default_chunk.len(), restored_chunk.len());
    }

    ctx.log(&format!(
        "state_chunk: default={}B modified={}B chunks_differ={} restore_matches={}",
        default_chunk.len(),
        modified_chunk.len(),
        chunks_differ,
        restore_matches
    ));

    Ok(())
}

// ---------------------------------------------------------------------------
// 6. State chunk apply then verify params — does restoring a chunk
//    actually change the parameter values visible via get_parameter?
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_state_chunk_reflects_in_params(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 ChunkParams", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    // Read default param values
    let default_params = fx.parameters().await?;
    let output_idx = default_params
        .iter()
        .find(|p| p.name == "Output Level")
        .map(|p| p.index)
        .unwrap_or(0);
    let default_output = fx.param(output_idx).get().await?;
    println!(
        "[proq4] Default Output Level (idx {}): {:.6}",
        output_idx, default_output
    );

    // Save default chunk
    let default_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("No default chunk"))?;

    // Try to change output level via param set
    let new_value = if default_output < 0.5 { 0.8 } else { 0.2 };
    fx.param(output_idx).set(new_value).await?;
    settle().await;
    let after_set = fx.param(output_idx).get().await?;
    println!(
        "[proq4] After param set to {:.4}: readback={:.6}",
        new_value, after_set
    );

    // Now save this (possibly changed) chunk
    let changed_chunk = fx
        .state_chunk()
        .await?
        .ok_or_else(|| eyre::eyre!("No chunk after set"))?;

    // Restore default chunk
    fx.set_state_chunk(default_chunk.clone()).await?;
    settle().await;
    let after_restore = fx.param(output_idx).get().await?;
    println!(
        "[proq4] After restoring default chunk: Output Level={:.6} (expected {:.6})",
        after_restore, default_output
    );

    // Restore changed chunk
    fx.set_state_chunk(changed_chunk.clone()).await?;
    settle().await;
    let after_reapply = fx.param(output_idx).get().await?;
    println!(
        "[proq4] After restoring changed chunk: Output Level={:.6}",
        after_reapply
    );

    ctx.log(&format!(
        "chunk_params: default={:.6} after_set={:.6} after_restore={:.6} after_reapply={:.6}",
        default_output, after_set, after_restore, after_reapply
    ));

    Ok(())
}

// ---------------------------------------------------------------------------
// 7. Encoded (base64) state chunk roundtrip
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_encoded_chunk_roundtrip(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 Encoded", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    // Get encoded chunk
    let encoded = fx
        .state_chunk_encoded()
        .await?
        .ok_or_else(|| eyre::eyre!("No encoded state chunk"))?;
    println!(
        "[proq4] Encoded state chunk: {} chars (base64)",
        encoded.len()
    );

    // Set it back
    fx.set_state_chunk_encoded(encoded.clone()).await?;
    settle().await;

    // Verify roundtrip
    let after = fx
        .state_chunk_encoded()
        .await?
        .ok_or_else(|| eyre::eyre!("No encoded chunk after restore"))?;

    let matches = encoded == after;
    println!("[proq4] Encoded chunk roundtrip matches: {}", matches);

    ctx.log(&format!(
        "encoded_chunk: len={} roundtrip_matches={}",
        encoded.len(),
        matches
    ));

    Ok(())
}

// ---------------------------------------------------------------------------
// 8. Comprehensive parameter sweep — try setting every param and check which
//    ones actually respond to set_parameter
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_param_sweep(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 Sweep", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    settle().await;

    let params = fx.parameters().await?;
    println!("[proq4] Sweeping {} parameters...", params.len());

    let mut responsive = Vec::new();
    let mut unresponsive = Vec::new();

    // Test first 50 params (covers band 1-2 + globals)
    let test_count = params.len().min(50);

    for p in params.iter().take(test_count) {
        let original = fx.param(p.index).get().await?;
        let target = if original < 0.5 { 0.75 } else { 0.25 };

        fx.param(p.index).set(target).await?;
        // Brief settle per param
        tokio::time::sleep(Duration::from_millis(50)).await;

        let readback = fx.param(p.index).get().await?;
        let delta = (readback - target).abs();

        if delta < 0.02 {
            responsive.push((p.index, p.name.clone()));
        } else {
            unresponsive.push((p.index, p.name.clone(), original, target, readback));
        }

        // Restore original
        fx.param(p.index).set(original).await?;
    }

    println!("\n[proq4] === Parameter Sweep Results ===");
    println!(
        "[proq4] Responsive: {}/{} params",
        responsive.len(),
        test_count
    );
    for (idx, name) in &responsive {
        println!("[proq4]   OK [{:3}] {}", idx, name);
    }

    println!(
        "[proq4] Unresponsive: {}/{} params",
        unresponsive.len(),
        test_count
    );
    for (idx, name, orig, target, readback) in &unresponsive {
        println!(
            "[proq4]   FAIL [{:3}] {} (orig={:.4} target={:.4} readback={:.4})",
            idx, name, orig, target, readback
        );
    }

    ctx.log(&format!(
        "param_sweep: {}/{} responsive, {}/{} unresponsive",
        responsive.len(),
        test_count,
        unresponsive.len(),
        test_count
    ));

    Ok(())
}

// ---------------------------------------------------------------------------
// 9. Dump track chunk — diagnostic to see what CLAP FX looks like in RPP
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn proq4_dump_track_chunk(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();

    let track = project
        .tracks()
        .add("ProQ4 ChunkDump", None)
        .await?;
    let fx = track.fx_chain().add(CLAP_PROQ4).await?;
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Get the track chunk to see what CLAP looks like
    let track_chunk = track.get_chunk().await?;
    let chunk_len = track_chunk.len();

    // Print first 3000 chars (should include the FX block header)
    let preview = if chunk_len > 3000 {
        &track_chunk[..3000]
    } else {
        &track_chunk
    };
    println!("[proq4] Track chunk ({} chars):\n{}", chunk_len, preview);
    if chunk_len > 3000 {
        println!("[proq4] ... ({} more chars truncated)", chunk_len - 3000);
    }

    // Also try state_chunk directly
    match fx.state_chunk().await? {
        Some(chunk) => {
            println!("[proq4] state_chunk() returned {} bytes", chunk.len());
            let preview = String::from_utf8_lossy(&chunk[..chunk.len().min(500)]);
            println!("[proq4] state_chunk preview:\n{}", preview);
        }
        None => {
            println!("[proq4] state_chunk() returned None");
        }
    }

    // Try encoded
    match fx.state_chunk_encoded().await? {
        Some(enc) => {
            println!("[proq4] state_chunk_encoded() returned {} chars", enc.len());
            println!("[proq4] encoded preview: {}...", &enc[..enc.len().min(200)]);
        }
        None => {
            println!("[proq4] state_chunk_encoded() returned None");
        }
    }

    ctx.log(&format!("track_chunk: {} chars", chunk_len));
    Ok(())
}
