//! Keys Engine API integration tests.
//!
//! Exercises multi-engine rig management: adding/removing engines, engine and
//! layer presets with snapshots/scenes, cross-engine scene switching, multi-level
//! overrides (layer replace, module replace, block replace, param set), and
//! full resolve pipeline for the keys rig.
//!
//! Uses the seeded keys-megarig (4 engines: Keys, Synth, Organ, Pad) as a base,
//! and also builds custom engine/layer/rig structures from scratch.
//!
//!   cargo test -p signal --test signal_keys_engine_api -- --nocapture

mod fixtures;

use fixtures::*;
use signal::{
    block::BlockType,
    engine::{Engine, EngineScene, LayerSelection},
    layer::{BlockRef, Layer, LayerRef, LayerSnapshot, ModuleRef},
    module_type::ModuleType,
    overrides::{NodeOverrideOp, NodePath, Override},
    profile::{Patch, PatchTarget, Profile},
    resolve::{ResolveTarget, ResolvedGraph},
    rig::{EngineSelection, Rig, RigId, RigScene, RigSceneId, RigType},
    seed_id,
    setlist::{Setlist, SetlistEntry},
    song::{Section, SectionSource, Song},
    traits::Collection,
    Block, BlockParameter, EngineType, ModuleBlock, ModuleBlockSource, ModulePreset,
    ModuleSnapshot, Preset, SignalChain, Snapshot,
};

// ═════════════════════════════════════════════════════════════
//  Group A: Seeded Keys Rig Verification (4 tests)
// ═════════════════════════════════════════════════════════════

/// The seeded keys-megarig has 4 engines and 4 scenes.
#[tokio::test]
async fn keys_megarig_structure() {
    let signal = controller().await;

    let rig = signal
        .rigs()
        .load(keys_megarig_id())
        .await
        .unwrap()
        .expect("keys-megarig should exist");

    assert_eq!(rig.name, "MegaRig");
    assert_eq!(rig.rig_type, Some(RigType::Keys));
    assert_eq!(rig.engine_ids.len(), 4, "should have 4 engines");
    assert_eq!(rig.variants.len(), 4, "should have 4 scenes");

    let scene_names: Vec<&str> = rig.variants.iter().map(|v| v.name.as_str()).collect();
    assert!(scene_names.contains(&"Default"));
    assert!(scene_names.contains(&"Wide"));
    assert!(scene_names.contains(&"Focus"));
    assert!(scene_names.contains(&"Air"));
}

/// Each keys engine has the expected layer count and scene count.
#[tokio::test]
async fn keys_engines_have_correct_structure() {
    let signal = controller().await;

    let engines = signal.engines().list().await.unwrap();

    let keys = engines
        .iter()
        .find(|e| e.name == "Keys Engine")
        .expect("Keys Engine");
    assert_eq!(keys.engine_type, EngineType::Keys);
    assert_eq!(keys.layer_ids.len(), 2, "Keys Engine: 2 layers");
    assert_eq!(keys.variants.len(), 2, "Keys Engine: 2 scenes");

    let synth = engines
        .iter()
        .find(|e| e.name == "Synth Engine")
        .expect("Synth Engine");
    assert_eq!(synth.engine_type, EngineType::Synth);
    assert_eq!(synth.layer_ids.len(), 3, "Synth Engine: 3 layers");
    assert_eq!(synth.variants.len(), 2, "Synth Engine: 2 scenes");

    let organ = engines
        .iter()
        .find(|e| e.name == "Organ Engine")
        .expect("Organ Engine");
    assert_eq!(organ.engine_type, EngineType::Organ);
    assert_eq!(organ.layer_ids.len(), 2, "Organ Engine: 2 layers");

    let pad = engines
        .iter()
        .find(|e| e.name == "Pad Engine")
        .expect("Pad Engine");
    assert_eq!(pad.engine_type, EngineType::Pad);
    assert_eq!(pad.layer_ids.len(), 2, "Pad Engine: 2 layers");
}

/// Resolve the keys-megarig default scene and verify multi-engine graph.
#[tokio::test]
async fn resolve_keys_default_scene_multi_engine() {
    let signal = controller().await;

    let graph = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: keys_megarig_id(),
            scene_id: keys_megarig_default_scene(),
        })
        .await
        .expect("resolve should succeed");

    assert!(
        graph.engines.len() >= 2,
        "resolved graph should have multiple engines, got {}",
        graph.engines.len()
    );

    // At least one engine should have resolved layers with blocks
    let total_layers: usize = graph.engines.iter().map(|e| e.layers.len()).sum();
    assert!(
        total_layers >= 2,
        "should have at least 2 resolved layers, got {total_layers}"
    );
}

/// Resolve all 4 keys scenes and verify each produces a valid graph.
#[tokio::test]
async fn resolve_all_keys_scenes() {
    let signal = controller().await;

    let scenes = [
        ("Default", keys_megarig_default_scene()),
        ("Wide", keys_megarig_wide_scene()),
        ("Focus", keys_megarig_focus_scene()),
        ("Air", keys_megarig_air_scene()),
    ];

    for (name, scene_id) in &scenes {
        let result = signal
            .resolve_target(ResolveTarget::RigScene {
                rig_id: keys_megarig_id(),
                scene_id: scene_id.clone(),
            })
            .await;
        assert!(
            result.is_ok(),
            "failed to resolve keys scene '{name}': {:?}",
            result.err()
        );
        let graph = result.unwrap();
        assert!(
            !graph.engines.is_empty(),
            "keys scene '{name}' should have engines"
        );
    }
}

// ═════════════════════════════════════════════════════════════
//  Group B: Engine CRUD + Presets/Scenes (5 tests)
// ═════════════════════════════════════════════════════════════

/// Create a new engine with multiple scenes (presets), save, reload, verify.
#[tokio::test]
async fn create_engine_with_multiple_scenes() {
    let signal = controller().await;

    // First create layers for this engine
    let layer_a_snap = LayerSnapshot::new(seed_id("custom-keys-core-snap"), "Default");
    let layer_a = Layer::new(
        seed_id("custom-keys-core"),
        "Custom Keys Core",
        EngineType::Keys,
        layer_a_snap,
    );
    let layer_b_snap = LayerSnapshot::new(seed_id("custom-keys-fx-snap"), "Default");
    let layer_b = Layer::new(
        seed_id("custom-keys-fx"),
        "Custom Keys FX",
        EngineType::Keys,
        layer_b_snap,
    );
    signal.layers().save(layer_a).await.unwrap();
    signal.layers().save(layer_b).await.unwrap();

    // Create engine with 3 scenes
    let default_scene = EngineScene::new(seed_id("cke-scene-warm"), "Warm")
        .with_layer(LayerSelection::new(
            seed_id("custom-keys-core"),
            seed_id("custom-keys-core-snap"),
        ))
        .with_layer(LayerSelection::new(
            seed_id("custom-keys-fx"),
            seed_id("custom-keys-fx-snap"),
        ));

    let bright_scene =
        EngineScene::new(seed_id("cke-scene-bright"), "Bright").with_layer(LayerSelection::new(
            seed_id("custom-keys-core"),
            seed_id("custom-keys-core-snap"),
        ));

    let ambient_scene = EngineScene::new(seed_id("cke-scene-ambient"), "Ambient")
        .with_layer(LayerSelection::new(
            seed_id("custom-keys-fx"),
            seed_id("custom-keys-fx-snap"),
        ))
        .with_override(Override::set(
            NodePath::layer("custom-keys-fx").with_parameter("mix"),
            0.8,
        ));

    let mut engine = Engine::new(
        seed_id("custom-keys-engine"),
        "Custom Keys Engine",
        EngineType::Keys,
        vec![
            seed_id("custom-keys-core").into(),
            seed_id("custom-keys-fx").into(),
        ],
        default_scene,
    );
    engine.add_variant(bright_scene);
    engine.add_variant(ambient_scene);
    signal.engines().save(engine).await.unwrap();

    let loaded = signal
        .engines()
        .load(seed_id("custom-keys-engine"))
        .await
        .unwrap()
        .expect("engine should exist");

    assert_eq!(loaded.name, "Custom Keys Engine");
    assert_eq!(loaded.engine_type, EngineType::Keys);
    assert_eq!(loaded.layer_ids.len(), 2);
    assert_eq!(loaded.variants.len(), 3);

    let scene_names: Vec<&str> = loaded.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(scene_names, vec!["Warm", "Bright", "Ambient"]);
}

/// Add a new scene to an existing engine, save, reload, verify.
#[tokio::test]
async fn add_scene_to_existing_engine() {
    let signal = controller().await;

    let engines = signal.engines().list().await.unwrap();
    let mut keys_engine = engines
        .iter()
        .find(|e| e.name == "Keys Engine")
        .cloned()
        .expect("Keys Engine should exist");

    let original_count = keys_engine.variants.len();

    let new_scene = EngineScene::new(seed_id("keys-engine-dark"), "Dark")
        .with_layer(LayerSelection::new(
            seed_id("keys-layer-core"),
            seed_id("keys-layer-core-default"),
        ))
        .with_override(Override::set(
            NodePath::layer("keys-layer-core")
                .with_block("keys-core-eq")
                .with_parameter("high_shelf"),
            0.2,
        ));

    keys_engine.add_variant(new_scene);
    signal.engines().save(keys_engine).await.unwrap();

    let reloaded = signal
        .engines()
        .load(seed_id("keys-engine"))
        .await
        .unwrap()
        .expect("keys engine");

    assert_eq!(reloaded.variants.len(), original_count + 1);
    let dark = reloaded
        .variants
        .iter()
        .find(|v| v.name == "Dark")
        .expect("Dark scene should exist");
    assert_eq!(dark.overrides.len(), 1);
}

/// Load a specific engine scene variant by ID.
#[tokio::test]
async fn load_engine_scene_by_id() {
    let signal = controller().await;

    let scene = signal
        .engines()
        .load_variant(seed_id("keys-engine"), seed_id("keys-engine-bright"))
        .await
        .unwrap()
        .expect("bright scene should exist");

    assert_eq!(scene.name, "Bright");
    assert!(!scene.layer_selections.is_empty());
}

/// Engine scene overrides survive save/load round-trip.
#[tokio::test]
async fn engine_scene_overrides_round_trip() {
    let signal = controller().await;

    let snap = LayerSnapshot::new(seed_id("ovr-eng-layer-snap"), "Default");
    let layer = Layer::new(
        seed_id("ovr-eng-layer"),
        "Override Test Layer",
        EngineType::Keys,
        snap,
    );
    signal.layers().save(layer).await.unwrap();

    let scene = EngineScene::new(seed_id("ovr-eng-scene"), "Override Scene")
        .with_layer(LayerSelection::new(
            seed_id("ovr-eng-layer"),
            seed_id("ovr-eng-layer-snap"),
        ))
        .with_override(Override::set(
            NodePath::layer("ovr-eng-layer").with_parameter("level"),
            0.7,
        ))
        .with_override(Override::set(
            NodePath::layer("ovr-eng-layer")
                .with_block("eq")
                .with_parameter("bass"),
            0.3,
        ));

    let engine = Engine::new(
        seed_id("ovr-eng"),
        "Override Engine",
        EngineType::Keys,
        vec![seed_id("ovr-eng-layer").into()],
        scene,
    );
    signal.engines().save(engine).await.unwrap();

    let loaded = signal
        .engines()
        .load(seed_id("ovr-eng"))
        .await
        .unwrap()
        .expect("engine");
    assert_eq!(loaded.variants[0].overrides.len(), 2);
}

/// Delete an engine, verify gone.
#[tokio::test]
async fn delete_engine() {
    let signal = controller().await;

    let snap = LayerSnapshot::new(seed_id("del-eng-layer-snap"), "Default");
    let layer = Layer::new(
        seed_id("del-eng-layer"),
        "Ephemeral Layer",
        EngineType::Keys,
        snap,
    );
    signal.layers().save(layer).await.unwrap();

    let engine = Engine::new(
        seed_id("del-eng"),
        "Ephemeral Engine",
        EngineType::Keys,
        vec![seed_id("del-eng-layer").into()],
        EngineScene::new(seed_id("del-eng-scene"), "Default"),
    );
    signal.engines().save(engine).await.unwrap();

    let exists = signal.engines().load(seed_id("del-eng")).await.unwrap();
    assert!(exists.is_some());

    signal.engines().delete(seed_id("del-eng")).await.unwrap();

    let gone = signal.engines().load(seed_id("del-eng")).await.unwrap();
    assert!(gone.is_none());
}

// ═════════════════════════════════════════════════════════════
//  Group C: Layer Presets + Snapshots (5 tests)
// ═════════════════════════════════════════════════════════════

/// Create a keys layer with multiple variants (snapshots), each selecting different refs.
#[tokio::test]
async fn layer_with_multiple_ref_types() {
    let signal = controller().await;

    let mut default_snap = LayerSnapshot::new(seed_id("mixed-refs-default"), "Default");
    default_snap.module_refs = vec![ModuleRef::new(seed_id("time-parallel"))];
    default_snap.block_refs = vec![
        BlockRef::new(seed_id("jm-comp")),
        BlockRef::new(seed_id("eq-proq4")),
    ];

    let mut alt_snap = LayerSnapshot::new(seed_id("mixed-refs-alt"), "Alt");
    alt_snap.module_refs = vec![
        ModuleRef::new(seed_id("time-parallel")).with_variant(seed_id("time-parallel-ambient"))
    ];
    alt_snap.block_refs = vec![BlockRef::new(seed_id("jm-comp"))];

    let mut layer = Layer::new(
        seed_id("mixed-refs-layer"),
        "Mixed Refs Layer",
        EngineType::Keys,
        default_snap,
    );
    layer.add_variant(alt_snap);
    signal.layers().save(layer).await.unwrap();

    let loaded = signal
        .layers()
        .load(seed_id("mixed-refs-layer"))
        .await
        .unwrap()
        .expect("layer should exist");

    assert_eq!(loaded.variants.len(), 2);

    let default = &loaded.variants[0];
    assert_eq!(default.module_refs.len(), 1);
    assert_eq!(default.block_refs.len(), 2);

    let alt = signal
        .layers()
        .load_variant(seed_id("mixed-refs-layer"), seed_id("mixed-refs-alt"))
        .await
        .unwrap()
        .expect("alt variant");
    assert_eq!(alt.name, "Alt");
    assert!(alt.module_refs[0].variant_id.is_some());
}

/// Layer with layer_refs (cross-layer embedding) round-trips.
#[tokio::test]
async fn layer_with_layer_refs() {
    let signal = controller().await;

    let mut snap = LayerSnapshot::new(seed_id("layerref-snap"), "Default");
    snap.layer_refs = vec![
        LayerRef::new(seed_id("guitar-layer-main")),
        LayerRef::new(seed_id("guitar-layer-main")).with_variant(seed_id("guitar-layer-main-lead")),
    ];

    let layer = Layer::new(
        seed_id("layerref-layer"),
        "Layer Ref Test",
        EngineType::Keys,
        snap,
    );
    signal.layers().save(layer).await.unwrap();

    let loaded = signal
        .layers()
        .load(seed_id("layerref-layer"))
        .await
        .unwrap()
        .expect("layer");

    assert_eq!(loaded.variants[0].layer_refs.len(), 2);
    assert!(loaded.variants[0].layer_refs[0].variant_id.is_none());
    assert!(loaded.variants[0].layer_refs[1].variant_id.is_some());
}

/// Layer snapshot overrides at block-parameter level survive round-trip.
#[tokio::test]
async fn layer_snapshot_block_param_overrides() {
    let signal = controller().await;

    let mut snap = LayerSnapshot::new(seed_id("block-param-snap"), "Overridden");
    snap.block_refs = vec![BlockRef::new(seed_id("jm-comp"))];
    snap.overrides = vec![
        Override::set(NodePath::block("comp").with_parameter("threshold"), 0.46),
        Override::set(NodePath::block("comp").with_parameter("ratio"), 0.6),
        Override::set(NodePath::block("eq").with_parameter("high_shelf"), 0.68),
    ];

    let layer = Layer::new(
        seed_id("block-param-layer"),
        "Block Param Layer",
        EngineType::Keys,
        snap,
    );
    signal.layers().save(layer).await.unwrap();

    let loaded = signal
        .layers()
        .load(seed_id("block-param-layer"))
        .await
        .unwrap()
        .expect("layer");

    assert_eq!(loaded.variants[0].overrides.len(), 3);
    let paths: Vec<String> = loaded.variants[0]
        .overrides
        .iter()
        .map(|o| o.path.as_str().to_string())
        .collect();
    assert!(paths.iter().any(|p| p.contains("threshold")));
    assert!(paths.iter().any(|p| p.contains("ratio")));
    assert!(paths.iter().any(|p| p.contains("high_shelf")));
}

/// Enabled/disabled flag on layer snapshots persists.
#[tokio::test]
async fn layer_snapshot_enabled_flag() {
    let signal = controller().await;

    let mut enabled_snap = LayerSnapshot::new(seed_id("enabled-snap"), "Enabled");
    enabled_snap.enabled = true;
    let mut disabled_snap = LayerSnapshot::new(seed_id("disabled-snap"), "Disabled");
    disabled_snap.enabled = false;

    let mut layer = Layer::new(
        seed_id("enabled-test-layer"),
        "Enabled Test",
        EngineType::Keys,
        enabled_snap,
    );
    layer.add_variant(disabled_snap);
    signal.layers().save(layer).await.unwrap();

    let loaded = signal
        .layers()
        .load(seed_id("enabled-test-layer"))
        .await
        .unwrap()
        .expect("layer");

    assert!(loaded.variants[0].enabled);
    assert!(!loaded.variants[1].enabled);
}

/// Delete a layer, verify gone.
#[tokio::test]
async fn delete_layer() {
    let signal = controller().await;

    let layer = Layer::new(
        seed_id("ephemeral-layer"),
        "Ephemeral",
        EngineType::Keys,
        LayerSnapshot::new(seed_id("ephemeral-snap"), "Default"),
    );
    signal.layers().save(layer).await.unwrap();

    assert!(signal
        .layers()
        .load(seed_id("ephemeral-layer"))
        .await
        .unwrap()
        .is_some());
    signal
        .layers()
        .delete(seed_id("ephemeral-layer"))
        .await
        .unwrap();
    assert!(signal
        .layers()
        .load(seed_id("ephemeral-layer"))
        .await
        .unwrap()
        .is_none());
}

// ═════════════════════════════════════════════════════════════
//  Group D: Add/Remove Engines from Rig (4 tests)
// ═════════════════════════════════════════════════════════════

/// Add a 5th engine to the keys megarig, save, reload, verify.
#[tokio::test]
async fn add_engine_to_rig() {
    let signal = controller().await;

    // Create a new effects engine
    let fx_snap = LayerSnapshot::new(seed_id("fx-eng-layer-snap"), "Default");
    let fx_layer = Layer::new(
        seed_id("fx-eng-layer"),
        "FX Layer",
        EngineType::Keys,
        fx_snap,
    );
    signal.layers().save(fx_layer).await.unwrap();

    let fx_engine = Engine::new(
        seed_id("fx-engine"),
        "FX Engine",
        EngineType::Keys,
        vec![seed_id("fx-eng-layer").into()],
        EngineScene::new(seed_id("fx-eng-scene-default"), "Default"),
    );
    signal.engines().save(fx_engine).await.unwrap();

    // Load rig, add engine
    let mut rig = signal
        .rigs()
        .load(keys_megarig_id())
        .await
        .unwrap()
        .expect("keys rig");
    let original_engine_count = rig.engine_ids.len();

    rig.engine_ids.push(seed_id("fx-engine").into());
    signal.rigs().save(rig).await.unwrap();

    let reloaded = signal
        .rigs()
        .load(keys_megarig_id())
        .await
        .unwrap()
        .expect("keys rig");
    assert_eq!(reloaded.engine_ids.len(), original_engine_count + 1);
}

/// Remove an engine from the keys megarig, save, reload, verify.
#[tokio::test]
async fn remove_engine_from_rig() {
    let signal = controller().await;

    // Create a fresh rig with 3 engines
    let snap_a = LayerSnapshot::new(seed_id("rem-layer-a-snap"), "Default");
    let layer_a = Layer::new(seed_id("rem-layer-a"), "Layer A", EngineType::Keys, snap_a);
    let snap_b = LayerSnapshot::new(seed_id("rem-layer-b-snap"), "Default");
    let layer_b = Layer::new(seed_id("rem-layer-b"), "Layer B", EngineType::Synth, snap_b);
    let snap_c = LayerSnapshot::new(seed_id("rem-layer-c-snap"), "Default");
    let layer_c = Layer::new(seed_id("rem-layer-c"), "Layer C", EngineType::Organ, snap_c);
    signal.layers().save(layer_a).await.unwrap();
    signal.layers().save(layer_b).await.unwrap();
    signal.layers().save(layer_c).await.unwrap();

    let eng_a = Engine::new(
        seed_id("rem-eng-a"),
        "Engine A",
        EngineType::Keys,
        vec![seed_id("rem-layer-a").into()],
        EngineScene::new(seed_id("rem-eng-a-scene"), "Default"),
    );
    let eng_b = Engine::new(
        seed_id("rem-eng-b"),
        "Engine B",
        EngineType::Synth,
        vec![seed_id("rem-layer-b").into()],
        EngineScene::new(seed_id("rem-eng-b-scene"), "Default"),
    );
    let eng_c = Engine::new(
        seed_id("rem-eng-c"),
        "Engine C",
        EngineType::Organ,
        vec![seed_id("rem-layer-c").into()],
        EngineScene::new(seed_id("rem-eng-c-scene"), "Default"),
    );
    signal.engines().save(eng_a).await.unwrap();
    signal.engines().save(eng_b).await.unwrap();
    signal.engines().save(eng_c).await.unwrap();

    let rig = Rig::new(
        seed_id("removable-rig"),
        "Removable Rig",
        vec![
            seed_id("rem-eng-a").into(),
            seed_id("rem-eng-b").into(),
            seed_id("rem-eng-c").into(),
        ],
        RigScene::new(seed_id("removable-rig-default"), "Default"),
    );
    signal.rigs().save(rig).await.unwrap();

    // Remove engine B
    let mut loaded = signal
        .rigs()
        .load(seed_id("removable-rig"))
        .await
        .unwrap()
        .expect("rig");
    let eng_b_id = seed_id("rem-eng-b").to_string();
    loaded.engine_ids.retain(|id| id.as_str() != eng_b_id);
    signal.rigs().save(loaded).await.unwrap();

    let reloaded = signal
        .rigs()
        .load(seed_id("removable-rig"))
        .await
        .unwrap()
        .expect("rig");
    assert_eq!(reloaded.engine_ids.len(), 2);
    assert!(!reloaded.engine_ids.iter().any(|id| id.as_str() == eng_b_id));
}

/// Build a multi-engine rig from scratch with cross-engine scene selections.
#[tokio::test]
async fn build_multi_engine_rig_from_scratch() {
    let signal = controller().await;

    // Use seeded engines
    let scene_a = RigScene::new(seed_id("multi-rig-scene-a"), "Scene A")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-default"),
        ));

    let scene_b = RigScene::new(seed_id("multi-rig-scene-b"), "Scene B")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-bright"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-scene-b"),
        ));

    let mut rig = Rig::new(
        seed_id("custom-multi-rig"),
        "Custom Multi Rig",
        vec![
            seed_id("keys-engine").into(),
            seed_id("synth-engine").into(),
        ],
        scene_a,
    )
    .with_rig_type("keys");
    rig.add_variant(scene_b);
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("custom-multi-rig"))
        .await
        .unwrap()
        .expect("rig");

    assert_eq!(loaded.engine_ids.len(), 2);
    assert_eq!(loaded.variants.len(), 2);

    // Scene A and B should select different engine scenes
    let a_keys_scene = &loaded.variants[0].engine_selections[0].variant_id;
    let b_keys_scene = &loaded.variants[1].engine_selections[0].variant_id;
    assert_ne!(a_keys_scene, b_keys_scene);
}

/// Reorder engines within a rig.
#[tokio::test]
async fn reorder_engines_in_rig() {
    let signal = controller().await;

    let mut rig = signal
        .rigs()
        .load(seed_id("removable-rig"))
        .await
        .unwrap()
        .or_else(|| {
            // Might not exist if tests run in isolation — create fresh
            None
        });

    // Create a fresh rig for this test
    let snap_x = LayerSnapshot::new(seed_id("reord-layer-x-snap"), "Default");
    let layer_x = Layer::new(
        seed_id("reord-layer-x"),
        "Layer X",
        EngineType::Keys,
        snap_x,
    );
    let snap_y = LayerSnapshot::new(seed_id("reord-layer-y-snap"), "Default");
    let layer_y = Layer::new(
        seed_id("reord-layer-y"),
        "Layer Y",
        EngineType::Synth,
        snap_y,
    );
    signal.layers().save(layer_x).await.unwrap();
    signal.layers().save(layer_y).await.unwrap();

    let eng_x = Engine::new(
        seed_id("reord-eng-x"),
        "Engine X",
        EngineType::Keys,
        vec![seed_id("reord-layer-x").into()],
        EngineScene::new(seed_id("reord-eng-x-scene"), "Default"),
    );
    let eng_y = Engine::new(
        seed_id("reord-eng-y"),
        "Engine Y",
        EngineType::Synth,
        vec![seed_id("reord-layer-y").into()],
        EngineScene::new(seed_id("reord-eng-y-scene"), "Default"),
    );
    signal.engines().save(eng_x).await.unwrap();
    signal.engines().save(eng_y).await.unwrap();

    let rig_to_save = Rig::new(
        seed_id("reord-rig"),
        "Reorder Engine Rig",
        vec![seed_id("reord-eng-x").into(), seed_id("reord-eng-y").into()],
        RigScene::new(seed_id("reord-rig-default"), "Default"),
    );
    signal.rigs().save(rig_to_save).await.unwrap();

    // Swap engine order: [Y, X]
    let mut loaded = signal
        .rigs()
        .load(seed_id("reord-rig"))
        .await
        .unwrap()
        .expect("rig");
    loaded.engine_ids.reverse();
    signal.rigs().save(loaded).await.unwrap();

    let reloaded = signal
        .rigs()
        .load(seed_id("reord-rig"))
        .await
        .unwrap()
        .expect("rig");

    assert_eq!(
        reloaded.engine_ids[0].as_str(),
        seed_id("reord-eng-y").to_string()
    );
    assert_eq!(
        reloaded.engine_ids[1].as_str(),
        seed_id("reord-eng-x").to_string()
    );
}

// ═════════════════════════════════════════════════════════════
//  Group E: Multi-Level Override System (4 tests)
// ═════════════════════════════════════════════════════════════

/// Rig scene with overrides at every level: engine, layer, module, block, param.
#[tokio::test]
async fn multi_level_overrides_in_rig_scene() {
    let signal = controller().await;

    let scene = RigScene::new(seed_id("ml-override-scene"), "Multi-Level")
        .with_override(Override::set(
            NodePath::engine("keys-engine")
                .with_layer("keys-layer-space")
                .with_block("keys-space-verb")
                .with_parameter("mix"),
            0.63,
        ))
        .with_override(Override::set(
            NodePath::engine("synth-engine")
                .with_layer("synth-layer-motion")
                .with_module("time-parallel")
                .with_parameter("feedback"),
            0.42,
        ))
        .with_override(Override::set(
            NodePath::engine("organ-engine").with_parameter("volume"),
            0.5,
        ));

    let rig = Rig::new(
        seed_id("ml-override-rig"),
        "Multi-Level Override Rig",
        vec![],
        scene,
    );
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("ml-override-rig"))
        .await
        .unwrap()
        .expect("rig");

    assert_eq!(loaded.variants[0].overrides.len(), 3);

    // Verify path depths
    let paths: Vec<String> = loaded.variants[0]
        .overrides
        .iter()
        .map(|o| o.path.as_str().to_string())
        .collect();
    // Deepest path: engine.layer.block.param (4 segments)
    assert!(paths.iter().any(|p| p.contains("mix")));
    // Medium path: engine.layer.module.param (4 segments)
    assert!(paths.iter().any(|p| p.contains("feedback")));
    // Shallow path: engine.param (2 segments)
    assert!(paths.iter().any(|p| p.contains("volume")));
}

/// ReplaceRef override: replace a layer variant within a rig scene.
#[tokio::test]
async fn replace_ref_override_in_rig_scene() {
    let signal = controller().await;

    let scene =
        RigScene::new(seed_id("replace-ref-scene"), "Replace Ref Test").with_override(Override {
            path: NodePath::engine("synth-engine").with_layer("synth-layer-osc"),
            op: NodeOverrideOp::ReplaceRef("synth-layer-osc-alt".to_string()),
        });

    let rig = Rig::new(seed_id("replace-ref-rig"), "Replace Ref Rig", vec![], scene);
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("replace-ref-rig"))
        .await
        .unwrap()
        .expect("rig");

    let ovr = &loaded.variants[0].overrides[0];
    assert!(
        matches!(&ovr.op, NodeOverrideOp::ReplaceRef(ref_id) if ref_id == "synth-layer-osc-alt")
    );
}

/// Bypass override on a block.
#[tokio::test]
async fn bypass_override_in_rig_scene() {
    let signal = controller().await;

    let scene =
        RigScene::new(seed_id("bypass-scene"), "Bypass Test").with_override(Override::bypass(
            NodePath::engine("keys-engine")
                .with_layer("keys-layer-core")
                .with_block("keys-core-comp"),
            true,
        ));

    let rig = Rig::new(seed_id("bypass-rig"), "Bypass Rig", vec![], scene);
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("bypass-rig"))
        .await
        .unwrap()
        .expect("rig");

    let ovr = &loaded.variants[0].overrides[0];
    assert!(matches!(&ovr.op, NodeOverrideOp::Bypass(true)));
}

/// Mixed override types in a single scene.
#[tokio::test]
async fn mixed_override_types_in_scene() {
    let signal = controller().await;

    let scene = RigScene::new(seed_id("mixed-ovr-scene"), "Mixed Overrides")
        .with_override(Override::set(
            NodePath::engine("keys-engine")
                .with_layer("keys-layer-core")
                .with_block("eq")
                .with_parameter("gain"),
            0.75,
        ))
        .with_override(Override::bypass(
            NodePath::engine("synth-engine")
                .with_layer("synth-layer-texture")
                .with_block("texture-verb"),
            true,
        ))
        .with_override(Override {
            path: NodePath::engine("synth-engine").with_layer("synth-layer-osc"),
            op: NodeOverrideOp::ReplaceRef("synth-layer-osc-alt".to_string()),
        });

    let rig = Rig::new(
        seed_id("mixed-ovr-rig"),
        "Mixed Override Rig",
        vec![],
        scene,
    );
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("mixed-ovr-rig"))
        .await
        .unwrap()
        .expect("rig");

    let overrides = &loaded.variants[0].overrides;
    assert_eq!(overrides.len(), 3);

    let has_set = overrides
        .iter()
        .any(|o| matches!(&o.op, NodeOverrideOp::Set(_)));
    let has_bypass = overrides
        .iter()
        .any(|o| matches!(&o.op, NodeOverrideOp::Bypass(_)));
    let has_replace = overrides
        .iter()
        .any(|o| matches!(&o.op, NodeOverrideOp::ReplaceRef(_)));
    assert!(has_set, "should have Set override");
    assert!(has_bypass, "should have Bypass override");
    assert!(has_replace, "should have ReplaceRef override");
}

// ═════════════════════════════════════════════════════════════
//  Group F: Keys Profile + Song Lifecycle (4 tests)
// ═════════════════════════════════════════════════════════════

/// The seeded Keys Feature profile has 4 patches targeting keys rig scenes.
#[tokio::test]
async fn keys_profile_structure() {
    let signal = controller().await;

    let profiles = signal.profiles().list().await.unwrap();
    let keys_profile = profiles
        .iter()
        .find(|p| p.name == "Keys Feature")
        .expect("Keys Feature profile should be seeded");

    assert_eq!(keys_profile.patches.len(), 4);

    let patch_names: Vec<&str> = keys_profile
        .patches
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert!(patch_names.contains(&"Foundation"));
    assert!(patch_names.contains(&"Wide"));
    assert!(patch_names.contains(&"Focus"));
    assert!(patch_names.contains(&"Air"));

    // All patches should target the keys megarig
    for patch in &keys_profile.patches {
        match &patch.target {
            PatchTarget::RigScene { rig_id, .. } => {
                assert_eq!(
                    rig_id.as_str(),
                    keys_megarig_id().as_str(),
                    "patch '{}' should target keys megarig",
                    patch.name
                );
            }
            _ => panic!("patch '{}' should have RigScene target", patch.name),
        }
    }
}

/// Resolve keys profile patches — each should produce a valid multi-engine graph.
#[tokio::test]
async fn resolve_keys_profile_patches() {
    let signal = controller().await;

    let profiles = signal.profiles().list().await.unwrap();
    let keys_profile = profiles
        .iter()
        .find(|p| p.name == "Keys Feature")
        .expect("Keys Feature");

    for patch in &keys_profile.patches {
        let result = signal
            .resolve_target(ResolveTarget::ProfilePatch {
                profile_id: keys_profile.id.clone(),
                patch_id: patch.id.clone(),
            })
            .await;
        assert!(
            result.is_ok(),
            "failed to resolve keys patch '{}': {:?}",
            patch.name,
            result.err()
        );
        let graph = result.unwrap();
        assert!(
            !graph.engines.is_empty(),
            "keys patch '{}' should resolve to engines",
            patch.name
        );
    }
}

/// Create a custom keys profile, retarget patches between scenes.
#[tokio::test]
async fn custom_keys_profile_with_retarget() {
    let signal = controller().await;

    let mut profile = Profile::new(
        seed_id("custom-keys-profile"),
        "Custom Keys Profile",
        Patch::from_rig_scene(
            seed_id("ckp-warm"),
            "Warm",
            keys_megarig_id(),
            keys_megarig_default_scene(),
        ),
    );
    profile.add_patch(Patch::from_rig_scene(
        seed_id("ckp-bright"),
        "Bright",
        keys_megarig_id(),
        keys_megarig_wide_scene(),
    ));
    signal.profiles().save(profile).await.unwrap();

    // Retarget "Bright" to the Focus scene
    signal
        .profiles()
        .set_patch_preset(
            seed_id("custom-keys-profile"),
            seed_id("ckp-bright"),
            keys_megarig_id(),
            keys_megarig_focus_scene(),
        )
        .await
        .unwrap();

    let loaded = signal
        .profiles()
        .load(seed_id("custom-keys-profile"))
        .await
        .unwrap()
        .expect("profile");
    let bright = loaded
        .patches
        .iter()
        .find(|p| p.name == "Bright")
        .expect("Bright patch");
    match &bright.target {
        PatchTarget::RigScene { scene_id, .. } => {
            assert_eq!(*scene_id, keys_megarig_focus_scene());
        }
        _ => panic!("expected RigScene target"),
    }
}

/// Feature-Demo Song sections resolve through the keys rig.
#[tokio::test]
async fn feature_demo_song_resolves() {
    let signal = controller().await;

    let songs = signal.songs().list().await.unwrap();
    let demo = songs
        .iter()
        .find(|s| s.name == "Feature-Demo Song")
        .expect("Feature-Demo Song should be seeded");

    assert_eq!(demo.sections.len(), 4);

    for section in &demo.sections {
        let result = signal
            .resolve_target(ResolveTarget::SongSection {
                song_id: demo.id.clone(),
                section_id: section.id.clone(),
            })
            .await;
        assert!(
            result.is_ok(),
            "failed to resolve section '{}': {:?}",
            section.name,
            result.err()
        );
    }
}

// ═════════════════════════════════════════════════════════════
//  Group G: Cross-Engine Scene Switching (3 tests)
// ═════════════════════════════════════════════════════════════

/// Different rig scenes select different engine scenes for the same engine.
#[tokio::test]
async fn different_scenes_select_different_engine_scenes() {
    let signal = controller().await;

    let default_scene = signal
        .rigs()
        .load_variant(keys_megarig_id(), keys_megarig_default_scene())
        .await
        .unwrap()
        .expect("default scene");
    let wide_scene = signal
        .rigs()
        .load_variant(keys_megarig_id(), keys_megarig_wide_scene())
        .await
        .unwrap()
        .expect("wide scene");

    // Both should have engine selections
    assert!(!default_scene.engine_selections.is_empty());
    assert!(!wide_scene.engine_selections.is_empty());

    // Check that at least one engine scene differs between Default and Wide
    let default_scenes: Vec<String> = default_scene
        .engine_selections
        .iter()
        .map(|s| s.variant_id.as_str().to_string())
        .collect();
    let wide_scenes: Vec<String> = wide_scene
        .engine_selections
        .iter()
        .map(|s| s.variant_id.as_str().to_string())
        .collect();

    assert_ne!(
        default_scenes, wide_scenes,
        "Default and Wide should select different engine scene combinations"
    );
}

/// Resolve two different rig scenes and compare their resolved engine structures.
#[tokio::test]
async fn resolved_graphs_differ_between_scenes() {
    let signal = controller().await;

    let graph_default = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: keys_megarig_id(),
            scene_id: keys_megarig_default_scene(),
        })
        .await
        .expect("default");

    let graph_wide = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: keys_megarig_id(),
            scene_id: keys_megarig_wide_scene(),
        })
        .await
        .expect("wide");

    // Both should have engines
    assert!(!graph_default.engines.is_empty());
    assert!(!graph_wide.engines.is_empty());

    // The scene IDs should differ
    assert_ne!(
        graph_default.rig_scene_id, graph_wide.rig_scene_id,
        "resolved graphs should have different scene ids"
    );
}

/// Build a rig with mixed engine types (Keys + Synth + Organ) and resolve.
#[tokio::test]
async fn mixed_engine_type_rig_resolves() {
    let signal = controller().await;

    let scene = RigScene::new(seed_id("mixed-type-scene"), "Mixed")
        .with_engine(EngineSelection::new(
            seed_id("keys-engine"),
            seed_id("keys-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("synth-engine"),
            seed_id("synth-engine-default"),
        ))
        .with_engine(EngineSelection::new(
            seed_id("organ-engine"),
            seed_id("organ-engine-default"),
        ));

    let rig = Rig::new(
        seed_id("mixed-type-rig"),
        "Mixed Type Rig",
        vec![
            seed_id("keys-engine").into(),
            seed_id("synth-engine").into(),
            seed_id("organ-engine").into(),
        ],
        scene,
    )
    .with_rig_type("keys");
    signal.rigs().save(rig).await.unwrap();

    let graph = signal
        .resolve_target(ResolveTarget::RigScene {
            rig_id: seed_id("mixed-type-rig").into(),
            scene_id: seed_id("mixed-type-scene").into(),
        })
        .await
        .expect("should resolve mixed engine rig");

    assert!(
        graph.engines.len() >= 2,
        "should resolve multiple engines, got {}",
        graph.engines.len()
    );
}

// ═════════════════════════════════════════════════════════════
//  Group H: Engine Scene Reordering (2 tests)
// ═════════════════════════════════════════════════════════════

/// Reorder engine scenes by mutating variants and re-saving.
#[tokio::test]
async fn reorder_engine_scenes() {
    let signal = controller().await;

    // Create engine with 4 scenes
    let snap = LayerSnapshot::new(seed_id("reord-scene-layer-snap"), "Default");
    let layer = Layer::new(
        seed_id("reord-scene-layer"),
        "Reorder Scene Layer",
        EngineType::Keys,
        snap,
    );
    signal.layers().save(layer).await.unwrap();

    let mut engine = Engine::new(
        seed_id("reord-scene-engine"),
        "Reorder Scene Engine",
        EngineType::Keys,
        vec![seed_id("reord-scene-layer").into()],
        EngineScene::new(seed_id("rse-a"), "A"),
    );
    engine.add_variant(EngineScene::new(seed_id("rse-b"), "B"));
    engine.add_variant(EngineScene::new(seed_id("rse-c"), "C"));
    engine.add_variant(EngineScene::new(seed_id("rse-d"), "D"));
    signal.engines().save(engine).await.unwrap();

    // Reorder to [D, B, C, A] by manipulating variants
    let mut loaded = signal
        .engines()
        .load(seed_id("reord-scene-engine"))
        .await
        .unwrap()
        .expect("engine");
    let original = loaded.variants.clone();
    loaded.variants = vec![
        original.iter().find(|v| v.name == "D").unwrap().clone(),
        original.iter().find(|v| v.name == "B").unwrap().clone(),
        original.iter().find(|v| v.name == "C").unwrap().clone(),
        original.iter().find(|v| v.name == "A").unwrap().clone(),
    ];
    signal.engines().save(loaded).await.unwrap();

    let reloaded = signal
        .engines()
        .load(seed_id("reord-scene-engine"))
        .await
        .unwrap()
        .expect("engine");
    let names: Vec<&str> = reloaded.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["D", "B", "C", "A"]);
}

/// Reorder layer variants by mutating and re-saving.
#[tokio::test]
async fn reorder_layer_variants() {
    let signal = controller().await;

    let mut layer = Layer::new(
        seed_id("reord-variant-layer"),
        "Reorder Variant Layer",
        EngineType::Keys,
        LayerSnapshot::new(seed_id("rvl-alpha"), "Alpha"),
    );
    layer.add_variant(LayerSnapshot::new(seed_id("rvl-beta"), "Beta"));
    layer.add_variant(LayerSnapshot::new(seed_id("rvl-gamma"), "Gamma"));
    signal.layers().save(layer).await.unwrap();

    // Reorder to [Gamma, Alpha, Beta]
    let mut loaded = signal
        .layers()
        .load(seed_id("reord-variant-layer"))
        .await
        .unwrap()
        .expect("layer");
    let original = loaded.variants.clone();
    loaded.variants = vec![
        original.iter().find(|v| v.name == "Gamma").unwrap().clone(),
        original.iter().find(|v| v.name == "Alpha").unwrap().clone(),
        original.iter().find(|v| v.name == "Beta").unwrap().clone(),
    ];
    signal.layers().save(loaded).await.unwrap();

    let reloaded = signal
        .layers()
        .load(seed_id("reord-variant-layer"))
        .await
        .unwrap()
        .expect("layer");
    let names: Vec<&str> = reloaded.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["Gamma", "Alpha", "Beta"]);
}

// ═════════════════════════════════════════════════════════════
//  Group I: Full Hierarchy Resolve Sweep (2 tests)
// ═════════════════════════════════════════════════════════════

/// Resolve all seeded keys profile patches end-to-end.
#[tokio::test]
async fn resolve_all_keys_patches_end_to_end() {
    let signal = controller().await;

    let profiles = signal.profiles().list().await.unwrap();
    let keys_profile = profiles
        .iter()
        .find(|p| p.name == "Keys Feature")
        .expect("Keys Feature profile");

    let mut total_engines = 0;
    for patch in &keys_profile.patches {
        let graph = signal
            .resolve_target(ResolveTarget::ProfilePatch {
                profile_id: keys_profile.id.clone(),
                patch_id: patch.id.clone(),
            })
            .await
            .expect("resolve");
        total_engines += graph.engines.len();
    }

    assert!(
        total_engines >= 4,
        "should have resolved at least 4 engines across all patches, got {total_engines}"
    );
}

/// Full setlist → song → section → resolve sweep including keys songs.
#[tokio::test]
async fn full_setlist_sweep_includes_keys() {
    let signal = controller().await;

    let setlists = signal.setlists().list().await.unwrap();
    assert!(!setlists.is_empty());

    let mut resolved_count = 0;
    let mut keys_song_found = false;

    for setlist in &setlists {
        for entry in &setlist.entries {
            if let Some(song) = signal.songs().load(entry.song_id.clone()).await.unwrap() {
                if song.name == "Feature-Demo Song" {
                    keys_song_found = true;
                }
                for section in &song.sections {
                    let result = signal
                        .resolve_target(ResolveTarget::SongSection {
                            song_id: song.id.clone(),
                            section_id: section.id.clone(),
                        })
                        .await;
                    assert!(
                        result.is_ok(),
                        "failed to resolve '{}' / '{}': {:?}",
                        song.name,
                        section.name,
                        result.err()
                    );
                    resolved_count += 1;
                }
            }
        }
    }

    assert!(
        resolved_count > 0,
        "should have resolved at least one section"
    );
    assert!(keys_song_found, "Feature-Demo Song should be in a setlist");
}
