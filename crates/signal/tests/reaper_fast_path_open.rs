//! REAPER integration test: Fast-path rig open via chunk-based FXCHAIN injection.
//!
//! Tests the full pipeline:
//! 1. Load modules onto a track (creates containers + FX via API)
//! 2. Capture track chunk → extract raw_block state per plugin
//! 3. Import as a rig into the signal DB (with state_data)
//! 4. Clear all tracks
//! 5. Re-open via fast path: build FXCHAIN from stored state → single set_chunk
//! 6. Verify the FX tree structure matches the original
//!
//! Run with:
//!   cargo xtask reaper-test fast_path_open

use std::collections::HashMap;
use std::time::{Duration, Instant};

use reaper_test::reaper_test;
use signal::ops::rig_importer::{ImportBlock, ImportChain, ImportModule};
use signal_proto::plugin_block::{FxRole, TrackRole};
use signal_proto::{ModuleBlockSource, ModuleType};

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
fn parse_raw_block_bytes(source_bytes: &[u8]) -> Option<dawfile_reaper::types::FxChainNode> {
    let source_str = std::str::from_utf8(source_bytes).ok()?;
    let source_chain = dawfile_reaper::FxChain::parse(&format!(
        "<FXCHAIN\nSHOW 0\nLASTSEL 0\nDOCKED 0\n{source_str}\n>\n"
    ))
    .ok()?;
    source_chain.nodes.into_iter().next()
}

// ---------------------------------------------------------------------------
// Test: Import track → fast-path open → verify FX tree
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn fast_path_open_roundtrip(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;
    settle().await;

    // ── 1. Bootstrap signal controller + create a source track ──
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let svc = signal.service();

    let source_track = project.tracks().add("Source Track", None).await?;
    settle().await;

    // Load two EQ modules onto the source track (creates [M] containers with
    // Pro-Q 4 blocks inside). The seed data has "eq-proq4-3band" (3 blocks)
    // and "eq-proq4-4band" (4 blocks).
    let t_api_load = Instant::now();
    let mod1 = svc
        .load_module_to_track(
            ModuleType::Eq,
            &signal::ModulePresetId::from_uuid(signal::seed_id("eq-proq4-3band")),
            0,
            &source_track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    settle().await;

    let mod2 = svc
        .load_module_to_track(
            ModuleType::Eq,
            &signal::ModulePresetId::from_uuid(signal::seed_id("eq-proq4-4band")),
            0,
            &source_track,
        )
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    settle().await;
    let api_load_ms = t_api_load.elapsed().as_secs_f64() * 1000.0;

    ctx.log(&format!(
        "API load: {} + {} FX onto source track in {:.1}ms",
        mod1.loaded_fx.len(),
        mod2.loaded_fx.len(),
        api_load_ms,
    ));

    // ── 2. Capture FX tree + raw_block state from the track chunk ──
    let t_capture = Instant::now();
    let tree = source_track.fx_chain().tree().await?;

    // Parse track chunk to get raw_block bytes per plugin.
    let chunk_str = source_track.get_chunk().await?;
    let fxchain_text = dawfile_reaper::chunk_ops::extract_fxchain_block(&chunk_str)
        .ok_or_else(|| eyre::eyre!("No FXCHAIN in source track chunk"))?;
    let parsed = dawfile_reaper::FxChain::parse(fxchain_text)
        .map_err(|e| eyre::eyre!("Failed to parse FXCHAIN: {e}"))?;

    // Collect raw_block bytes keyed by FXID (GUID).
    let mut state_by_guid: HashMap<String, Vec<u8>> = HashMap::new();
    fn collect_state(
        nodes: &[dawfile_reaper::types::FxChainNode],
        out: &mut HashMap<String, Vec<u8>>,
    ) {
        for node in nodes {
            match node {
                dawfile_reaper::types::FxChainNode::Plugin(p) => {
                    if !p.raw_block.is_empty() {
                        if let Some(fxid) = &p.fxid {
                            let guid = fxid
                                .strip_prefix('{')
                                .and_then(|s| s.strip_suffix('}'))
                                .unwrap_or(fxid);
                            out.insert(guid.to_string(), p.raw_block.as_bytes().to_vec());
                        } else if let Some(cn) = &p.custom_name {
                            out.insert(cn.clone(), p.raw_block.as_bytes().to_vec());
                        }
                    }
                }
                dawfile_reaper::types::FxChainNode::Container(c) => {
                    collect_state(&c.children, out);
                }
            }
        }
    }
    collect_state(&parsed.nodes, &mut state_by_guid);
    let capture_ms = t_capture.elapsed().as_secs_f64() * 1000.0;
    ctx.log(&format!(
        "State capture: {} plugins in {:.1}ms",
        state_by_guid.len(),
        capture_ms,
    ));

    // ── 3. Infer chain via signal-daw-bridge + build ImportChain with state ──
    let inferred = signal_daw_bridge::infer_chain_from_fx_tree(&tree);
    assert_eq!(inferred.modules.len(), 2, "should infer 2 modules");

    let import_chain = ImportChain {
        modules: inferred
            .modules
            .iter()
            .map(|m| {
                let blocks_vec = m.chain.blocks();
                ImportModule {
                    name: m.name.clone(),
                    module_type: m.module_type,
                    has_parallel_routing: !m.chain.is_serial(),
                    blocks: blocks_vec
                        .iter()
                        .enumerate()
                        .map(|(i, b)| {
                            let sd = state_by_guid
                                .get(b.id())
                                .or_else(|| state_by_guid.get(b.label()))
                                .cloned();
                            ImportBlock {
                                label: b.label().to_string(),
                                block_type: b.block_type(),
                                plugin_name: m
                                    .block_plugin_names
                                    .get(i)
                                    .filter(|s| !s.is_empty())
                                    .cloned(),
                                state_data: sd,
                                parameters: Vec::new(),
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
        standalone_blocks: vec![],
    };

    // Count blocks with state — all should have state for fast path.
    let total_blocks: usize = import_chain.modules.iter().map(|m| m.blocks.len()).sum();
    let blocks_with_state: usize = import_chain
        .modules
        .iter()
        .flat_map(|m| &m.blocks)
        .filter(|b| b.state_data.is_some())
        .count();
    ctx.log(&format!(
        "Import: {}/{} blocks have state data",
        blocks_with_state, total_blocks
    ));
    assert_eq!(
        blocks_with_state, total_blocks,
        "all blocks must have state_data for fast path ({}/{})",
        blocks_with_state, total_blocks,
    );

    // ── 4. Import as a rig ──
    let t_import = Instant::now();
    let result = signal
        .import_rig_from_chain(&import_chain, "Fast Path Test Rig")
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    let import_ms = t_import.elapsed().as_secs_f64() * 1000.0;
    ctx.log(&format!(
        "Rig import: {} ({}) in {:.1}ms — {} new, {} reused block presets",
        result.rig.name, result.rig_id, import_ms,
        result.new_block_preset_count, result.reused_block_preset_count,
    ));

    // ── 5. Clear all tracks and re-open via fast path ──
    project.tracks().remove_all().await?;
    settle().await;

    let t_open = Instant::now();

    // Reload the rig from DB to simulate a fresh open.
    let rig = signal
        .rigs()
        .load(result.rig_id.to_string())
        .await?
        .ok_or_else(|| eyre::eyre!("rig not found after import"))?;

    // Collect all block preset state data.
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

    // Resolve rig hierarchy → specs.
    let all_mp = signal.module_presets().list().await.unwrap_or_default();
    let default_scene = rig
        .default_variant()
        .ok_or_else(|| eyre::eyre!("rig has no default scene"))?;

    struct BlockSpec {
        state_data: Option<Vec<u8>>,
    }
    struct ModuleSpec {
        container_name: String,
        blocks: Vec<BlockSpec>,
    }
    struct LayerSpec {
        name: String,
        modules: Vec<ModuleSpec>,
    }

    let mut layer_specs: Vec<LayerSpec> = Vec::new();
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

            let mut module_specs = Vec::new();
            for module_ref in &layer_snap.module_refs {
                if let Some(mp) = all_mp.iter().find(|p| p.id() == &module_ref.collection_id) {
                    let snap = module_ref
                        .variant_id
                        .as_ref()
                        .and_then(|vid| mp.snapshot(vid))
                        .unwrap_or_else(|| mp.default_snapshot().clone());

                    let blocks: Vec<BlockSpec> = snap
                        .module()
                        .blocks()
                        .iter()
                        .map(|block| {
                            let preset_data = match block.source() {
                                ModuleBlockSource::PresetDefault { preset_id, .. }
                                | ModuleBlockSource::PresetSnapshot { preset_id, .. } => {
                                    state_by_preset_id.get(&preset_id.to_string()).cloned()
                                }
                                _ => None,
                            };
                            BlockSpec {
                                state_data: preset_data,
                            }
                        })
                        .collect();

                    let role = FxRole::Module {
                        module_type: mp.module_type(),
                        name: mp.name().to_string(),
                    };
                    module_specs.push(ModuleSpec {
                        container_name: role.display_name(),
                        blocks,
                    });
                }
            }
            layer_specs.push(LayerSpec {
                name: layer.name.clone(),
                modules: module_specs,
            });
        }
    }

    // Verify fast path is viable.
    let resolved_total: usize = layer_specs
        .iter()
        .flat_map(|l| &l.modules)
        .map(|m| m.blocks.len())
        .sum();
    let resolved_with_state: usize = layer_specs
        .iter()
        .flat_map(|l| &l.modules)
        .flat_map(|m| &m.blocks)
        .filter(|b| b.state_data.is_some())
        .count();
    assert_eq!(
        resolved_with_state, resolved_total,
        "fast path requires all blocks have state ({}/{})",
        resolved_with_state, resolved_total,
    );

    // ── 6. Fast path: build track hierarchy + FXCHAIN from stored state ──
    let rig_track = project
        .tracks()
        .add(
            &TrackRole::Rig {
                name: rig.name.clone(),
            }
            .display_name(),
            None,
        )
        .await?;
    rig_track.set_folder_depth(1).await?;

    let engine_track = project
        .tracks()
        .add(
            &TrackRole::Engine {
                name: "Test Engine".to_string(),
            }
            .display_name(),
            None,
        )
        .await?;
    engine_track.set_folder_depth(1).await?;

    // One layer per spec (just 1 in this test).
    let mut layer_track_guids = Vec::new();
    for (li, layer) in layer_specs.iter().enumerate() {
        let layer_track = project
            .tracks()
            .add(
                &TrackRole::Layer {
                    name: layer.name.clone(),
                }
                .display_name(),
                None,
            )
            .await?;

        // Close folders: last layer closes both engine + rig.
        let is_last = li == layer_specs.len() - 1;
        if is_last {
            layer_track.set_folder_depth(-2).await?;
        }

        // Build FXCHAIN from stored raw_blocks.
        let mut fxchain_nodes = Vec::new();
        for module in &layer.modules {
            let mut children = Vec::new();
            for block in &module.blocks {
                if let Some(ref data) = block.state_data {
                    if let Some(node) = parse_raw_block_bytes(data) {
                        children.push(node);
                    }
                }
            }
            fxchain_nodes.push(dawfile_reaper::types::FxChainNode::Container(
                dawfile_reaper::types::FxContainer {
                    name: module.container_name.clone(),
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

        let fxchain = dawfile_reaper::FxChain {
            window_rect: None,
            show: 0,
            last_sel: 0,
            docked: false,
            nodes: fxchain_nodes,
            raw_content: String::new(),
        };

        // Inject FXCHAIN into track chunk.
        let chunk = layer_track.get_chunk().await?;
        let fxchain_text = fxchain.to_rpp_string();
        let new_chunk =
            if let Some(existing) = dawfile_reaper::chunk_ops::extract_fxchain_block(&chunk) {
                chunk.replace(existing, &fxchain_text)
            } else {
                let pos = chunk
                    .rfind('>')
                    .ok_or_else(|| eyre::eyre!("invalid track chunk"))?;
                format!("{}{}\n{}", &chunk[..pos], fxchain_text, &chunk[pos..])
            };
        layer_track.set_chunk(new_chunk).await?;

        layer_track_guids.push(layer_track.guid().to_string());
    }

    let open_ms = t_open.elapsed().as_secs_f64() * 1000.0;
    let total_fx: usize = layer_specs
        .iter()
        .flat_map(|l| &l.modules)
        .map(|m| m.blocks.len())
        .sum();
    ctx.log(&format!(
        "Fast-path open: {} FX in {} modules loaded in {:.1}ms",
        total_fx,
        layer_specs.iter().flat_map(|l| &l.modules).count(),
        open_ms,
    ));

    settle().await;

    // ── 7. Verify FX tree matches expected structure ──
    let tracks = project.tracks().all().await?;
    assert!(
        tracks.len() >= 3,
        "should have at least 3 tracks (rig+engine+layer), got {}",
        tracks.len()
    );
    assert!(
        tracks[0].name.starts_with("[R]"),
        "track 0 should be [R], got '{}'",
        tracks[0].name
    );
    assert!(
        tracks[1].name.starts_with("[E]"),
        "track 1 should be [E], got '{}'",
        tracks[1].name
    );
    assert!(
        tracks[2].name.starts_with("[L]"),
        "track 2 should be [L], got '{}'",
        tracks[2].name
    );

    // Verify FX tree on the layer track.
    for guid in &layer_track_guids {
        let track = project
            .tracks()
            .by_guid(guid)
            .await?
            .ok_or_else(|| eyre::eyre!("layer track not found"))?;

        let verify_tree = track.fx_chain().tree().await?;

        // Should have 2 top-level containers (one per module).
        assert_eq!(
            verify_tree.nodes.len(),
            2,
            "layer should have 2 top-level containers, got {}",
            verify_tree.nodes.len()
        );

        // Container 0: 3-Band module with 3 children.
        match &verify_tree.nodes[0].kind {
            daw_control::FxNodeKind::Container {
                name, children, ..
            } => {
                assert!(
                    name.contains("[M]"),
                    "container 0 should have [M] prefix, got '{name}'"
                );
                assert_eq!(
                    children.len(),
                    3,
                    "3-Band module should have 3 children, got {}",
                    children.len()
                );
            }
            _ => panic!("node 0 should be a container"),
        }

        // Container 1: 4-Band module with 4 children.
        match &verify_tree.nodes[1].kind {
            daw_control::FxNodeKind::Container {
                name, children, ..
            } => {
                assert!(
                    name.contains("[M]"),
                    "container 1 should have [M] prefix, got '{name}'"
                );
                assert_eq!(
                    children.len(),
                    4,
                    "4-Band module should have 4 children, got {}",
                    children.len()
                );
            }
            _ => panic!("node 1 should be a container"),
        }

        // Verify all plugins have non-empty raw_block (state was loaded).
        let chunk_str = track.get_chunk().await?;
        let fxc = dawfile_reaper::chunk_ops::extract_fxchain_block(&chunk_str)
            .ok_or_else(|| eyre::eyre!("no FXCHAIN in loaded track"))?;
        let loaded = dawfile_reaper::FxChain::parse(fxc)
            .map_err(|e| eyre::eyre!("Failed to parse FXCHAIN: {e}"))?;

        let mut verified_fx = 0usize;
        fn count_plugins_with_state(
            nodes: &[dawfile_reaper::types::FxChainNode],
            count: &mut usize,
        ) {
            for node in nodes {
                match node {
                    dawfile_reaper::types::FxChainNode::Plugin(p) => {
                        assert!(
                            !p.raw_block.is_empty(),
                            "plugin '{}' should have non-empty raw_block",
                            p.custom_name.as_deref().unwrap_or(&p.name)
                        );
                        *count += 1;
                    }
                    dawfile_reaper::types::FxChainNode::Container(c) => {
                        count_plugins_with_state(&c.children, count);
                    }
                }
            }
        }
        count_plugins_with_state(&loaded.nodes, &mut verified_fx);
        assert_eq!(
            verified_fx, 7,
            "should have 7 FX with state (3+4), got {}",
            verified_fx
        );
    }

    // ── Timing summary ──
    ctx.log("────────────────────────────────────────────");
    ctx.log(&format!("  API load (baseline):  {:.1}ms", api_load_ms));
    ctx.log(&format!("  State capture:        {:.1}ms", capture_ms));
    ctx.log(&format!("  Rig import:           {:.1}ms", import_ms));
    ctx.log(&format!("  Fast-path open:       {:.1}ms", open_ms));
    if api_load_ms > 0.0 {
        ctx.log(&format!(
            "  Speedup:              {:.1}x faster than API load",
            api_load_ms / open_ms
        ));
    }
    ctx.log("────────────────────────────────────────────");

    ctx.log("fast_path_open_roundtrip: PASS");
    Ok(())
}
