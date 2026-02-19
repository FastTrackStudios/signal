//! REAPER integration tests — loads track templates, discovers track/FX
//! hierarchies, builds domain objects, and tests snapshot round-trips.
//!
//! Run with:
//!
//!   cargo xtask reaper-test

mod daw_helpers;

use daw_helpers::{
    apply_snapshot_to_fx_at, capture_snapshot_at, child_tracks, find_leaf_plugin_with_params,
    get_fx_at, randomize_fx_params, read_fx_list,
};
use reaper_test::reaper_test;
use signal::{
    block::BlockType,
    engine::Engine,
    fx_send::FxSend,
    fx_send::FxSendBus,
    fx_send::FxSendCategory,
    layer::Layer,
    module_type::ModuleType,
    plugin_block::{FxRole, PluginBlockDef, VirtualBlock, VirtualModule},
    rig::Rig,
    seed_id, DawParameterSnapshot,
};

// ─────────────────────────────────────────────────────────────
//  Scenario 1: Discover Guitar Rig structure
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn discover_guitar_rig(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: discover_guitar_rig ===");
    ctx.load_template("testing-stockjs-guitar-rig").await?;

    let track = ctx.track_by_name("GUITAR Rig").await?;
    let fx_list = read_fx_list(&track).await?;
    println!("  FX on GUITAR Rig ({} total):", fx_list.len());
    for (idx, name) in &fx_list {
        println!("    [{}] {}", idx, name);
    }

    assert!(
        fx_list.len() >= 8,
        "expected at least 8 top-level FX items on Guitar Rig, got {}",
        fx_list.len()
    );

    let names_joined: String = fx_list
        .iter()
        .map(|(_, n)| n.as_str())
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        names_joined.contains("INPUT")
            || names_joined.contains("input")
            || names_joined.contains("Container"),
        "expected to find INPUT module or Container in FX names"
    );

    println!(
        "  FX chain has {} items suitable for domain mapping",
        fx_list.len()
    );

    let default_scene = signal::rig::RigScene::new(seed_id("guitar-rig-default-scene"), "Default");
    let rig = Rig::new(
        seed_id("guitar-rig"),
        "GUITAR Rig",
        vec![seed_id("guitar-engine").into()],
        default_scene,
    )
    .with_input_track("RIG: Guitar Input".to_string());

    assert_eq!(rig.name, "GUITAR Rig");
    assert!(rig.input_track_ref.is_some());
    println!("  Built Rig domain object: {}", rig.name);

    let _input = ctx.track_by_name("RIG: Guitar Input").await?;
    println!("  Verified input track 'RIG: Guitar Input' exists");

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 2: Guitar Rig snapshot round-trip
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn guitar_rig_snapshot_round_trip(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: guitar_rig_snapshot_round_trip ===");
    ctx.load_template("testing-stockjs-guitar-rig").await?;

    let track = ctx.track_by_name("GUITAR Rig").await?;
    let fx_list = read_fx_list(&track).await?;

    let mut test_fx_idx = None;
    for (idx, _name) in &fx_list {
        let fx = get_fx_at(&track, *idx).await?;
        let params = fx.parameters().await?;
        if params.len() > 2 {
            test_fx_idx = Some(*idx);
            println!(
                "  Using FX[{}] with {} params for round-trip",
                idx,
                params.len()
            );
            break;
        }
    }
    let fx_idx = test_fx_idx.ok_or_else(|| eyre::eyre!("no FX with >2 params found"))?;

    let original = capture_snapshot_at(&track, fx_idx, "guitar-fx").await?;
    println!(
        "  Captured {} params from FX[{}]",
        original.params.len(),
        fx_idx
    );

    let (old_params, new_params) = randomize_fx_params(&track, fx_idx).await?;
    let changed_count = old_params
        .iter()
        .zip(new_params.iter())
        .filter(|(o, n)| (o.value - n.value).abs() > 0.001)
        .count();
    println!(
        "  Randomized {} of {} params",
        changed_count,
        old_params.len()
    );
    assert!(
        changed_count > 0,
        "randomization should change at least one param"
    );

    apply_snapshot_to_fx_at(&track, fx_idx, &original).await?;

    let restored = capture_snapshot_at(&track, fx_idx, "guitar-fx").await?;
    let mut mismatches = 0;
    for (orig, rest) in original.params.iter().zip(restored.params.iter()) {
        let diff = (orig.value - rest.value).abs();
        if diff > 0.02 {
            println!(
                "    MISMATCH [{}] {}: {:.4} vs {:.4}",
                orig.param_index, orig.param_name, orig.value, rest.value
            );
            mismatches += 1;
        }
    }
    assert_eq!(
        mismatches, 0,
        "{mismatches} param(s) didn't match after snapshot restore"
    );
    println!("  All params restored correctly");

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 3: Multi-FX Amp Block (container with sub-containers)
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn multi_fx_amp_block(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: multi_fx_amp_block ===");
    ctx.load_template("testing-stockjs-guitar-rig").await?;

    let track = ctx.track_by_name("GUITAR Rig").await?;
    let fx_list = read_fx_list(&track).await?;

    println!("  Full FX list for multi-FX analysis:");
    for (idx, name) in &fx_list {
        println!("    [{}] {}", idx, name);
    }

    let mut fx_with_params: Vec<(u32, String, usize)> = Vec::new();
    for (idx, name) in &fx_list {
        let fx = get_fx_at(&track, *idx).await?;
        let params = fx.parameters().await?;
        if !params.is_empty() {
            fx_with_params.push((*idx, name.clone(), params.len()));
        }
    }
    println!(
        "  {} FX have params (out of {} total)",
        fx_with_params.len(),
        fx_list.len()
    );

    let mut combined_params = Vec::new();
    let mut fx_count = 0;
    for (idx, name, _param_count) in &fx_with_params {
        if fx_count >= 4 {
            break;
        }
        let snap = capture_snapshot_at(&track, *idx, &format!("amp-fx-{idx}")).await?;
        println!(
            "  Captured {} params from FX[{}] '{}'",
            snap.params.len(),
            idx,
            name
        );
        combined_params.extend(snap.params.iter().cloned());
        fx_count += 1;
    }

    let combined_snapshot = DawParameterSnapshot::new(combined_params);
    println!(
        "  Combined multi-FX snapshot: {} total params across {} FX",
        combined_snapshot.params.len(),
        fx_count
    );
    assert!(
        combined_snapshot.params.len() > 5,
        "multi-FX snapshot should have substantial params"
    );

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 4: Keys Engine layers
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn keys_engine_layers(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: keys_engine_layers ===");
    ctx.load_template("testing-stockjs-keys-rig").await?;

    let engine_track = ctx.track_by_name("KEYS ENGINE").await?;
    let children = child_tracks(ctx.project(), &engine_track).await?;
    println!("  KEYS ENGINE has {} child tracks:", children.len());
    for child in &children {
        println!("    [{}] {}", child.index, child.name);
    }

    let layer_names: Vec<&str> = children.iter().map(|t| t.name.as_str()).collect();
    assert!(
        layer_names.iter().any(|n| n.contains("Layer 1")),
        "expected Layer 1 child track"
    );
    assert!(
        layer_names.iter().any(|n| n.contains("Layer 2")),
        "expected Layer 2 child track"
    );

    let layer1_snap = signal::layer::LayerSnapshot::new(seed_id("keys-layer-1-default"), "Default");
    let layer1 = Layer::new(
        seed_id("keys-layer-1"),
        "Layer 1: ReaKeys - Sine",
        signal::EngineType::Keys,
        layer1_snap,
    );
    let layer2_snap = signal::layer::LayerSnapshot::new(seed_id("keys-layer-2-default"), "Default");
    let layer2 = Layer::new(
        seed_id("keys-layer-2"),
        "Layer 2: Stock Keys - Low Sine",
        signal::EngineType::Keys,
        layer2_snap,
    );

    let engine_scene =
        signal::engine::EngineScene::new(seed_id("keys-engine-default-scene"), "Default");
    let engine = Engine::new(
        seed_id("keys-engine"),
        "KEYS ENGINE",
        signal::EngineType::Keys,
        vec![layer1.id.clone(), layer2.id.clone()],
        engine_scene,
    )
    .with_input_track("INPUT - KEYS ENGINE".to_string());

    println!(
        "  Built Engine: {} with {} layers",
        engine.name,
        engine.layer_ids.len()
    );
    assert_eq!(engine.layer_ids.len(), 2);
    assert!(engine.input_track_ref.is_some());

    let sends_track = ctx.track_by_name("FX Sends: KEYS ENGINE").await?;
    let send_children = child_tracks(ctx.project(), &sends_track).await?;
    println!(
        "  FX Sends: KEYS ENGINE has {} send tracks:",
        send_children.len()
    );
    for child in &send_children {
        println!("    {}", child.name);
    }

    let mut fx_sends = Vec::new();
    for child in &send_children {
        let category = FxSendCategory::infer_from_name(&child.name);
        let send = FxSend::new(
            seed_id(&format!("keys-send-{}", child.index)),
            child.name.clone(),
            category,
            BlockType::Send,
        )
        .with_track_ref(child.guid.clone());
        fx_sends.push(send);
    }
    println!("  Built {} FxSend domain objects", fx_sends.len());
    assert!(
        fx_sends.len() >= 2,
        "expected at least reverb + delay sends"
    );

    for child in &children {
        if child.name.contains("Layer") {
            let layer_track = ctx.track_by_name(&child.name).await?;
            let fx_list = read_fx_list(&layer_track).await?;
            println!("  {} FX: {:?}", child.name, fx_list);
        }
    }

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 5: Vocal Rack structure
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn vocal_rack_structure(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: vocal_rack_structure ===");
    ctx.load_template("testing-stockjs-vocal-rack").await?;

    let rack_track = ctx.track_by_name("Vocal Rack").await?;
    let rack_children = child_tracks(ctx.project(), &rack_track).await?;
    println!("  Vocal Rack has {} child tracks:", rack_children.len());
    for child in &rack_children {
        println!("    [{}] {}", child.index, child.name);
    }

    let rig_names: Vec<&str> = rack_children
        .iter()
        .filter(|t| t.name.contains("Rig") || t.name.contains("Vocal"))
        .map(|t| t.name.as_str())
        .collect();
    println!("  Rig-related tracks: {:?}", rig_names);

    let rack_sends_track = ctx.track_by_name("FX Sends: Vocal Rack").await?;
    let rack_send_children = child_tracks(ctx.project(), &rack_sends_track).await?;
    println!("  Rack-level FX Sends ({}):", rack_send_children.len());
    for child in &rack_send_children {
        println!("    {}", child.name);
    }

    let mut aux_sends = Vec::new();
    let mut time_sends = Vec::new();

    for child in &rack_send_children {
        if child.name == "AUX" || child.name == "TIME" {
            let sub_track = ctx.track_by_name(&child.name).await?;
            let sub_children = child_tracks(ctx.project(), &sub_track).await?;
            println!("    {} sub-sends ({}):", child.name, sub_children.len());
            for sc in &sub_children {
                println!("      {}", sc.name);
                let category = FxSendCategory::infer_from_name(&sc.name);
                let send = FxSend::new(
                    seed_id(&format!(
                        "vocal-rack-{}-{}",
                        child.name.to_lowercase(),
                        sc.index
                    )),
                    sc.name.clone(),
                    category,
                    BlockType::Send,
                )
                .with_track_ref(sc.guid.clone());
                if child.name == "AUX" {
                    aux_sends.push(send);
                } else {
                    time_sends.push(send);
                }
            }
        }
    }

    let mut aux_bus = FxSendBus::new(seed_id("vocal-rack-aux-bus"), "AUX").with_sub_category("AUX");
    for send in aux_sends {
        aux_bus = aux_bus.with_send(send);
    }

    let mut time_bus =
        FxSendBus::new(seed_id("vocal-rack-time-bus"), "TIME").with_sub_category("TIME");
    for send in time_sends {
        time_bus = time_bus.with_send(send);
    }

    println!(
        "  AUX bus: {} sends, TIME bus: {} sends",
        aux_bus.sends.len(),
        time_bus.sends.len()
    );

    let rack = signal::rack::Rack::new(seed_id("vocal-rack"), "Vocal Rack".to_string())
        .with_fx_send_bus(aux_bus)
        .with_fx_send_bus(time_bus);

    println!(
        "  Built Rack: {} with {} FX send buses",
        rack.name,
        rack.fx_send_buses.len()
    );
    assert_eq!(rack.fx_send_buses.len(), 2);

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 6: Block parameter overrides
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn block_parameter_overrides(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: block_parameter_overrides ===");
    ctx.load_template("testing-stockjs-guitar-rig").await?;

    let track = ctx.track_by_name("GUITAR Rig").await?;
    let (fx_idx, fx_name) = find_leaf_plugin_with_params(&track, 5).await?;
    println!("  Using FX[{}] '{}' for override test", fx_idx, fx_name);

    let baseline = capture_snapshot_at(&track, fx_idx, "override-test").await?;
    println!("  Baseline: {} params", baseline.params.len());

    let fx = get_fx_at(&track, fx_idx).await?;
    let override_indices: Vec<u32> = baseline
        .params
        .iter()
        .take(3)
        .map(|p| p.param_index)
        .collect();

    for &idx in &override_indices {
        fx.param(idx).set(0.8).await?;
    }
    println!("  Applied override (0.8) to params: {:?}", override_indices);

    let overridden = capture_snapshot_at(&track, fx_idx, "override-test").await?;
    for &idx in &override_indices {
        let val = overridden
            .params
            .iter()
            .find(|p| p.param_index == idx)
            .unwrap();
        assert!(
            (val.value - 0.8).abs() < 0.05,
            "param {} should be ~0.8 after override, got {:.4}",
            idx,
            val.value
        );
    }
    println!("  Verified overrides applied correctly");

    apply_snapshot_to_fx_at(&track, fx_idx, &baseline).await?;

    let restored = capture_snapshot_at(&track, fx_idx, "override-test").await?;
    for &idx in &override_indices {
        let orig = baseline
            .params
            .iter()
            .find(|p| p.param_index == idx)
            .unwrap();
        let rest = restored
            .params
            .iter()
            .find(|p| p.param_index == idx)
            .unwrap();
        let diff = (orig.value - rest.value).abs();
        assert!(
            diff < 0.02,
            "param {} should revert to {:.4}, got {:.4}",
            idx,
            orig.value,
            rest.value
        );
    }
    println!("  Verified baseline restored after removing overrides");

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 7: Randomize and save snapshot
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn randomize_and_save_snapshot(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: randomize_and_save_snapshot ===");
    ctx.load_template("testing-stockjs-guitar-rig").await?;

    let track = ctx.track_by_name("GUITAR Rig").await?;
    let (fx_idx, fx_name) = find_leaf_plugin_with_params(&track, 5).await?;
    println!("  Using FX[{}] '{}' for randomize test", fx_idx, fx_name);

    let default_snap = capture_snapshot_at(&track, fx_idx, "rand-test").await?;

    let (_, randomized_live) = randomize_fx_params(&track, fx_idx).await?;
    println!("  Randomized {} params", randomized_live.len());

    let randomized_snap = capture_snapshot_at(&track, fx_idx, "rand-test").await?;

    let diff_count = default_snap
        .params
        .iter()
        .zip(randomized_snap.params.iter())
        .filter(|(d, r)| (d.value - r.value).abs() > 0.01)
        .count();
    println!(
        "  {} params differ between default and randomized",
        diff_count
    );
    assert!(
        diff_count > 0,
        "randomization should change at least one param"
    );

    apply_snapshot_to_fx_at(&track, fx_idx, &default_snap).await?;
    let post_restore = capture_snapshot_at(&track, fx_idx, "rand-test").await?;

    let restore_mismatches: usize = default_snap
        .params
        .iter()
        .zip(post_restore.params.iter())
        .filter(|(d, r)| (d.value - r.value).abs() > 0.02)
        .count();
    assert_eq!(restore_mismatches, 0, "default should be fully restored");
    println!("  Default restored OK");

    apply_snapshot_to_fx_at(&track, fx_idx, &randomized_snap).await?;
    let re_randomized = capture_snapshot_at(&track, fx_idx, "rand-test").await?;

    let reapply_mismatches: usize = randomized_snap
        .params
        .iter()
        .zip(re_randomized.params.iter())
        .filter(|(r, rr)| (r.value - rr.value).abs() > 0.02)
        .count();
    assert_eq!(
        reapply_mismatches, 0,
        "re-applied randomized snapshot should match saved values"
    );
    println!("  Re-applied randomized snapshot matches saved values");

    apply_snapshot_to_fx_at(&track, fx_idx, &default_snap).await?;

    println!("PASS");
    Ok(())
}

// ─────────────────────────────────────────────────────────────
//  Scenario 8: Guitar Rig hierarchy mapping from live FX chain
// ─────────────────────────────────────────────────────────────

#[reaper_test]
async fn guitar_rig_hierarchy_mapping(ctx: &ReaperTestContext) -> Result<()> {
    println!("\n=== scenario: guitar_rig_hierarchy_mapping ===");
    ctx.load_template("testing-stockjs-guitar-rig").await?;

    let track = ctx.track_by_name("GUITAR Rig").await?;

    use daw_proto::fx::tree::{FxNode as TreeNode, FxNodeKind};

    let tree = track.fx_chain().tree().await?;
    println!(
        "  FxTree: {} top-level nodes, {} total nodes",
        tree.nodes.len(),
        tree.total_count()
    );

    for (depth, node) in tree.iter_depth_first() {
        let indent = "  ".repeat(depth + 2);
        let kind_label = if node.is_container() {
            "CONTAINER"
        } else {
            "PLUGIN"
        };
        let detail = match &node.kind {
            FxNodeKind::Plugin(fx) => format!("{} params", fx.parameter_count),
            FxNodeKind::Container { children, .. } => format!("{} children", children.len()),
        };
        println!(
            "{}[{}] {} ({}: {})",
            indent,
            node.id,
            node.display_name(),
            kind_label,
            detail
        );
    }

    // ── Parse the tree into modules + blocks ──

    struct ParsedModule {
        module_type: ModuleType,
        name: String,
        blocks: Vec<ParsedBlock>,
    }
    struct ParsedBlock {
        block_type: BlockType,
        name: String,
        fx_node_ids: Vec<String>,
    }

    fn collect_leaf_plugins(node: &TreeNode, out: &mut Vec<String>) {
        match &node.kind {
            FxNodeKind::Plugin(_) => {
                out.push(node.id.as_str().to_string());
            }
            FxNodeKind::Container { children, .. } => {
                for child in children {
                    collect_leaf_plugins(child, out);
                }
            }
        }
    }

    fn process_child_as_block(child: &TreeNode, blocks: &mut Vec<ParsedBlock>, module_name: &str) {
        let child_role = FxRole::parse(child.display_name());
        match (&child_role, child.is_container()) {
            (FxRole::Block { block_type, name }, true) => {
                let mut fx_ids = Vec::new();
                collect_leaf_plugins(child, &mut fx_ids);
                blocks.push(ParsedBlock {
                    block_type: *block_type,
                    name: name.clone(),
                    fx_node_ids: fx_ids,
                });
            }
            (FxRole::Block { block_type, name }, false) => {
                blocks.push(ParsedBlock {
                    block_type: *block_type,
                    name: name.clone(),
                    fx_node_ids: vec![child.id.as_str().to_string()],
                });
            }
            (FxRole::GenericModule { .. }, true) => {
                for grandchild in child.children() {
                    process_child_as_block(grandchild, blocks, module_name);
                }
            }
            (_, false) => {
                println!(
                    "    INFO: non-block plugin '{}' in module '{}'",
                    child.display_name(),
                    module_name
                );
                blocks.push(ParsedBlock {
                    block_type: BlockType::Custom,
                    name: child.display_name().to_string(),
                    fx_node_ids: vec![child.id.as_str().to_string()],
                });
            }
            (_, true) => {
                println!(
                    "    INFO: unrecognized container '{}' in module '{}'",
                    child.display_name(),
                    module_name
                );
            }
        }
    }

    let mut modules: Vec<ParsedModule> = Vec::new();

    for top_node in &tree.nodes {
        let role = FxRole::parse(top_node.display_name());
        let (module_type, module_name) = match role {
            FxRole::Module { module_type, name } => (module_type, name),
            _ => {
                println!(
                    "  WARN: top-level node '{}' not a Module, skipping",
                    top_node.display_name()
                );
                continue;
            }
        };

        let mut blocks: Vec<ParsedBlock> = Vec::new();
        for child in top_node.children() {
            process_child_as_block(child, &mut blocks, &module_name);
        }

        modules.push(ParsedModule {
            module_type,
            name: module_name,
            blocks,
        });
    }

    println!("\n  Parsed hierarchy ({} modules):", modules.len());
    for m in &modules {
        println!(
            "    {:?} Module: \"{}\" ({} blocks)",
            m.module_type,
            m.name,
            m.blocks.len()
        );
        for b in &m.blocks {
            let multi = if b.fx_node_ids.len() > 1 {
                " [MULTI-FX]"
            } else {
                ""
            };
            println!(
                "      {:?} Block: \"{}\" ({} FX){}",
                b.block_type,
                b.name,
                b.fx_node_ids.len(),
                multi
            );
        }
    }

    assert!(
        modules.len() >= 6,
        "expected at least 6 modules from the Guitar Rig, got {}",
        modules.len()
    );

    let module_types: Vec<ModuleType> = modules.iter().map(|m| m.module_type).collect();
    println!("\n  Module types: {:?}", module_types);

    assert_eq!(
        module_types[0],
        ModuleType::Source,
        "first module should be INPUT (Source)"
    );
    assert!(
        module_types.contains(&ModuleType::Amp),
        "should contain an AMP module"
    );
    assert_eq!(
        *module_types.last().unwrap(),
        ModuleType::Master,
        "last module should be MASTER"
    );

    let amp_module = modules
        .iter()
        .find(|m| m.module_type == ModuleType::Amp)
        .expect("AMP module");
    println!("\n  AMP module has {} blocks:", amp_module.blocks.len());
    for b in &amp_module.blocks {
        println!(
            "    {:?}: \"{}\" -> {} FX node(s)",
            b.block_type,
            b.name,
            b.fx_node_ids.len()
        );
    }

    let multi_fx_count = amp_module
        .blocks
        .iter()
        .filter(|b| b.fx_node_ids.len() > 1)
        .count();
    assert!(
        multi_fx_count > 0,
        "AMP module should have at least one multi-FX block"
    );
    println!("  Found {} multi-FX block(s) in AMP module", multi_fx_count);

    let total_nodes = tree.total_count() as u32;
    let mut def = PluginBlockDef::new("Guitar Rig (Stock - Live)", total_nodes);
    for m in &modules {
        let id = format!("{}-{}", m.module_type.as_str(), slug(&m.name));
        let mut vm = VirtualModule::new(&id, &m.name, m.module_type);
        for b in &m.blocks {
            let block_id = format!("{}-{}", b.block_type.as_str(), slug(&b.name));
            let mut vb = VirtualBlock::new(&block_id, &b.name, b.block_type);
            if b.fx_node_ids.len() > 1 {
                let linked: Vec<u32> = (0..b.fx_node_ids.len() as u32).collect();
                vb = vb.with_linked_fx(linked);
            }
            vm = vm.with_block(vb);
        }
        def = def.with_module(vm);
    }

    println!("\n  Built PluginBlockDef:");
    println!("    plugin_name: {}", def.plugin_name);
    println!("    modules: {}", def.modules.len());
    println!("    total blocks: {}", def.all_blocks().len());

    assert_eq!(def.modules.len(), modules.len());
    assert!(
        def.all_blocks().len() >= 10,
        "expected at least 10 blocks total, got {}",
        def.all_blocks().len()
    );

    let amp_def = def
        .modules
        .iter()
        .find(|m| m.module_type == ModuleType::Amp)
        .expect("AMP in def");
    for block in &amp_def.blocks {
        if block.is_multi_fx() {
            println!(
                "    Multi-FX block '{}': linked_fx = {:?}",
                block.label, block.linked_fx_indices
            );
            assert!(block.resolve_fx_index(0).is_some());
        }
    }

    println!("PASS");
    Ok(())
}

/// Convert a name into a simple slug for IDs.
fn slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
