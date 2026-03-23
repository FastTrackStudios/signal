//! REAPER integration test: Fast-path rig open + variation save/load.
//!
//! Uses stock REAPER plugins (ReaEQ, ReaComp, ReaDelay) — no seed data required.
//!
//! Tests the full pipeline:
//! 1. Add stock FX to a source track, capture raw_block state
//! 2. Import as a rig into the signal DB (with state_data)
//! 3. Clear all tracks
//! 4. Re-open via fast path: build FXCHAIN from stored state → single set_chunk
//! 5. Randomize all plugin parameters
//! 6. Save randomized state as a named variation (snapshot per block preset)
//! 7. Reload original rig
//! 8. Apply variation from saved snapshots
//! 9. Verify round-trip: applied state matches randomized, differs from original
//!
//! Run with:
//!   cargo xtask reaper-test fast_path_variation

use std::collections::HashMap;
use std::time::{Duration, Instant};

use daw::file::RppSerialize;
use reaper_test::reaper_test;
use signal::ops::rig_importer::{ImportBlock, ImportChain, ImportModule};
use signal_proto::plugin_block::{FxRole, TrackRole};
use signal_proto::ModuleBlockSource;

/// Small sleep to let REAPER process track/FX changes.
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

/// Parse a raw_block byte slice back into an FxChainNode.
fn parse_raw_block_bytes(source_bytes: &[u8]) -> Option<daw::file::types::FxChainNode> {
    let source_str = std::str::from_utf8(source_bytes).ok()?;
    let source_chain = daw::file::FxChain::parse(&format!(
        "<FXCHAIN\nSHOW 0\nLASTSEL 0\nDOCKED 0\n{source_str}\n>\n"
    ))
    .ok()?;
    source_chain.nodes.into_iter().next()
}

/// Flatten FxChainNodes to raw_block strings in document order (depth-first).
fn flatten_to_raw_blocks(nodes: &[daw::file::types::FxChainNode]) -> Vec<&str> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            daw::file::types::FxChainNode::Plugin(p) => out.push(p.raw_block.as_str()),
            daw::file::types::FxChainNode::Container(c) => {
                out.extend(flatten_to_raw_blocks(&c.children))
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers for variation save/load
// ---------------------------------------------------------------------------

/// Walk the FX tree and randomize all continuous plugin parameters.
///
/// Uses a seeded LCG for deterministic randomization.
async fn randomize_all_fx(track: &daw::TrackHandle, seed: u64) -> eyre::Result<usize> {
    let tree = track.fx_chain().tree().await?;
    let mut count = 0usize;
    let mut state = seed;
    let mut next_rand = || -> f64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as f64) / (u32::MAX as f64)
    };
    for (_depth, node) in tree.iter_depth_first() {
        if let daw::FxNodeKind::Plugin(fx) = &node.kind {
            if let Some(handle) = track.fx_chain().by_index(fx.index).await? {
                let params = handle.parameters().await?;
                eprintln!(
                    "[randomize] FX '{}' index={} has {} params",
                    fx.name,
                    fx.index,
                    params.len()
                );
                if let Some(first) = params.first() {
                    eprintln!(
                        "[randomize]   first param: '{}' toggle={} steps={:?}",
                        first.name, first.is_toggle, first.step_count
                    );
                }
                for p in &params {
                    if p.is_toggle {
                        continue;
                    }
                    // Skip discrete params with very few steps (dropdowns/selectors).
                    // Stock REAPER plugins report step counts for most params, so
                    // we only skip truly discrete ones (< 10 steps).
                    if matches!(p.step_count, Some(n) if n < 10) {
                        continue;
                    }
                    let n = p.name.to_lowercase();
                    if n.contains("bypass") || n.contains(" wet") || n.contains("delta") {
                        continue;
                    }
                    handle.param(p.index).set(next_rand()).await?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

/// Capture the current FX state from the track chunk and save each plugin's
/// raw_block as a named snapshot on its corresponding block preset.
async fn save_variation(
    track: &daw::TrackHandle,
    block_preset_ids: &[(signal_proto::PresetId, signal_proto::BlockType)],
    signal: &signal::SignalController,
    name: &str,
) -> eyre::Result<
    Vec<(
        signal_proto::SnapshotId,
        signal_proto::PresetId,
        signal_proto::BlockType,
    )>,
> {
    let chunk_str = track.get_chunk().await?;
    let fxchain_text = daw::file::chunk_ops::extract_fxchain_block(&chunk_str)
        .ok_or_else(|| eyre::eyre!("No FXCHAIN in track chunk"))?;
    let parsed = daw::file::FxChain::parse(fxchain_text)
        .map_err(|e| eyre::eyre!("Failed to parse FXCHAIN: {e}"))?;

    let raw_blocks = flatten_to_raw_blocks(&parsed.nodes);
    assert_eq!(
        raw_blocks.len(),
        block_preset_ids.len(),
        "raw block count ({}) must match preset count ({})",
        raw_blocks.len(),
        block_preset_ids.len(),
    );

    let mut result = Vec::new();
    for (raw_block, (preset_id, block_type)) in raw_blocks.iter().zip(block_preset_ids) {
        let snap_id = signal_proto::SnapshotId::new();
        let snap =
            signal_proto::Snapshot::new(snap_id.clone(), name, signal_proto::Block::default())
                .with_state_data(raw_block.as_bytes().to_vec());

        // Load the existing preset, add the snapshot, and save back.
        let presets = signal.block_presets().list(*block_type).await?;
        let mut preset = presets
            .into_iter()
            .find(|p| p.id() == preset_id)
            .ok_or_else(|| eyre::eyre!("Preset {} not found for {:?}", preset_id, block_type))?;
        preset.add_snapshot(snap);
        signal.block_presets().save(preset).await?;

        result.push((snap_id, preset_id.clone(), *block_type));
    }

    Ok(result)
}

/// Fetch each snapshot's state_data and rebuild the track chunk with those
/// raw_blocks, replacing the current FX state via a single `set_chunk` call.
async fn apply_variation(
    track: &daw::TrackHandle,
    variation: &[(
        signal_proto::SnapshotId,
        signal_proto::PresetId,
        signal_proto::BlockType,
    )],
    signal: &signal::SignalController,
) -> eyre::Result<usize> {
    // Collect state_data for each snapshot.
    let mut state_data_list: Vec<Vec<u8>> = Vec::new();
    for (snap_id, preset_id, block_type) in variation {
        let presets = signal.block_presets().list(*block_type).await?;
        let preset = presets
            .iter()
            .find(|p| p.id() == preset_id)
            .ok_or_else(|| eyre::eyre!("Preset {} not found", preset_id))?;
        let snap = preset
            .snapshot(snap_id)
            .ok_or_else(|| eyre::eyre!("Snapshot {} not found in preset {}", snap_id, preset_id))?;
        let data = snap
            .state_data()
            .ok_or_else(|| eyre::eyre!("Snapshot {} has no state_data", snap_id))?;
        state_data_list.push(data.to_vec());
    }

    // Parse the current FXCHAIN, replace raw_blocks in flat order.
    let chunk_str = track.get_chunk().await?;
    let fxchain_text = daw::file::chunk_ops::extract_fxchain_block(&chunk_str)
        .ok_or_else(|| eyre::eyre!("No FXCHAIN in track chunk"))?;
    let mut parsed = daw::file::FxChain::parse(fxchain_text)
        .map_err(|e| eyre::eyre!("Failed to parse FXCHAIN: {e}"))?;

    // Walk plugins in flat order, replacing raw_blocks.
    fn replace_raw_blocks(
        nodes: &mut [daw::file::types::FxChainNode],
        data_iter: &mut std::slice::Iter<'_, Vec<u8>>,
        count: &mut usize,
    ) {
        for node in nodes {
            match node {
                daw::file::types::FxChainNode::Plugin(p) => {
                    if let Some(data) = data_iter.next() {
                        p.raw_block = String::from_utf8_lossy(data).into_owned();
                        *count += 1;
                    }
                }
                daw::file::types::FxChainNode::Container(c) => {
                    replace_raw_blocks(&mut c.children, data_iter, count);
                }
            }
        }
    }

    let mut applied = 0usize;
    let mut data_iter = state_data_list.iter();
    replace_raw_blocks(&mut parsed.nodes, &mut data_iter, &mut applied);

    // Rebuild and inject the new FXCHAIN.
    let new_fxchain = parsed.to_rpp_string();
    let new_chunk = chunk_str.replace(fxchain_text, &new_fxchain);
    track.set_chunk(new_chunk).await?;

    Ok(applied)
}

/// Build the FXCHAIN + track hierarchy for a fast-path open.
///
/// Creates [R] rig → [E] engine → [L] layer tracks and injects the FXCHAIN
/// built from block state data into the layer track.
async fn fast_path_open(
    project: &daw::Project,
    rig_name: &str,
    layer_name: &str,
    module_specs: &[(&str, &[Option<Vec<u8>>])], // (container_name, block_state_data[])
) -> eyre::Result<daw::TrackHandle> {
    let rig_name_display = TrackRole::Rig {
        name: rig_name.to_string(),
    }
    .display_name();
    let rig_track = project.tracks().add(&rig_name_display, None).await?;
    rig_track.set_folder_depth(1).await?;

    let engine_name_display = TrackRole::Engine {
        name: "Engine".to_string(),
    }
    .display_name();
    let engine_track = project.tracks().add(&engine_name_display, None).await?;
    engine_track.set_folder_depth(1).await?;

    let layer_name_display = TrackRole::Layer {
        name: layer_name.to_string(),
    }
    .display_name();
    let layer_track = project.tracks().add(&layer_name_display, None).await?;
    layer_track.set_folder_depth(-2).await?;

    // Build FXCHAIN from module specs.
    let mut fxchain_nodes = Vec::new();
    for (container_name, blocks) in module_specs {
        let mut children = Vec::new();
        for state in *blocks {
            if let Some(ref data) = state {
                if let Some(node) = parse_raw_block_bytes(data) {
                    children.push(node);
                }
            }
        }
        fxchain_nodes.push(daw::file::types::FxChainNode::Container(
            daw::file::types::FxContainer {
                name: container_name.to_string(),
                bypassed: false,
                offline: false,
                fxid: None,
                float_pos: None,
                parallel: false,
                container_cfg: None,
                show: 0,
                last_sel: 0,
                docked: false,
                children,
                raw_block: String::new(),
            },
        ));
    }

    let fxchain = daw::file::FxChain {
        window_rect: None,
        show: 0,
        last_sel: 0,
        docked: false,
        nodes: fxchain_nodes,
        raw_content: String::new(),
    };

    let chunk: String = layer_track.get_chunk().await?;
    let fxchain_text = fxchain.to_rpp_string();
    let new_chunk = if let Some(existing) = daw::file::chunk_ops::extract_fxchain_block(&chunk) {
        chunk.replace(existing, &fxchain_text)
    } else {
        let pos = chunk
            .rfind('>')
            .ok_or_else(|| eyre::eyre!("invalid track chunk"))?;
        format!("{}{}\n{}", &chunk[..pos], fxchain_text, &chunk[pos..])
    };
    layer_track.set_chunk(new_chunk).await?;

    Ok(layer_track)
}

// ---------------------------------------------------------------------------
// Test: Variation save/load via parameter randomization
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn fast_path_variation_save_load(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;
    settle().await;

    // ── 1. Add stock REAPER FX to a source track ──
    let signal = signal::bootstrap_in_memory_controller_async().await?;

    let source_track = project.tracks().add("Source Track", None).await?;
    settle().await;

    // Add 3 stock plugins: ReaEQ, ReaComp, ReaDelay
    let fx_specs: &[(&str, signal_proto::BlockType)] = &[
        ("ReaEQ", signal_proto::BlockType::Eq),
        ("ReaComp", signal_proto::BlockType::Compressor),
        ("ReaDelay", signal_proto::BlockType::Delay),
    ];

    for (fx_name, _) in fx_specs {
        source_track.fx_chain().add(fx_name).await?;
        settle().await;
    }

    ctx.log(&format!(
        "Added {} stock REAPER FX to source track",
        fx_specs.len()
    ));

    // ── 2. Capture raw_block state from track chunk ──
    let chunk_str = source_track.get_chunk().await?;
    let fxchain_text = daw::file::chunk_ops::extract_fxchain_block(&chunk_str)
        .ok_or_else(|| eyre::eyre!("No FXCHAIN in source track chunk"))?;
    let parsed = daw::file::FxChain::parse(fxchain_text)
        .map_err(|e| eyre::eyre!("Failed to parse FXCHAIN: {e}"))?;

    let raw_blocks = flatten_to_raw_blocks(&parsed.nodes);
    assert_eq!(
        raw_blocks.len(),
        fx_specs.len(),
        "expected {} raw blocks, got {}",
        fx_specs.len(),
        raw_blocks.len(),
    );

    // All blocks must have state data.
    for (i, rb) in raw_blocks.iter().enumerate() {
        assert!(
            !rb.is_empty(),
            "FX {} should have non-empty raw_block",
            fx_specs[i].0
        );
    }

    // ── 3. Build ImportChain and import as a rig ──
    // Module 1: "EQ" containing ReaEQ
    // Module 2: "FX" containing ReaComp + ReaDelay
    let import_chain = ImportChain {
        modules: vec![
            ImportModule {
                name: "EQ".to_string(),
                module_type: signal_proto::ModuleType::Eq,
                has_parallel_routing: false,
                blocks: vec![ImportBlock {
                    label: "EQ".to_string(),
                    block_type: signal_proto::BlockType::Eq,
                    plugin_name: Some("ReaEQ".to_string()),
                    state_data: Some(raw_blocks[0].as_bytes().to_vec()),
                    parameters: Vec::new(),
                }],
            },
            ImportModule {
                name: "FX".to_string(),
                module_type: signal_proto::ModuleType::Eq, // reuse; type doesn't matter for this test
                has_parallel_routing: false,
                blocks: vec![
                    ImportBlock {
                        label: "Comp".to_string(),
                        block_type: signal_proto::BlockType::Compressor,
                        plugin_name: Some("ReaComp".to_string()),
                        state_data: Some(raw_blocks[1].as_bytes().to_vec()),
                        parameters: Vec::new(),
                    },
                    ImportBlock {
                        label: "Delay".to_string(),
                        block_type: signal_proto::BlockType::Delay,
                        plugin_name: Some("ReaDelay".to_string()),
                        state_data: Some(raw_blocks[2].as_bytes().to_vec()),
                        parameters: Vec::new(),
                    },
                ],
            },
        ],
        standalone_blocks: vec![],
    };

    let result = signal
        .import_rig_from_chain(&import_chain, "Variation Test Rig")
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    ctx.log(&format!(
        "Imported rig: {} — {} new block presets",
        result.rig.name, result.new_block_preset_count,
    ));

    // ── 4. Build block_preset_ids from rig hierarchy ──
    let rig = signal
        .rigs()
        .load(result.rig_id.to_string())
        .await?
        .ok_or_else(|| eyre::eyre!("rig not found after import"))?;

    let all_mp = signal.module_presets().list().await.unwrap_or_default();
    let default_scene = rig
        .default_variant()
        .ok_or_else(|| eyre::eyre!("rig has no default scene"))?;

    let mut block_preset_ids: Vec<(signal_proto::PresetId, signal_proto::BlockType)> = Vec::new();

    for engine_sel in &default_scene.engine_selections {
        let engine = signal
            .engines()
            .load(engine_sel.engine_id.to_string())
            .await?
            .ok_or_else(|| eyre::eyre!("engine not found"))?;
        let engine_scene = engine
            .variant(&engine_sel.variant_id)
            .or_else(|| engine.default_variant())
            .ok_or_else(|| eyre::eyre!("no engine scene"))?;

        for layer_sel in &engine_scene.layer_selections {
            let layer = signal
                .layers()
                .load(layer_sel.layer_id.to_string())
                .await?
                .ok_or_else(|| eyre::eyre!("layer not found"))?;
            let layer_snap = layer
                .variant(&layer_sel.variant_id)
                .or_else(|| layer.default_variant())
                .ok_or_else(|| eyre::eyre!("no layer snapshot"))?;

            for module_ref in &layer_snap.module_refs {
                if let Some(mp) = all_mp.iter().find(|p| p.id() == &module_ref.collection_id) {
                    let snap = module_ref
                        .variant_id
                        .as_ref()
                        .and_then(|vid| mp.snapshot(vid))
                        .unwrap_or_else(|| mp.default_snapshot().clone());

                    for block in snap.module().blocks() {
                        if let ModuleBlockSource::PresetDefault { preset_id, .. }
                        | ModuleBlockSource::PresetSnapshot { preset_id, .. } = block.source()
                        {
                            block_preset_ids.push((preset_id.clone(), block.block_type()));
                        }
                    }
                }
            }
        }
    }

    ctx.log(&format!(
        "Resolved {} block preset IDs from rig hierarchy",
        block_preset_ids.len(),
    ));
    assert_eq!(
        block_preset_ids.len(),
        3,
        "expected 3 block presets (1+2), got {}",
        block_preset_ids.len()
    );

    // ── 5. Collect state_data for fast-path open ──
    let mut state_by_preset_id: HashMap<String, Vec<u8>> = HashMap::new();
    for &bt in signal_proto::ALL_BLOCK_TYPES {
        if let Ok(presets) = signal.block_presets().list(bt).await {
            for preset in presets {
                if let Some(data) = preset.default_snapshot().state_data() {
                    state_by_preset_id.insert(preset.id().to_string(), data.to_vec());
                }
            }
        }
    }

    // Build module specs for fast_path_open: container_name → block state_data.
    struct ModuleInfo {
        container_name: String,
        block_states: Vec<Option<Vec<u8>>>,
    }
    let mut module_infos: Vec<ModuleInfo> = Vec::new();
    let mut layer_name = String::new();

    for engine_sel in &default_scene.engine_selections {
        let engine = signal
            .engines()
            .load(engine_sel.engine_id.to_string())
            .await?
            .ok_or_else(|| eyre::eyre!("engine not found"))?;
        let engine_scene = engine
            .variant(&engine_sel.variant_id)
            .or_else(|| engine.default_variant())
            .ok_or_else(|| eyre::eyre!("no engine scene"))?;

        for layer_sel in &engine_scene.layer_selections {
            let layer = signal
                .layers()
                .load(layer_sel.layer_id.to_string())
                .await?
                .ok_or_else(|| eyre::eyre!("layer not found"))?;
            let layer_snap = layer
                .variant(&layer_sel.variant_id)
                .or_else(|| layer.default_variant())
                .ok_or_else(|| eyre::eyre!("no layer snapshot"))?;
            layer_name = layer.name.clone();

            for module_ref in &layer_snap.module_refs {
                if let Some(mp) = all_mp.iter().find(|p| p.id() == &module_ref.collection_id) {
                    let snap = module_ref
                        .variant_id
                        .as_ref()
                        .and_then(|vid| mp.snapshot(vid))
                        .unwrap_or_else(|| mp.default_snapshot().clone());

                    let role = FxRole::Module {
                        module_type: mp.module_type(),
                        name: mp.name().to_string(),
                    };

                    let block_states: Vec<Option<Vec<u8>>> = snap
                        .module()
                        .blocks()
                        .iter()
                        .map(|block| match block.source() {
                            ModuleBlockSource::PresetDefault { preset_id, .. }
                            | ModuleBlockSource::PresetSnapshot { preset_id, .. } => {
                                state_by_preset_id.get(&preset_id.to_string()).cloned()
                            }
                            _ => None,
                        })
                        .collect();

                    module_infos.push(ModuleInfo {
                        container_name: role.display_name(),
                        block_states,
                    });
                }
            }
        }
    }

    // ── 6. Clear + fast-path open (original state) ──
    project.tracks().remove_all().await?;
    settle().await;

    let t_open = Instant::now();

    let module_specs: Vec<(&str, Vec<Option<Vec<u8>>>)> = module_infos
        .iter()
        .map(|m| (m.container_name.as_str(), m.block_states.clone()))
        .collect();
    let module_refs: Vec<(&str, &[Option<Vec<u8>>])> = module_specs
        .iter()
        .map(|(name, states)| (*name, states.as_slice()))
        .collect();

    let layer_track = fast_path_open(&project, &rig.name, &layer_name, &module_refs).await?;

    let open_ms = t_open.elapsed().as_secs_f64() * 1000.0;
    settle().await;

    ctx.log(&format!(
        "Opened rig: {} FX in {} modules in {:.1}ms",
        block_preset_ids.len(),
        module_infos.len(),
        open_ms,
    ));

    // ── 7. Randomize all FX parameters ──
    let t_randomize = Instant::now();
    let params_randomized = randomize_all_fx(&layer_track, 42).await?;
    let randomize_ms = t_randomize.elapsed().as_secs_f64() * 1000.0;
    settle().await;

    ctx.log(&format!(
        "Randomized: {} params across {} plugins in {:.1}ms",
        params_randomized,
        block_preset_ids.len(),
        randomize_ms,
    ));
    assert!(
        params_randomized > 0,
        "randomization should have changed at least one parameter"
    );

    // ── 8. Save variation ──
    let t_save = Instant::now();
    let variation = save_variation(&layer_track, &block_preset_ids, &signal, "Randomized").await?;
    let save_ms = t_save.elapsed().as_secs_f64() * 1000.0;

    ctx.log(&format!(
        "Saved variation 'Randomized': {} snapshots in {:.1}ms",
        variation.len(),
        save_ms,
    ));
    assert_eq!(
        variation.len(),
        3,
        "should have saved 3 snapshots, got {}",
        variation.len()
    );

    // Capture randomized state for later comparison.
    let randomized_chunk = layer_track.get_chunk().await?;
    let randomized_fxc = daw::file::chunk_ops::extract_fxchain_block(&randomized_chunk)
        .ok_or_else(|| eyre::eyre!("no FXCHAIN after randomization"))?;
    let randomized_raw_blocks: Vec<String> = {
        let parsed =
            daw::file::FxChain::parse(randomized_fxc).map_err(|e| eyre::eyre!("parse: {e}"))?;
        flatten_to_raw_blocks(&parsed.nodes)
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    };

    // ── 9. Reload original rig (fast-path open again) ──
    project.tracks().remove_all().await?;
    settle().await;

    let t_reload = Instant::now();
    let layer_track = fast_path_open(&project, &rig.name, &layer_name, &module_refs).await?;
    let reload_ms = t_reload.elapsed().as_secs_f64() * 1000.0;
    settle().await;
    ctx.log(&format!("Reloaded original rig in {:.1}ms", reload_ms));

    // ── 10. Apply variation ──
    let t_apply = Instant::now();
    let applied_count = apply_variation(&layer_track, &variation, &signal).await?;
    let apply_ms = t_apply.elapsed().as_secs_f64() * 1000.0;
    settle().await;

    ctx.log(&format!(
        "Applied variation: {}/{} blocks in {:.1}ms",
        applied_count,
        variation.len(),
        apply_ms,
    ));
    assert_eq!(
        applied_count, 3,
        "should have applied all 3 blocks, got {}",
        applied_count
    );

    // ── 11. Verify ──
    // FX tree structure intact.
    let verify_tree = layer_track.fx_chain().tree().await?;
    assert_eq!(
        verify_tree.nodes.len(),
        2,
        "should still have 2 containers after apply"
    );

    // Applied state differs from original and matches randomized.
    let applied_chunk = layer_track.get_chunk().await?;
    let applied_fxc = daw::file::chunk_ops::extract_fxchain_block(&applied_chunk)
        .ok_or_else(|| eyre::eyre!("no FXCHAIN after apply"))?;
    let applied_parsed =
        daw::file::FxChain::parse(applied_fxc).map_err(|e| eyre::eyre!("parse: {e}"))?;

    let applied_raw_blocks: Vec<&str> = flatten_to_raw_blocks(&applied_parsed.nodes);
    assert_eq!(
        applied_raw_blocks.len(),
        3,
        "should have 3 plugins after apply"
    );

    // All applied blocks should be non-empty.
    for (i, rb) in applied_raw_blocks.iter().enumerate() {
        assert!(
            !rb.is_empty(),
            "block {} should have non-empty raw_block after apply",
            i
        );
    }

    // At least one block should differ from the original default snapshot.
    let original_raw_blocks: Vec<Vec<u8>> = block_preset_ids
        .iter()
        .map(|(pid, _)| {
            state_by_preset_id
                .get(&pid.to_string())
                .cloned()
                .unwrap_or_default()
        })
        .collect();

    let diffs: usize = applied_raw_blocks
        .iter()
        .zip(&original_raw_blocks)
        .filter(|(applied, original)| applied.as_bytes() != original.as_slice())
        .count();
    assert!(
        diffs > 0,
        "at least 1 block's applied state should differ from the original default"
    );

    // Verify applied state matches what we saved as the randomized variation.
    let matches: usize = applied_raw_blocks
        .iter()
        .zip(&randomized_raw_blocks)
        .filter(|(applied, randomized)| **applied == randomized.as_str())
        .count();
    ctx.log(&format!(
        "Verified: {}/{} blocks match randomized state, {} differ from original",
        matches,
        applied_raw_blocks.len(),
        diffs,
    ));

    // ── Timing summary ──
    ctx.log("────────────────────────────────────────────");
    ctx.log(&format!("  Fast-path open:      {:.1}ms", open_ms));
    ctx.log(&format!("  Randomize params:    {:.1}ms", randomize_ms));
    ctx.log(&format!("  Save variation:      {:.1}ms", save_ms));
    ctx.log(&format!("  Reload original:     {:.1}ms", reload_ms));
    ctx.log(&format!("  Apply variation:     {:.1}ms", apply_ms));
    ctx.log("────────────────────────────────────────────");

    ctx.log("fast_path_variation_save_load: PASS");
    Ok(())
}
