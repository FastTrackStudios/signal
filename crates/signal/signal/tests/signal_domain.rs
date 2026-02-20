//! Domain integration tests for the JM (Archetype: John Mayer X) rig.
//!
//! Tests the full hierarchy:
//!   blocks → modules → layers → engines → rigs
//!
//! Uses an in-memory SQLite database seeded with `runtime_seed_bundle()` — no
//! REAPER connection required. Run with:
//!
//!   cargo test -p signal --test signal_domain -- --nocapture

use signal::{
    bootstrap_in_memory_controller_async,
    overrides::{NodePath, Override},
    rig::{EngineSelection, RigScene},
    seed_id, BlockType, ModuleBlockSource,
};

// ─────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────

async fn controller() -> signal::Signal {
    bootstrap_in_memory_controller_async()
        .await
        .expect("failed to bootstrap in-memory controller")
}

// ─────────────────────────────────────────────────────────────
//  Block-level tests
// ─────────────────────────────────────────────────────────────

/// Verify the JM block presets are present in the seeded DB.
#[tokio::test]
async fn jm_block_presets_are_seeded() {
    let signal = controller().await;

    let boosts = signal.block_presets().list(BlockType::Boost).await.unwrap();
    assert!(
        boosts
            .iter()
            .any(|p| p.id().to_string() == seed_id("jm-justa-boost").to_string()),
        "jm-justa-boost not found in Boost collections"
    );

    let drives = signal.block_presets().list(BlockType::Drive).await.unwrap();
    assert!(
        drives
            .iter()
            .any(|p| p.id().to_string() == seed_id("jm-halfman-od").to_string()),
        "jm-halfman-od not found in Drive collections"
    );
    assert!(
        drives
            .iter()
            .any(|p| p.id().to_string() == seed_id("jm-tealbreaker").to_string()),
        "jm-tealbreaker not found in Drive collections"
    );

    let amps = signal.block_presets().list(BlockType::Amp).await.unwrap();
    assert!(
        amps.iter()
            .any(|p| p.id().to_string() == seed_id("jm-amp").to_string()),
        "jm-amp not found in Amp collections"
    );

    println!(
        "✓ JM block presets seeded: boosts={}, drives={}, amps={}",
        boosts.len(),
        drives.len(),
        amps.len()
    );
}

/// Load the default snapshot of jm-amp and verify parameter values.
#[tokio::test]
async fn load_jm_amp_default_block() {
    let signal = controller().await;

    let block = signal
        .block_presets()
        .load_default(BlockType::Amp, seed_id("jm-amp"))
        .await
        .unwrap()
        .expect("jm-amp default snapshot not found");

    let params = block.parameters().to_vec();
    println!("jm-amp default params:");
    for p in &params {
        println!("  {} = {:.3}", p.id(), p.value().get());
    }

    assert_eq!(params.len(), 6, "expected 6 amp parameters");

    let gain = params
        .iter()
        .find(|p| p.id() == "gain")
        .expect("gain not found");
    assert!(
        (gain.value().get() - 0.45).abs() < 0.001,
        "gain should be 0.45, got {}",
        gain.value().get()
    );

    let master = params
        .iter()
        .find(|p| p.id() == "master")
        .expect("master not found");
    assert!(
        (master.value().get() - 0.50).abs() < 0.001,
        "master should be 0.50, got {}",
        master.value().get()
    );
}

/// Load Lead and Clean snapshots of jm-amp and verify lead has higher gain.
#[tokio::test]
async fn load_jm_amp_lead_snapshot() {
    let signal = controller().await;

    let lead_block = signal
        .block_presets()
        .load_variant(BlockType::Amp, seed_id("jm-amp"), seed_id("jm-amp-lead"))
        .await
        .unwrap()
        .expect("jm-amp lead snapshot not found");

    let clean_block = signal
        .block_presets()
        .load_variant(BlockType::Amp, seed_id("jm-amp"), seed_id("jm-amp-clean"))
        .await
        .unwrap()
        .expect("jm-amp clean snapshot not found");

    let lead_gain = lead_block
        .parameters()
        .iter()
        .find(|p| p.id() == "gain")
        .unwrap()
        .value()
        .get();
    let clean_gain = clean_block
        .parameters()
        .iter()
        .find(|p| p.id() == "gain")
        .unwrap()
        .value()
        .get();

    println!("jm-amp gain: lead={:.2} clean={:.2}", lead_gain, clean_gain);
    assert!(
        lead_gain > clean_gain,
        "lead gain ({:.2}) should exceed clean gain ({:.2})",
        lead_gain,
        clean_gain
    );
}

/// Mutate a block's parameters, save back, reload, and verify persistence.
#[tokio::test]
async fn mutate_and_persist_jm_boost_snapshot() {
    let signal = controller().await;

    let original = signal
        .block_presets()
        .load_default(BlockType::Boost, seed_id("jm-justa-boost"))
        .await
        .unwrap()
        .expect("jm-justa-boost default not found");

    let original_level = original
        .parameters()
        .iter()
        .find(|p| p.id() == "level")
        .unwrap()
        .value()
        .get();

    let mut mutated = original.clone();
    let new_level = 0.99_f32;
    if let Some(idx) = mutated.parameters().iter().position(|p| p.id() == "level") {
        mutated.set_parameter_value(idx, new_level);
    }

    signal
        .block_presets()
        .update_snapshot_params(
            BlockType::Boost,
            seed_id("jm-justa-boost"),
            seed_id("jm-justa-boost-default"),
            mutated,
        )
        .await
        .unwrap();

    let reloaded = signal
        .block_presets()
        .load_default(BlockType::Boost, seed_id("jm-justa-boost"))
        .await
        .unwrap()
        .expect("jm-justa-boost default not found after save");

    let reloaded_level = reloaded
        .parameters()
        .iter()
        .find(|p| p.id() == "level")
        .unwrap()
        .value()
        .get();

    println!(
        "jm-justa-boost level: original={:.2} → mutated={:.2} → reloaded={:.2}",
        original_level, new_level, reloaded_level
    );
    assert!(
        (reloaded_level - new_level).abs() < 0.001,
        "reloaded level {reloaded_level:.3} should be {new_level:.3}"
    );
}

// ─────────────────────────────────────────────────────────────
//  Module-level tests
// ─────────────────────────────────────────────────────────────

/// Verify all JM module collections are seeded.
#[tokio::test]
async fn jm_module_presets_are_seeded() {
    let signal = controller().await;

    let modules = signal.module_presets().list().await.unwrap();

    let jm_module_ids = [
        "jm-pedals",
        "jm-pre-fx",
        "jm-amp-module",
        "jm-cab-module",
        "jm-eq-module",
        "jm-post-fx",
    ];

    for id in &jm_module_ids {
        let found = modules
            .iter()
            .any(|m| m.id().to_string() == seed_id(id).to_string());
        assert!(found, "module preset '{id}' not found in seeded DB");
        println!("  ✓ {id}");
    }

    println!("✓ All {} JM module presets found", jm_module_ids.len());
}

/// Load the JM pedals module default snapshot and verify it has 5 blocks.
#[tokio::test]
async fn jm_pedals_module_has_5_blocks() {
    let signal = controller().await;

    let snapshot = signal
        .module_presets()
        .load_default(seed_id("jm-pedals"))
        .await
        .unwrap()
        .expect("jm-pedals default snapshot not found");

    let module = snapshot.module().clone();
    let blocks = module.blocks();
    println!("jm-pedals blocks:");
    for b in &blocks {
        println!("  {} ({:?})", b.label(), b.block_type());
    }

    assert_eq!(blocks.len(), 5, "expected 5 blocks in jm-pedals");
}

/// Load the "Lead" variant of jm-pedals and verify justa-boost uses the Edge snapshot.
#[tokio::test]
async fn jm_pedals_lead_variant_uses_edge_snapshot() {
    let signal = controller().await;

    let default_snap = signal
        .module_presets()
        .load_default(seed_id("jm-pedals"))
        .await
        .unwrap()
        .expect("jm-pedals default not found");

    let lead_snap = signal
        .module_presets()
        .load_variant(seed_id("jm-pedals"), seed_id("jm-pedals-lead"))
        .await
        .unwrap()
        .expect("jm-pedals lead variant not found");

    // Collect blocks into owned vecs to avoid temporary borrow issues
    let default_module = default_snap.module().clone();
    let lead_module = lead_snap.module().clone();
    let default_blocks = default_module.blocks();
    let lead_blocks = lead_module.blocks();

    let default_boost = default_blocks
        .iter()
        .find(|b| b.id() == "justa-boost")
        .expect("justa-boost block not in default");
    let lead_boost = lead_blocks
        .iter()
        .find(|b| b.id() == "justa-boost")
        .expect("justa-boost block not in lead");

    println!("default justa-boost source: {:?}", default_boost.source());
    println!("lead justa-boost source:    {:?}", lead_boost.source());

    // Default should use PresetDefault, lead should use the "edge" snapshot
    match lead_boost.source() {
        ModuleBlockSource::PresetSnapshot { snapshot_id, .. } => {
            assert_eq!(
                snapshot_id.to_string(),
                seed_id("jm-justa-boost-edge").to_string(),
                "lead variant should reference jm-justa-boost-edge snapshot"
            );
            println!("✓ Lead variant correctly selects jm-justa-boost-edge");
        }
        other => panic!(
            "expected PresetSnapshot source for lead boost, got {:?}",
            other
        ),
    }
}

/// Load the JM amp module and verify it has 4 snapshots (default + 3 named).
#[tokio::test]
async fn jm_amp_module_has_4_snapshots() {
    let signal = controller().await;

    let modules = signal.module_presets().list().await.unwrap();
    let amp_module = modules
        .iter()
        .find(|m| m.id().to_string() == seed_id("jm-amp-module").to_string())
        .expect("jm-amp-module not found");

    println!("jm-amp-module snapshots:");
    for s in amp_module.snapshots() {
        println!("  {} — {}", s.id(), s.name());
    }

    // 4 = default + clean + crunch + lead
    assert_eq!(
        amp_module.snapshots().len(),
        4,
        "expected 4 amp module snapshots"
    );
}

// ─────────────────────────────────────────────────────────────
//  Layer-level tests
// ─────────────────────────────────────────────────────────────

/// Verify the guitar-layer-archetype-jm layer is present and has both variants.
#[tokio::test]
async fn guitar_layer_archetype_jm_is_seeded() {
    let signal = controller().await;

    let layer = signal
        .layers()
        .load(seed_id("guitar-layer-archetype-jm"))
        .await
        .unwrap()
        .expect("guitar-layer-archetype-jm not found");

    println!("Layer '{}' variants:", layer.name);
    for v in &layer.variants {
        println!(
            "  {} — {} ({} module refs)",
            v.id,
            v.name,
            v.module_refs.len()
        );
    }

    assert_eq!(
        layer.variants.len(),
        2,
        "expected 2 layer variants (default + lead)"
    );
}

/// Load the default layer variant and verify it references all 6 JM modules.
#[tokio::test]
async fn jm_layer_default_has_6_module_refs() {
    let signal = controller().await;

    let variant = signal
        .layers()
        .load_variant(
            seed_id("guitar-layer-archetype-jm"),
            seed_id("guitar-layer-archetype-jm-default"),
        )
        .await
        .unwrap()
        .expect("jm layer default variant not found");

    println!("Module refs in default variant:");
    for mr in &variant.module_refs {
        println!("  collection={}", mr.collection_id);
    }

    assert_eq!(
        variant.module_refs.len(),
        6,
        "expected 6 module refs in default variant"
    );
}

/// Load the Lead layer variant and verify it selects jm-amp-module-crunch.
#[tokio::test]
async fn jm_layer_lead_variant_selects_crunch_amp() {
    let signal = controller().await;

    let lead_variant = signal
        .layers()
        .load_variant(
            seed_id("guitar-layer-archetype-jm"),
            seed_id("guitar-layer-archetype-jm-lead"),
        )
        .await
        .unwrap()
        .expect("jm layer lead variant not found");

    let amp_ref = lead_variant
        .module_refs
        .iter()
        .find(|mr| mr.collection_id.to_string() == seed_id("jm-amp-module").to_string())
        .expect("jm-amp-module ref not found in lead variant");

    let amp_variant_id = amp_ref
        .variant_id
        .as_ref()
        .expect("amp module ref in lead variant should have explicit variant_id");

    println!("Lead variant amp module → variant: {}", amp_variant_id);
    assert_eq!(
        amp_variant_id.to_string(),
        seed_id("jm-amp-module-crunch").to_string(),
        "lead variant should select jm-amp-module-crunch"
    );
}

/// Save a modified layer and confirm changes persist.
#[tokio::test]
async fn save_and_reload_jm_layer() {
    let signal = controller().await;

    let mut layer = signal
        .layers()
        .load(seed_id("guitar-layer-archetype-jm"))
        .await
        .unwrap()
        .expect("layer not found");

    let original_name = layer.name.clone();
    layer.name = format!("{} (modified)", original_name);

    signal.layers().save(layer).await.unwrap();

    let reloaded = signal
        .layers()
        .load(seed_id("guitar-layer-archetype-jm"))
        .await
        .unwrap()
        .expect("layer not found after save");

    println!("Layer name: '{}' → '{}'", original_name, reloaded.name);
    assert_eq!(reloaded.name, format!("{} (modified)", original_name));
}

// ─────────────────────────────────────────────────────────────
//  Engine-level tests
// ─────────────────────────────────────────────────────────────

/// Verify the guitar engine is seeded and references the JM archetype layer.
#[tokio::test]
async fn guitar_engine_has_jm_layer() {
    let signal = controller().await;

    let engine = signal
        .engines()
        .load(seed_id("guitar-engine"))
        .await
        .unwrap()
        .expect("guitar-engine not found");

    println!(
        "Engine '{}': {} layer(s), {} variant(s)",
        engine.name,
        engine.layer_ids.len(),
        engine.variants.len()
    );

    for lid in &engine.layer_ids {
        println!("  layer_id: {}", lid);
    }

    let has_jm_layer = engine
        .layer_ids
        .iter()
        .any(|id| id.to_string() == seed_id("guitar-layer-archetype-jm").to_string());

    assert!(
        has_jm_layer,
        "guitar-engine should reference guitar-layer-archetype-jm"
    );
}

/// Load the default scene of the guitar engine.
#[tokio::test]
async fn guitar_engine_default_scene_loads() {
    let signal = controller().await;

    let scene = signal
        .engines()
        .load_variant(seed_id("guitar-engine"), seed_id("guitar-engine-default"))
        .await
        .unwrap()
        .expect("guitar-engine default scene not found");

    println!(
        "Engine scene '{}': {} layer selections, {} overrides",
        scene.name,
        scene.layer_selections.len(),
        scene.overrides.len()
    );

    assert!(!scene.name.is_empty());
}

// ─────────────────────────────────────────────────────────────
//  Rig-level tests
// ─────────────────────────────────────────────────────────────

/// Verify the guitar MegaRig is seeded with multiple scenes.
#[tokio::test]
async fn guitar_megarig_is_seeded() {
    let signal = controller().await;

    let rigs = signal.rigs().list().await.unwrap();
    println!("Rigs:");
    for r in &rigs {
        println!("  {} — {} ({} variants)", r.id, r.name, r.variants.len());
    }

    let guitar_rig = rigs
        .iter()
        .find(|r| r.id.to_string() == seed_id("guitar-megarig").to_string())
        .expect("guitar-megarig not found in rig collections");

    assert!(
        guitar_rig.variants.len() >= 2,
        "guitar-megarig should have at least 2 scenes, got {}",
        guitar_rig.variants.len()
    );
}

/// Load the guitar MegaRig lead scene and verify it has engine selections and overrides.
#[tokio::test]
async fn guitar_megarig_lead_scene_has_overrides() {
    let signal = controller().await;

    let scene = signal
        .rigs()
        .load_variant(seed_id("guitar-megarig"), seed_id("guitar-megarig-lead"))
        .await
        .unwrap()
        .expect("guitar-megarig lead scene not found");

    println!(
        "Rig scene '{}': {} engine selections, {} overrides",
        scene.name,
        scene.engine_selections.len(),
        scene.overrides.len()
    );

    assert!(
        !scene.engine_selections.is_empty(),
        "lead scene should have engine selections"
    );
    assert!(
        !scene.overrides.is_empty(),
        "lead scene should have overrides"
    );
}

/// Add a custom scene to the guitar MegaRig with a parameter override, save, reload, verify.
#[tokio::test]
async fn save_rig_with_custom_scene_and_override() {
    let signal = controller().await;

    let mut rig = signal
        .rigs()
        .load(seed_id("guitar-megarig"))
        .await
        .unwrap()
        .expect("guitar-megarig not found");

    let original_count = rig.variants.len();

    // Build a custom scene that overrides the JM amp gain to 0.80
    let custom_scene = RigScene::new(signal::seed_id("guitar-megarig-test-custom"), "Custom Test")
        .with_engine(EngineSelection::new(
            seed_id("guitar-engine"),
            seed_id("guitar-engine-default"),
        ))
        .with_override(Override::set(
            NodePath::engine("guitar-engine")
                .with_layer("guitar-layer-archetype-jm")
                .with_module("jm-amp-module")
                .with_block("amp")
                .with_parameter("gain"),
            0.80,
        ));

    rig.variants.push(custom_scene);
    signal.rigs().save(rig).await.unwrap();

    let reloaded = signal
        .rigs()
        .load(seed_id("guitar-megarig"))
        .await
        .unwrap()
        .expect("guitar-megarig not found after save");

    println!(
        "Rig variants: {} → {} (added custom scene)",
        original_count,
        reloaded.variants.len()
    );

    assert_eq!(
        reloaded.variants.len(),
        original_count + 1,
        "should have one more variant after save"
    );

    let custom = reloaded
        .variants
        .iter()
        .find(|v| v.name == "Custom Test")
        .expect("Custom Test scene not found after reload");

    assert_eq!(
        custom.overrides.len(),
        1,
        "custom scene should have 1 override"
    );
    println!("Override path: {}", custom.overrides[0].path.as_str());
    assert!(
        custom.overrides[0].path.as_str().contains("gain"),
        "override path should reference 'gain'"
    );
}
