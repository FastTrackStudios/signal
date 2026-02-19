//! Test helpers that bridge `daw-control` types into the `signal-live` param bridge.
//!
//! This module sits at the intersection of the two worlds:
//! - `daw-control` — `TrackHandle`, `FxHandle`, `FxParameter`
//! - `signal-live` — `LiveParam`, `block_to_snapshot`, `graph_to_snapshot`, `MorphEngine`
//!
//! Nothing here contains domain logic — it's all mechanical conversion and
//! async convenience wrappers for use in REAPER integration tests.

use daw_control::{Daw, FxHandle, Project, TrackHandle};
use daw_proto::fx::tree::FxNodeKind;
use eyre::Result;
use signal::resolve::ResolvedGraph;
use signal::{
    block_to_snapshot, find_param_index, graph_state_chunks, graph_to_snapshot,
    live_params_into_block, Block, DawParameterSnapshot, LiveParam, MorphEngine,
};

// ─── Track discovery ─────────────────────────────────────────────────────────

/// Find a track by exact name, returning an error if not found.
pub async fn track_by_name(project: &Project, name: &str) -> Result<TrackHandle> {
    project
        .tracks()
        .by_name(name)
        .await?
        .ok_or_else(|| eyre::eyre!("track not found: '{name}'"))
}

/// Return all direct child tracks of a folder track using REAPER's folder depth.
///
/// REAPER doesn't expose parent GUIDs — folder hierarchy is determined by
/// `I_FOLDERDEPTH`: +1 = folder start, 0 = normal child, -1 = close folder.
/// Children are all tracks between the folder and its matching close, at depth 1.
pub async fn child_tracks(
    project: &Project,
    parent: &TrackHandle,
) -> Result<Vec<daw_proto::Track>> {
    let parent_info = parent.info().await?;
    let all = project.tracks().all().await?;

    // Find the folder track by index
    let folder_idx = parent_info.index as usize;
    let mut children = Vec::new();
    let mut depth = 0i32;

    for track in all.iter().skip(folder_idx + 1) {
        // First track after folder: depth goes to 1 (we're inside the folder)
        if depth == 0 {
            depth = 1;
        }

        // Accumulate folder depth changes
        depth += track.folder_depth;

        if depth <= 0 {
            // We've exited the folder
            break;
        }

        // Only collect direct children (depth == 1), not nested sub-folder contents
        if depth == 1 {
            children.push(track.clone());
        }
    }

    Ok(children)
}

// ─── FX chain helpers (any index) ────────────────────────────────────────────

/// List all FX on a track's chain with their indices and names.
pub async fn read_fx_list(track: &TrackHandle) -> Result<Vec<(u32, String)>> {
    let chain = track.fx_chain().all().await?;
    Ok(chain.into_iter().map(|fx| (fx.index, fx.name)).collect())
}

/// Get an FxHandle at a specific index, returning an error if missing.
pub async fn get_fx_at(track: &TrackHandle, index: u32) -> Result<FxHandle> {
    track
        .fx_chain()
        .by_index(index)
        .await?
        .ok_or_else(|| eyre::eyre!("no FX at index {index}"))
}

/// Read all parameters from an FX at a specific index as `LiveParam`s.
pub async fn read_live_params_at(track: &TrackHandle, fx_index: u32) -> Result<Vec<LiveParam>> {
    let fx = get_fx_at(track, fx_index).await?;
    let params = fx.parameters().await?;
    Ok(fx_params_to_live(&params))
}

/// Apply a `DawParameterSnapshot` to an FX at a specific index on a track.
pub async fn apply_snapshot_to_fx_at(
    track: &TrackHandle,
    fx_index: u32,
    snapshot: &DawParameterSnapshot,
) -> Result<()> {
    let fx = get_fx_at(track, fx_index).await?;
    for p in &snapshot.params {
        fx.param(p.param_index).set(p.value).await?;
    }
    Ok(())
}

/// Capture a snapshot from a specific FX index.
pub async fn capture_snapshot_at(
    track: &TrackHandle,
    fx_index: u32,
    fx_id: &str,
) -> Result<DawParameterSnapshot> {
    let fx = get_fx_at(track, fx_index).await?;
    let params = fx.parameters().await?;
    let values = params
        .iter()
        .map(|p| signal::DawParamValue {
            fx_id: fx_id.to_string(),
            param_index: p.index,
            param_name: p.name.clone(),
            value: p.value,
        })
        .collect();
    Ok(DawParameterSnapshot::new(values))
}

/// Randomize all continuous (non-toggle) parameters on an FX at a given index.
///
/// Returns `(old_params, new_params)` so callers can verify restore.
pub async fn randomize_fx_params(
    track: &TrackHandle,
    fx_index: u32,
) -> Result<(Vec<LiveParam>, Vec<LiveParam>)> {
    let fx = get_fx_at(track, fx_index).await?;
    let params = fx.parameters().await?;
    let old = fx_params_to_live(&params);

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    for p in &params {
        if p.is_toggle {
            continue;
        }
        // Deterministic pseudo-random based on param index + FX index
        let mut hasher = DefaultHasher::new();
        (fx_index, p.index, "randomize").hash(&mut hasher);
        let hash = hasher.finish();
        let random_val = (hash % 10000) as f64 / 10000.0;
        fx.param(p.index).set(random_val).await?;
    }

    let new_params = fx.parameters().await?;
    let new = fx_params_to_live(&new_params);
    Ok((old, new))
}

/// Walk the FX tree to find the first leaf plugin with at least `min_params` parameters.
///
/// Returns `(fx_index, fx_name)` — the raw index suitable for `get_fx_at`,
/// `capture_snapshot_at`, etc. Unlike `read_fx_list()`, this peers inside
/// containers to find the actual plugin FX nodes.
pub async fn find_leaf_plugin_with_params(
    track: &TrackHandle,
    min_params: u32,
) -> Result<(u32, String)> {
    let tree = track.fx_chain().tree().await?;
    for (_depth, node) in tree.iter_depth_first() {
        if let FxNodeKind::Plugin(fx) = &node.kind {
            if fx.parameter_count >= min_params {
                return Ok((fx.index, fx.name.clone()));
            }
        }
    }
    Err(eyre::eyre!(
        "no leaf plugin with >= {} params found in FX tree",
        min_params
    ))
}

// ─── Conversion ──────────────────────────────────────────────────────────────

/// Convert a slice of `daw_proto::FxParameter` into `LiveParam`s for the signal-live bridge.
pub fn fx_params_to_live(params: &[daw_control::FxParameter]) -> Vec<LiveParam> {
    params
        .iter()
        .map(|p| LiveParam {
            index: p.index,
            name: p.name.clone(),
            value: p.value,
        })
        .collect()
}

// ─── FX helpers ──────────────────────────────────────────────────────────────

/// Get FX index 0 from a track, returning an error if none is loaded.
pub async fn get_fx0(track: &TrackHandle) -> Result<FxHandle> {
    track
        .fx_chain()
        .by_index(0)
        .await?
        .ok_or_else(|| eyre::eyre!("no FX at index 0"))
}

/// Read all parameters from FX index 0 as `LiveParam`s.
pub async fn read_live_params(track: &TrackHandle) -> Result<Vec<LiveParam>> {
    let fx = get_fx0(track).await?;
    let params = fx.parameters().await?;
    Ok(fx_params_to_live(&params))
}

/// Apply a `DawParameterSnapshot` to FX index 0 on a track.
///
/// Only parameters present in the snapshot are written; all others are untouched.
pub async fn apply_snapshot_to_fx(
    track: &TrackHandle,
    snapshot: &DawParameterSnapshot,
) -> Result<()> {
    let fx = get_fx0(track).await?;
    for p in &snapshot.params {
        fx.param(p.param_index).set(p.value).await?;
    }
    Ok(())
}

// ─── Block / Graph apply ─────────────────────────────────────────────────────

/// Apply a domain `Block`'s parameters to FX index 0 on a track.
///
/// Returns the number of parameters matched and applied.
pub async fn apply_block(track: &TrackHandle, block: &Block, fx_id: &str) -> Result<usize> {
    let live = read_live_params(track).await?;
    let (snapshot, count) = block_to_snapshot(block, &live, fx_id);
    apply_snapshot_to_fx(track, &snapshot).await?;
    Ok(count)
}

/// Apply a fully resolved `ResolvedGraph` to FX index 0 on a track.
///
/// Walks engines → layers → modules → blocks, matching each block parameter
/// to a live DAW parameter by name. Returns the total count applied.
pub async fn apply_graph(track: &TrackHandle, graph: &ResolvedGraph, fx_id: &str) -> Result<usize> {
    let live = read_live_params(track).await?;
    let (snapshot, count) = graph_to_snapshot(graph, &live, fx_id);
    apply_snapshot_to_fx(track, &snapshot).await?;
    Ok(count)
}

/// Apply a resolved graph that carries binary state data.
///
/// If the graph has state chunks (e.g. from a catalog `.bin` file), loads
/// them directly via `set_state_chunk` — this sets ALL plugin params at once
/// and shows the correct preset name in the plugin UI. Falls back to
/// param-by-param matching if no state data is present.
///
/// Returns `true` if a state chunk was loaded, `false` if param-by-param was used.
pub async fn apply_graph_with_state(
    track: &TrackHandle,
    graph: &ResolvedGraph,
    fx_id: &str,
) -> Result<bool> {
    let chunks = graph_state_chunks(graph, fx_id);
    if let Some(chunk) = chunks.first() {
        let fx = get_fx0(track).await?;
        fx.set_state_chunk(chunk.chunk_data.clone()).await?;
        Ok(true)
    } else {
        apply_graph(track, graph, fx_id).await?;
        Ok(false)
    }
}

// ─── Snapshot capture ────────────────────────────────────────────────────────

/// Capture the current FX index 0 parameter values as a `DawParameterSnapshot`.
pub async fn capture_snapshot(track: &TrackHandle, fx_id: &str) -> Result<DawParameterSnapshot> {
    let fx = get_fx0(track).await?;
    let params = fx.parameters().await?;
    let values = params
        .iter()
        .map(|p| signal::DawParamValue {
            fx_id: fx_id.to_string(),
            param_index: p.index,
            param_name: p.name.clone(),
            value: p.value,
        })
        .collect();
    Ok(DawParameterSnapshot::new(values))
}

/// Capture live FX params and map them back onto a domain `Block`.
///
/// Each block parameter whose ID matches a live param name gets overwritten
/// with the live value. Unmatched parameters are left at their stored values.
pub async fn capture_block_from_fx(track: &TrackHandle, template: Block) -> Result<Block> {
    let live = read_live_params(track).await?;
    Ok(live_params_into_block(template, &live))
}

// ─── Morph helpers ───────────────────────────────────────────────────────────

/// Build a `MorphEngine` pre-loaded with snapshots A and B captured from the
/// live FX after applying `block_a` and `block_b` in sequence.
///
/// Returns `(engine, snap_a, snap_b)`.
pub async fn build_morph_engine(
    track: &TrackHandle,
    block_a: &Block,
    block_b: &Block,
    fx_id: &str,
) -> Result<(MorphEngine, DawParameterSnapshot, DawParameterSnapshot)> {
    apply_block(track, block_a, fx_id).await?;
    let snap_a = capture_snapshot(track, fx_id).await?;

    apply_block(track, block_b, fx_id).await?;
    let snap_b = capture_snapshot(track, fx_id).await?;

    let mut engine = MorphEngine::new();
    engine.set_a(snap_a.clone());
    engine.set_b(snap_b.clone());

    Ok((engine, snap_a, snap_b))
}

// ─── Gain readback ───────────────────────────────────────────────────────────

/// Read the current value of the first parameter whose name contains "gain"
/// (case-insensitive) from FX index 0. Returns `None` if no such param exists.
pub async fn read_gain(track: &TrackHandle) -> Result<Option<f64>> {
    let fx = get_fx0(track).await?;
    let params = fx.parameters().await?;
    let live = fx_params_to_live(&params);
    if let Some(idx) = find_param_index(&live, "gain") {
        Ok(Some(fx.param(idx).get().await?))
    } else {
        Ok(None)
    }
}

// ─── RfxChain capture ────────────────────────────────────────────────────────

/// Capture the FXCHAIN inner content from a track as bytes.
///
/// Gets the full RPP track chunk, extracts the `<FXCHAIN ...>` block, strips
/// the header and closing `>`, and returns the inner content as UTF-8 bytes.
/// This is the format expected by `ReaperPatchApplier::splice_fxchain`.
pub async fn capture_rfxchain_bytes(track: &TrackHandle) -> Result<Vec<u8>> {
    let chunk = track.get_chunk().await.map_err(|e| eyre::eyre!(e))?;
    let inner = extract_fxchain_inner(&chunk)
        .ok_or_else(|| eyre::eyre!("no FXCHAIN block found in track chunk"))?;
    Ok(inner.as_bytes().to_vec())
}

/// Extract the inner content of an `<FXCHAIN ...>` block from a track chunk.
///
/// Returns everything between the first header line and the matching closing `>`.
fn extract_fxchain_inner(track_chunk: &str) -> Option<String> {
    let start = track_chunk.find("<FXCHAIN")?;
    let after = &track_chunk[start..];

    // Skip the `<FXCHAIN` header line
    let first_newline = after.find('\n')? + 1;

    // Find the matching closing `>` by counting bracket depth
    let mut depth = 1i32; // we're already inside the opening <FXCHAIN
    let mut end_byte = None;
    for (i, line) in after[first_newline..].split('\n').enumerate() {
        if line.trim_start().starts_with('<') {
            depth += 1;
        }
        if line.trim() == ">" {
            depth -= 1;
            if depth == 0 {
                // Calculate byte offset within after[first_newline..]
                let byte_pos: usize = after[first_newline..]
                    .split('\n')
                    .take(i)
                    .map(|l| l.len() + 1)
                    .sum();
                end_byte = Some(byte_pos);
                break;
            }
        }
    }

    let end = end_byte?;
    let inner = &after[first_newline..first_newline + end];

    // Strip SHOW/LASTSEL/DOCKED header lines that splice_fxchain adds back
    let mut content = String::new();
    for line in inner.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("SHOW ")
            || trimmed.starts_with("LASTSEL ")
            || trimmed.starts_with("DOCKED ")
        {
            continue;
        }
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(line);
    }
    Some(content)
}

// ─── Track creation ──────────────────────────────────────────────────────────

const JM_PLUGIN_NAME: &str = "Archetype John Mayer X";

/// Create a new track with the JM plugin loaded, polling until the FX appears.
///
/// VST3 loads asynchronously on REAPER's side — this polls up to 10s.
pub async fn add_jm_track(project: &Project, name: &str) -> Result<TrackHandle> {
    let track = project.tracks().add(name, None).await?;
    track.fx_chain().add(JM_PLUGIN_NAME).await?;

    for _ in 0..40 {
        let fx_list = track.fx_chain().all().await?;
        if !fx_list.is_empty() {
            return Ok(track);
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Err(eyre::eyre!(
        "Timed out waiting for JM plugin to load on track '{name}'"
    ))
}

/// No-op track removal — track removals cause REAPER to close the Unix socket.
/// The project tab is closed by the test framework when the test finishes.
pub async fn remove_track(_project: &Project, _track: TrackHandle) {}
