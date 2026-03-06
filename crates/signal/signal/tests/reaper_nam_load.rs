//! REAPER integration test: Load a NAM model into the Neural Amp Modeler plugin
//! via nam-manager's catalog and VST chunk rewriting.
//!
//! Run with:
//!   cargo xtask reaper-test nam

use std::path::PathBuf;
use std::time::Duration;

use nam_manager::{
    scan_directory, merge_into_catalog, NamCatalog,
    resolve_path, nam_root_from_env,
    vst_chunk::{decode_chunk, encode_chunk, rewrite_paths},
};
use reaper_test::reaper_test;

/// Small sleep to let REAPER/CLAP process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Ensure REAPER's audio engine is running.
async fn ensure_audio(ctx: &reaper_test::ReaperTestContext) {
    if !ctx.daw.audio_engine().is_running().await.unwrap_or(false) {
        let _ = ctx.daw.audio_engine().init().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Default NAM root path (signal-library/nam in the parent repo).
fn default_nam_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../signal-library/nam")
}

/// Extract the base64 plugin state lines from a REAPER chunk block.
///
/// REAPER VST3 chunks have the format:
/// ```text
/// <VST "VST3: Plugin (Vendor)" plugin.vst3 0 "" {GUID} ""
///   <main base64 state (may end with = padding)>
///   <extra base64 segment (trailing data)>
/// >
/// ```
/// The main state is the first base64 block (all lines up to one ending with `=`).
/// Additional lines after padding are separate REAPER metadata, not part of the plugin state.
fn extract_state_base64(chunk: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = chunk.lines().collect();
    if lines.len() < 3 {
        return None;
    }
    let data_lines: Vec<String> = lines[1..lines.len() - 1]
        .iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if data_lines.is_empty() {
        return None;
    }
    Some(data_lines)
}

/// Extract the first base64 segment from REAPER chunk data lines.
///
/// REAPER VST3 chunks contain the plugin state as the first base64 segment
/// (all lines concatenated until one ends with `=` padding). Lines after the
/// padding are REAPER's own metadata (e.g. `AAAAAAAA` = 8 zero bytes for
/// plugin-specific REAPER state), not part of the plugin's binary state.
fn first_base64_segment(segments: &[String]) -> String {
    let mut result = String::new();
    for line in segments {
        result.push_str(line);
        if line.ends_with('=') {
            break; // First segment complete
        }
    }
    result
}

/// Rebuild the full REAPER chunk block with new base64 state data.
///
/// Preserves the header line (first) and any trailing REAPER metadata lines
/// (segments after the first `=`-terminated base64 segment), replacing only
/// the plugin state portion.
fn rebuild_chunk_with_state(chunk: &str, new_b64: &str) -> String {
    let lines: Vec<&str> = chunk.lines().collect();
    let header = lines.first().copied().unwrap_or("");

    // Find trailing REAPER metadata lines (after the first base64 segment ends with `=`)
    let data_lines: Vec<&str> = lines[1..lines.len().saturating_sub(1)]
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    let mut trailing: Vec<&str> = Vec::new();
    let mut found_end = false;
    for line in &data_lines {
        if found_end {
            trailing.push(line);
        } else if line.ends_with('=') {
            found_end = true;
        }
    }

    // Build result: header + new base64 lines + trailing metadata + closing >
    let mut result = String::from(header);
    result.push('\n');
    for chunk_line in new_b64.as_bytes().chunks(128) {
        result.push_str("  ");
        result.push_str(&String::from_utf8_lossy(chunk_line));
        result.push('\n');
    }
    for t in &trailing {
        result.push_str("  ");
        result.push_str(t);
        result.push('\n');
    }
    result.push('>');
    result
}

// ---------------------------------------------------------------------------
// Test: Scan catalog, add NAM VST3 plugin, rewrite chunk to load a model
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn nam_load_model_via_chunk_rewrite(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();

    // Step 1: Discover NAM models from signal-library
    let nam_root = nam_root_from_env(&default_nam_root());
    let nam_root = std::fs::canonicalize(&nam_root)
        .map_err(|e| eyre::eyre!("NAM root not found at {}: {e}", nam_root.display()))?;

    ctx.log(&format!("NAM root: {}", nam_root.display()));

    let scanned = scan_directory(&nam_root)
        .map_err(|e| eyre::eyre!("Failed to scan NAM directory: {e}"))?;

    let mut catalog = NamCatalog::new();
    merge_into_catalog(&mut catalog, scanned);

    let amp_models = catalog.amp_models();
    assert!(
        !amp_models.is_empty(),
        "Should find at least one .nam amp model in signal-library/nam"
    );

    let model = amp_models[0];
    let model_path = resolve_path(&catalog, &model.hash, &nam_root)
        .map_err(|e| eyre::eyre!("Failed to resolve model path: {e}"))?;

    ctx.log(&format!(
        "Selected model: {} ({})",
        model.filename,
        model.architecture.as_deref().unwrap_or("unknown arch")
    ));

    // Step 2: Add NAM plugin to a track
    let track = project.tracks().add("NAM Load Test", None).await?;
    settle().await;

    let fx = track
        .fx_chain()
        .add("VST3: NeuralAmpModeler (Steven Atkinson)")
        .await?;
    settle().await;

    // Verify plugin loaded
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(fx_count, 1, "should have exactly 1 FX on the track");

    let info = fx.info().await?;
    ctx.log(&format!("FX loaded: {}", info.name));

    // Step 3: Get the REAPER chunk and extract the plugin state base64
    let reaper_chunk = fx.state_chunk_encoded().await?
        .ok_or_else(|| eyre::eyre!("Failed to get NAM plugin state chunk"))?;

    let segments = extract_state_base64(&reaper_chunk)
        .ok_or_else(|| eyre::eyre!("Failed to extract base64 state from REAPER chunk"))?;
    let unified_b64 = first_base64_segment(&segments);

    // Step 4: Decode NAM chunk, rewrite model path, re-encode
    let mut nam_chunk = decode_chunk(unified_b64.trim())
        .map_err(|e| eyre::eyre!("Failed to decode NAM chunk: {e}"))?;

    ctx.log(&format!(
        "Decoded — plugin_id: {}, version: {}, model: '{}'",
        nam_chunk.plugin_id, nam_chunk.version, nam_chunk.model_path
    ));

    rewrite_paths(
        &mut nam_chunk,
        Some(model_path.to_str().unwrap()),
        None, // no IR for this test
    );

    ctx.log(&format!("Rewritten model path: '{}'", nam_chunk.model_path));

    let new_state_b64 = encode_chunk(&nam_chunk);

    // Step 5: Rebuild the full REAPER chunk and apply
    let new_reaper_chunk = rebuild_chunk_with_state(&reaper_chunk, &new_state_b64);
    fx.set_state_chunk_encoded(new_reaper_chunk).await?;
    // Give the plugin time to load the model (neural net inference init)
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Step 6: Verify — read back and confirm the model path persisted
    let readback_chunk = fx.state_chunk_encoded().await?
        .ok_or_else(|| eyre::eyre!("Failed to read back state chunk after rewrite"))?;

    let readback_segments = extract_state_base64(&readback_chunk)
        .ok_or_else(|| eyre::eyre!("Failed to extract readback base64"))?;
    let readback_unified = first_base64_segment(&readback_segments);

    let readback_nam = decode_chunk(&readback_unified)
        .map_err(|e| eyre::eyre!("Failed to decode readback chunk: {e}"))?;

    ctx.log(&format!("Readback model path: '{}'", readback_nam.model_path));

    assert_eq!(
        readback_nam.model_path,
        model_path.to_str().unwrap(),
        "Model path should persist after chunk rewrite"
    );

    assert!(
        readback_nam.model_path.ends_with(".nam"),
        "Model path should end with .nam, got '{}'",
        readback_nam.model_path
    );

    ctx.log(&format!(
        "nam_load_model_via_chunk_rewrite: PASS — loaded '{}' into NAM plugin",
        model.filename
    ));
    Ok(())
}
