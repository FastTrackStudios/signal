//! Comprehensive Guitar Rig API integration tests.
//!
//! Exercises the full domain lifecycle: blocks → modules → layers → engines →
//! rigs → profiles → songs → setlists, plus reorder, resolve, smart diff,
//! and morph engine.
//!
//! Uses an in-memory SQLite database — no REAPER required.
//!
//!   cargo test -p signal --test signal_guitar_rig_api -- --nocapture

mod fixtures;

use fixtures::*;
use signal::{
    block::BlockType,
    engine::{Engine, EngineScene, LayerSelection},
    layer::{Layer, LayerSnapshot, ModuleRef},
    module_type::ModuleType,
    overrides::{NodePath, Override},
    profile::{Patch, PatchTarget, Profile},
    resolve::{ResolveTarget, ResolvedGraph},
    rig::{EngineSelection, Rig, RigId, RigScene, RigSceneId},
    seed_id,
    setlist::{Setlist, SetlistEntry},
    song::{Section, SectionSource, Song},
    traits::Collection,
    Block, BlockParameter, BlockParameterOverride, DawParamValue, DawParameterSnapshot, EngineType,
    Module, ModuleBlock, ModuleBlockSource, ModulePreset, ModulePresetId, ModuleSnapshot,
    ModuleSnapshotId, MorphEngine, Preset, PresetId, SignalChain, SignalNode, Snapshot, SnapshotId,
};
use signal_live::engine::{
    compute_diff, slot::InstanceHandle, ModuleTarget, ResolvedSlot, SlotDiff, SlotState,
};

// ═════════════════════════════════════════════════════════════
//  Group A: Block Collection CRUD
// ═════════════════════════════════════════════════════════════

/// Create a brand-new block collection with 3 snapshots, save, reload, verify.
#[tokio::test]
async fn create_new_block_collection() {
    let signal = controller().await;

    let default_snap = Snapshot::new(
        seed_id("custom-drive-default"),
        "Default",
        Block::from_parameters(vec![
            BlockParameter::new("gain", "Gain", 0.5),
            BlockParameter::new("tone", "Tone", 0.5),
        ]),
    );
    let crunch = Snapshot::new(
        seed_id("custom-drive-crunch"),
        "Crunch",
        Block::from_parameters(vec![
            BlockParameter::new("gain", "Gain", 0.7),
            BlockParameter::new("tone", "Tone", 0.6),
        ]),
    );
    let high_gain = Snapshot::new(
        seed_id("custom-drive-high"),
        "High Gain",
        Block::from_parameters(vec![
            BlockParameter::new("gain", "Gain", 0.95),
            BlockParameter::new("tone", "Tone", 0.4),
        ]),
    );

    let preset = Preset::new(
        seed_id("custom-drive-collection"),
        "Custom Drive",
        BlockType::Drive,
        default_snap,
        vec![crunch, high_gain],
    );

    signal.block_presets().save(preset).await.unwrap();

    // Reload
    let collections = signal.block_presets().list(BlockType::Drive).await.unwrap();
    let loaded = collections
        .iter()
        .find(|p| p.name() == "Custom Drive")
        .expect("custom drive collection should exist");

    assert_eq!(loaded.snapshots().len(), 3);
    assert_eq!(loaded.default_snapshot().name(), "Default");

    // Verify specific snapshot
    let block = signal
        .block_presets()
        .load_variant(
            BlockType::Drive,
            loaded.id().clone(),
            seed_id("custom-drive-high"),
        )
        .await
        .unwrap()
        .expect("high gain snapshot should exist");
    assert!((block.first_value().unwrap() - 0.95).abs() < 0.001);
}

/// Add a snapshot to an existing seeded block collection.
#[tokio::test]
async fn add_snapshot_to_existing_collection() {
    let signal = controller().await;

    let amp_collections = signal.block_presets().list(BlockType::Amp).await.unwrap();
    let mut jm_amp = amp_collections
        .iter()
        .find(|p| p.id().to_string() == seed_id("jm-amp").to_string())
        .cloned()
        .expect("jm-amp should be seeded");

    let original_count = jm_amp.snapshots().len();

    // Add a new "High Gain" snapshot
    let new_snap = Snapshot::new(
        seed_id("jm-amp-highgain"),
        "High Gain",
        Block::from_parameters(vec![
            BlockParameter::new("gain", "Gain", 0.9),
            BlockParameter::new("bass", "Bass", 0.5),
            BlockParameter::new("mid", "Mid", 0.5),
            BlockParameter::new("treble", "Treble", 0.5),
            BlockParameter::new("presence", "Presence", 0.5),
            BlockParameter::new("master", "Master", 0.6),
        ]),
    );
    jm_amp.variants_mut().push(new_snap);
    signal.block_presets().save(jm_amp).await.unwrap();

    // Reload and verify
    let reloaded = signal.block_presets().list(BlockType::Amp).await.unwrap();
    let loaded = reloaded
        .iter()
        .find(|p| p.id().to_string() == seed_id("jm-amp").to_string())
        .expect("jm-amp should still exist");

    assert_eq!(loaded.snapshots().len(), original_count + 1);

    let high_gain = signal
        .block_presets()
        .load_variant(
            BlockType::Amp,
            loaded.id().clone(),
            seed_id("jm-amp-highgain"),
        )
        .await
        .unwrap()
        .expect("high gain snapshot should exist");
    assert!((high_gain.first_value().unwrap() - 0.9).abs() < 0.001);
}

/// Verify update_snapshot_params increments the version.
#[tokio::test]
async fn update_snapshot_params_increments_version() {
    let signal = controller().await;

    // Load the jm-amp Lead snapshot's version before
    let collections = signal.block_presets().list(BlockType::Amp).await.unwrap();
    let jm_amp = collections
        .iter()
        .find(|p| p.id().to_string() == seed_id("jm-amp").to_string())
        .expect("jm-amp");

    let lead_snap = jm_amp
        .snapshots()
        .iter()
        .find(|s| s.name() == "Lead")
        .expect("Lead snapshot");
    let version_before = lead_snap.version();

    // Update params
    let mut block = lead_snap.block();
    block.set_first_value(0.85);
    signal
        .block_presets()
        .update_snapshot_params(
            BlockType::Amp,
            jm_amp.id().clone(),
            lead_snap.id().clone(),
            block,
        )
        .await
        .unwrap();

    // Reload
    let reloaded = signal.block_presets().list(BlockType::Amp).await.unwrap();
    let jm_amp_reloaded = reloaded
        .iter()
        .find(|p| p.id().to_string() == seed_id("jm-amp").to_string())
        .expect("jm-amp");
    let lead_reloaded = jm_amp_reloaded
        .snapshots()
        .iter()
        .find(|s| s.name() == "Lead")
        .expect("Lead snapshot");

    assert_eq!(lead_reloaded.version(), version_before + 1);
    assert!((lead_reloaded.block().first_value().unwrap() - 0.85).abs() < 0.001);
}

/// Block collections are isolated by type.
#[tokio::test]
async fn block_collections_isolated_by_type() {
    let signal = controller().await;

    let custom = Preset::with_default_snapshot(
        seed_id("isolated-drive"),
        "Isolated Drive",
        BlockType::Drive,
        Snapshot::new(
            seed_id("isolated-drive-snap"),
            "Default",
            Block::from_parameters(vec![BlockParameter::new("gain", "Gain", 0.5)]),
        ),
    );
    signal.block_presets().save(custom).await.unwrap();

    let amp_collections = signal.block_presets().list(BlockType::Amp).await.unwrap();
    assert!(
        !amp_collections.iter().any(|p| p.name() == "Isolated Drive"),
        "drive collection should not appear in amp list"
    );
}

/// Delete a block preset and verify it's gone from the listing.
#[tokio::test]
async fn delete_block_preset() {
    let signal = controller().await;

    // Create a custom drive collection so we have something to delete
    let preset = Preset::new(
        seed_id("delete-me-drive"),
        "Delete Me Drive",
        BlockType::Drive,
        Snapshot::new(
            seed_id("delete-me-drive-default"),
            "Default",
            Block::from_parameters(vec![BlockParameter::new("gain", "Gain", 0.5)]),
        ),
        vec![],
    );
    signal.block_presets().save(preset).await.unwrap();

    // Verify it exists
    let before = signal.block_presets().list(BlockType::Drive).await.unwrap();
    let count_before = before.len();
    assert!(
        before.iter().any(|p| p.name() == "Delete Me Drive"),
        "preset should exist before deletion"
    );

    // Delete it
    signal
        .block_presets()
        .delete(BlockType::Drive, seed_id("delete-me-drive"))
        .await
        .unwrap();

    // Verify it's gone
    let after = signal.block_presets().list(BlockType::Drive).await.unwrap();
    assert_eq!(after.len(), count_before - 1, "count should decrease by 1");
    assert!(
        !after.iter().any(|p| p.name() == "Delete Me Drive"),
        "deleted preset should not appear in listing"
    );
}

// ═════════════════════════════════════════════════════════════
//  Group B: Module Collection CRUD
// ═════════════════════════════════════════════════════════════

/// Create a new module collection with a serial signal chain.
#[tokio::test]
async fn create_new_module_collection() {
    let signal = controller().await;

    let module = Module::from_blocks(vec![
        ModuleBlock::new(
            "drive",
            "Drive",
            BlockType::Drive,
            ModuleBlockSource::PresetDefault {
                preset_id: PresetId::from_uuid(seed_id("jm-halfman-od")),
                saved_at_version: None,
            },
        ),
        ModuleBlock::new(
            "amp",
            "Amp",
            BlockType::Amp,
            ModuleBlockSource::PresetDefault {
                preset_id: PresetId::from_uuid(seed_id("jm-amp")),
                saved_at_version: None,
            },
        ),
        ModuleBlock::new(
            "eq",
            "EQ",
            BlockType::Eq,
            ModuleBlockSource::Inline {
                block: Block::from_parameters(vec![
                    BlockParameter::new("bass", "Bass", 0.5),
                    BlockParameter::new("mid", "Mid", 0.6),
                    BlockParameter::new("treble", "Treble", 0.5),
                ]),
            },
        ),
    ]);

    let snapshot = ModuleSnapshot::new(seed_id("custom-guitar-mod-default"), "Default", module);
    let preset = ModulePreset::new(
        seed_id("custom-guitar-module"),
        "Custom Guitar Module",
        ModuleType::Drive,
        snapshot,
        vec![],
    );

    signal.module_presets().save(preset).await.unwrap();

    // Reload
    let modules = signal.module_presets().list().await.unwrap();
    let loaded = modules
        .iter()
        .find(|m| m.name() == "Custom Guitar Module")
        .expect("custom guitar module should exist");

    assert_eq!(loaded.module_type(), ModuleType::Drive);
    let default = loaded.default_snapshot();
    assert_eq!(default.module().blocks().len(), 3);
    assert_eq!(default.module().blocks()[0].id(), "drive");
    assert_eq!(default.module().blocks()[1].id(), "amp");
    assert_eq!(default.module().blocks()[2].id(), "eq");
}

/// Add a variant to an existing module collection.
#[tokio::test]
async fn add_variant_to_module_collection() {
    let signal = controller().await;

    let modules = signal.module_presets().list().await.unwrap();
    let jm_pedals = modules
        .iter()
        .find(|m| m.name() == "JM Pedals")
        .cloned()
        .expect("JM Pedals should exist in seed data");

    let original_count = jm_pedals.snapshots().len();

    // Create a "Heavy" variant with different block sources
    let heavy_module = Module::from_blocks(vec![ModuleBlock::new(
        "boost",
        "Boost",
        BlockType::Boost,
        ModuleBlockSource::PresetDefault {
            preset_id: PresetId::from_uuid(seed_id("jm-justa-boost")),
            saved_at_version: None,
        },
    )
    .with_overrides(vec![BlockParameterOverride::new("level", 0.9)])]);
    let heavy_snap = ModuleSnapshot::new(seed_id("jm-pedals-heavy"), "Heavy", heavy_module);

    let mut updated = jm_pedals;
    updated.variants_mut().push(heavy_snap);
    signal.module_presets().save(updated).await.unwrap();

    // Reload
    let reloaded = signal.module_presets().list().await.unwrap();
    let loaded = reloaded
        .iter()
        .find(|m| m.name() == "JM Pedals")
        .expect("JM Pedals should still exist");

    assert_eq!(loaded.snapshots().len(), original_count + 1);

    let heavy = loaded
        .snapshots()
        .iter()
        .find(|s| s.name() == "Heavy")
        .expect("Heavy variant should exist");
    assert_eq!(heavy.module().blocks().len(), 1);
}

/// Module block sources (PresetDefault, PresetSnapshot) round-trip correctly.
#[tokio::test]
async fn module_block_source_references() {
    let signal = controller().await;

    let module = Module::from_blocks(vec![
        ModuleBlock::new(
            "slot-default",
            "Slot Default",
            BlockType::Drive,
            ModuleBlockSource::PresetDefault {
                preset_id: PresetId::from_uuid(seed_id("jm-halfman-od")),
                saved_at_version: Some(1),
            },
        ),
        ModuleBlock::new(
            "slot-snapshot",
            "Slot Snapshot",
            BlockType::Amp,
            ModuleBlockSource::PresetSnapshot {
                preset_id: PresetId::from_uuid(seed_id("jm-amp")),
                snapshot_id: SnapshotId::from_uuid(seed_id("jm-amp-lead")),
                saved_at_version: Some(1),
            },
        ),
    ]);

    let snap = ModuleSnapshot::new(seed_id("source-test-snap"), "Default", module);
    let preset = ModulePreset::new(
        seed_id("source-test"),
        "Source Test",
        ModuleType::Custom,
        snap,
        vec![],
    );
    signal.module_presets().save(preset).await.unwrap();

    // Reload
    let loaded_snap = signal
        .module_presets()
        .load_default(seed_id("source-test"))
        .await
        .unwrap()
        .expect("should find module");

    let blocks = loaded_snap.module().blocks();
    assert!(matches!(
        blocks[0].source(),
        ModuleBlockSource::PresetDefault { preset_id, .. }
            if preset_id.to_string() == seed_id("jm-halfman-od").to_string()
    ));
    assert!(matches!(
        blocks[1].source(),
        ModuleBlockSource::PresetSnapshot { preset_id, snapshot_id, .. }
            if preset_id.to_string() == seed_id("jm-amp").to_string()
            && snapshot_id.to_string() == seed_id("jm-amp-lead").to_string()
    ));
}

/// Signal chain with split (parallel routing) survives save/load.
#[tokio::test]
async fn module_signal_chain_with_split() {
    let signal = controller().await;

    let chain = SignalChain::new(vec![
        SignalNode::Block(ModuleBlock::new(
            "pre-eq",
            "Pre EQ",
            BlockType::Eq,
            ModuleBlockSource::Inline {
                block: Block::from_parameters(vec![BlockParameter::new("gain", "Gain", 0.5)]),
            },
        )),
        SignalNode::Split {
            lanes: vec![
                SignalChain::serial(vec![ModuleBlock::new(
                    "delay",
                    "Delay",
                    BlockType::Delay,
                    ModuleBlockSource::Inline {
                        block: Block::from_parameters(vec![BlockParameter::new(
                            "time", "Time", 0.4,
                        )]),
                    },
                )]),
                SignalChain::serial(vec![ModuleBlock::new(
                    "reverb",
                    "Reverb",
                    BlockType::Reverb,
                    ModuleBlockSource::Inline {
                        block: Block::from_parameters(vec![BlockParameter::new(
                            "decay", "Decay", 0.7,
                        )]),
                    },
                )]),
            ],
        },
    ]);

    let module = Module::from_chain(chain);
    let snap = ModuleSnapshot::new(seed_id("split-test-snap"), "Default", module);
    let preset = ModulePreset::new(
        seed_id("split-test"),
        "Split Test",
        ModuleType::Time,
        snap,
        vec![],
    );
    signal.module_presets().save(preset).await.unwrap();

    let loaded = signal
        .module_presets()
        .load_default(seed_id("split-test"))
        .await
        .unwrap()
        .expect("should load split module");

    assert!(
        !loaded.module().chain().is_serial(),
        "chain should have a split"
    );
    assert_eq!(loaded.module().blocks().len(), 3); // pre-eq, delay, reverb
}

// ═════════════════════════════════════════════════════════════
//  Group C: Layer Construction
// ═════════════════════════════════════════════════════════════

/// Build a layer from scratch with module_refs.
#[tokio::test]
async fn build_layer_from_scratch() {
    let signal = controller().await;

    let mut default_snap = LayerSnapshot::new(seed_id("test-layer-default"), "Default");
    default_snap.module_refs = vec![
        ModuleRef::new(seed_id("drive-full-stack")),
        ModuleRef::new(seed_id("jm-pedals")),
    ];

    let layer = Layer::new(
        seed_id("test-guitar-layer"),
        "Test Guitar Layer",
        EngineType::Guitar,
        default_snap,
    );
    signal.layers().save(layer).await.unwrap();

    let loaded = signal
        .layers()
        .load(seed_id("test-guitar-layer"))
        .await
        .unwrap()
        .expect("layer should exist");

    assert_eq!(loaded.name, "Test Guitar Layer");
    assert_eq!(loaded.engine_type, EngineType::Guitar);
    assert_eq!(loaded.variants.len(), 1);
    assert_eq!(loaded.variants[0].module_refs.len(), 2);
}

/// Layer with multiple variants selecting different module snapshots.
#[tokio::test]
async fn layer_with_multiple_variants() {
    let signal = controller().await;

    let mut clean = LayerSnapshot::new(seed_id("multi-layer-clean"), "Clean");
    clean.module_refs = vec![ModuleRef::new(seed_id("jm-pedals"))];

    let mut crunch = LayerSnapshot::new(seed_id("multi-layer-crunch"), "Crunch");
    crunch.module_refs =
        vec![ModuleRef::new(seed_id("drive-full-stack"))
            .with_variant(seed_id("drive-full-stack-push"))];

    let mut lead = LayerSnapshot::new(seed_id("multi-layer-lead"), "Lead");
    lead.module_refs = vec![ModuleRef::new(seed_id("drive-full-stack"))];

    let mut layer = Layer::new(
        seed_id("multi-variant-layer"),
        "Multi-Variant Layer",
        EngineType::Guitar,
        clean,
    );
    layer.add_variant(crunch);
    layer.add_variant(lead);
    signal.layers().save(layer).await.unwrap();

    let loaded = signal
        .layers()
        .load(seed_id("multi-variant-layer"))
        .await
        .unwrap()
        .expect("layer should exist");

    assert_eq!(loaded.variants.len(), 3);

    let crunch_loaded = signal
        .layers()
        .load_variant(
            seed_id("multi-variant-layer"),
            seed_id("multi-layer-crunch"),
        )
        .await
        .unwrap()
        .expect("crunch variant should exist");
    assert_eq!(crunch_loaded.name, "Crunch");
    assert!(crunch_loaded.module_refs[0].variant_id.is_some());
}

/// Layer snapshot overrides survive save/load.
#[tokio::test]
async fn layer_snapshot_overrides_round_trip() {
    let signal = controller().await;

    let mut snap = LayerSnapshot::new(seed_id("override-layer-snap"), "With Overrides");
    snap.overrides = vec![
        Override::set(NodePath::block("amp").with_parameter("gain"), 0.75),
        Override::set(NodePath::block("drive").with_parameter("tone"), 0.3),
    ];

    let layer = Layer::new(
        seed_id("override-layer"),
        "Override Layer",
        EngineType::Guitar,
        snap,
    );
    signal.layers().save(layer).await.unwrap();

    let loaded = signal
        .layers()
        .load(seed_id("override-layer"))
        .await
        .unwrap()
        .expect("layer should exist");

    assert_eq!(loaded.variants[0].overrides.len(), 2);
    assert!(loaded.variants[0].overrides[0]
        .path
        .as_str()
        .contains("gain"));
    assert!(loaded.variants[0].overrides[1]
        .path
        .as_str()
        .contains("tone"));
}

// ═════════════════════════════════════════════════════════════
//  Group D: Engine + Rig Construction
// ═════════════════════════════════════════════════════════════

/// Build an engine with layers and multiple scenes.
#[tokio::test]
async fn build_engine_with_layers() {
    let signal = controller().await;

    // First save two layers
    let layer_a_snap = LayerSnapshot::new(seed_id("eng-layer-a-snap"), "Default");
    let layer_a = Layer::new(
        seed_id("eng-layer-a"),
        "Layer A",
        EngineType::Guitar,
        layer_a_snap,
    );
    let layer_b_snap = LayerSnapshot::new(seed_id("eng-layer-b-snap"), "Default");
    let layer_b = Layer::new(
        seed_id("eng-layer-b"),
        "Layer B",
        EngineType::Guitar,
        layer_b_snap,
    );
    signal.layers().save(layer_a).await.unwrap();
    signal.layers().save(layer_b).await.unwrap();

    // Build engine with two scenes
    let default_scene = EngineScene::new(seed_id("eng-scene-default"), "Default").with_layer(
        LayerSelection::new(seed_id("eng-layer-a"), seed_id("eng-layer-a-snap")),
    );
    let lead_scene = EngineScene::new(seed_id("eng-scene-lead"), "Lead").with_layer(
        LayerSelection::new(seed_id("eng-layer-b"), seed_id("eng-layer-b-snap")),
    );

    let mut engine = Engine::new(
        seed_id("test-engine"),
        "Test Engine",
        EngineType::Guitar,
        vec![seed_id("eng-layer-a").into(), seed_id("eng-layer-b").into()],
        default_scene,
    );
    engine.add_variant(lead_scene);
    signal.engines().save(engine).await.unwrap();

    let loaded = signal
        .engines()
        .load(seed_id("test-engine"))
        .await
        .unwrap()
        .expect("engine should exist");

    assert_eq!(loaded.name, "Test Engine");
    assert_eq!(loaded.layer_ids.len(), 2);
    assert_eq!(loaded.variants.len(), 2);

    let lead = signal
        .engines()
        .load_variant(seed_id("test-engine"), seed_id("eng-scene-lead"))
        .await
        .unwrap()
        .expect("lead scene should exist");
    assert_eq!(lead.name, "Lead");
}

/// Build a rig with 4 scenes and overrides.
#[tokio::test]
async fn build_rig_with_scenes() {
    let signal = controller().await;

    let default_scene = RigScene::new(seed_id("rig-scene-clean"), "Clean");
    let crunch = RigScene::new(seed_id("rig-scene-crunch"), "Crunch");
    let lead = RigScene::new(seed_id("rig-scene-lead"), "Lead").with_override(Override::set(
        NodePath::block("amp").with_parameter("gain"),
        0.8,
    ));
    let solo = RigScene::new(seed_id("rig-scene-solo"), "Solo")
        .with_override(Override::set(
            NodePath::block("amp").with_parameter("gain"),
            0.95,
        ))
        .with_override(Override::set(
            NodePath::block("amp").with_parameter("master"),
            0.7,
        ));

    let mut rig = Rig::new(
        seed_id("custom-guitar-rig"),
        "Custom Guitar Rig",
        vec![],
        default_scene,
    );
    rig.add_variant(crunch);
    rig.add_variant(lead);
    rig.add_variant(solo);
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("custom-guitar-rig"))
        .await
        .unwrap()
        .expect("rig should exist");

    assert_eq!(loaded.name, "Custom Guitar Rig");
    assert_eq!(loaded.variants.len(), 4);

    let solo_loaded = signal
        .rigs()
        .load_variant(seed_id("custom-guitar-rig"), seed_id("rig-scene-solo"))
        .await
        .unwrap()
        .expect("solo scene should exist");
    assert_eq!(solo_loaded.overrides.len(), 2);
}

/// Rig scene overrides at different NodePath depths are preserved.
#[tokio::test]
async fn rig_scene_overrides_stack() {
    let signal = controller().await;

    let scene = RigScene::new(seed_id("override-scene"), "Deep Overrides")
        .with_override(Override::set(
            NodePath::engine("guitar-engine").with_parameter("mix"),
            0.5,
        ))
        .with_override(Override::set(
            NodePath::layer("guitar-layer").with_parameter("level"),
            0.3,
        ))
        .with_override(Override::set(
            NodePath::block("amp").with_parameter("gain"),
            0.9,
        ));

    let rig = Rig::new(seed_id("override-rig"), "Override Rig", vec![], scene);
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("override-rig"))
        .await
        .unwrap()
        .expect("rig should exist");

    assert_eq!(loaded.variants[0].overrides.len(), 3);
    let paths: Vec<String> = loaded.variants[0]
        .overrides
        .iter()
        .map(|o| o.path.as_str().to_string())
        .collect();
    assert!(paths.iter().any(|p| p.contains("mix")));
    assert!(paths.iter().any(|p| p.contains("level")));
    assert!(paths.iter().any(|p| p.contains("gain")));
}

/// Each rig scene selects a different engine scene.
#[tokio::test]
async fn rig_scene_engine_selections() {
    let signal = controller().await;

    let scene_a = RigScene::new(seed_id("sel-scene-a"), "Scene A").with_engine(
        EngineSelection::new(seed_id("eng-1"), seed_id("eng-1-default")),
    );
    let scene_b = RigScene::new(seed_id("sel-scene-b"), "Scene B").with_engine(
        EngineSelection::new(seed_id("eng-1"), seed_id("eng-1-lead")),
    );

    let mut rig = Rig::new(
        seed_id("sel-rig"),
        "Selection Rig",
        vec![seed_id("eng-1").into()],
        scene_a,
    );
    rig.add_variant(scene_b);
    signal.rigs().save(rig).await.unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("sel-rig"))
        .await
        .unwrap()
        .expect("rig");

    let a = &loaded.variants[0];
    let b = &loaded.variants[1];
    assert_ne!(
        a.engine_selections[0].variant_id, b.engine_selections[0].variant_id,
        "scenes should select different engine variants"
    );
}

// ═════════════════════════════════════════════════════════════
//  Group E: Reorder Operations (via controller API)
// ═════════════════════════════════════════════════════════════

/// Reorder rig scenes via the controller API.
#[tokio::test]
async fn reorder_rig_scenes() {
    let signal = controller().await;

    let mut rig = Rig::new(
        seed_id("reorder-rig"),
        "Reorder Rig",
        vec![],
        RigScene::new(seed_id("r-scene-a"), "A"),
    );
    rig.add_variant(RigScene::new(seed_id("r-scene-b"), "B"));
    rig.add_variant(RigScene::new(seed_id("r-scene-c"), "C"));
    rig.add_variant(RigScene::new(seed_id("r-scene-d"), "D"));
    signal.rigs().save(rig).await.unwrap();

    // Reorder to [D, B, A, C]
    signal
        .rigs()
        .reorder_scenes(
            seed_id("reorder-rig"),
            &[
                seed_id("r-scene-d").into(),
                seed_id("r-scene-b").into(),
                seed_id("r-scene-a").into(),
                seed_id("r-scene-c").into(),
            ],
        )
        .await
        .unwrap();

    let loaded = signal
        .rigs()
        .load(seed_id("reorder-rig"))
        .await
        .unwrap()
        .expect("rig");

    let names: Vec<&str> = loaded.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["D", "B", "A", "C"]);
}

/// Reorder profile patches via the controller API.
#[tokio::test]
async fn reorder_profile_patches() {
    let signal = controller().await;

    let rig_id: RigId = guitar_megarig_id();
    let scene_id: RigSceneId = guitar_megarig_default_scene();

    let mut profile = Profile::new(
        seed_id("reorder-profile"),
        "Reorder Profile",
        Patch::from_rig_scene(seed_id("rp-a"), "A", rig_id.clone(), scene_id.clone()),
    );
    profile.add_patch(Patch::from_rig_scene(
        seed_id("rp-b"),
        "B",
        rig_id.clone(),
        scene_id.clone(),
    ));
    profile.add_patch(Patch::from_rig_scene(
        seed_id("rp-c"),
        "C",
        rig_id.clone(),
        scene_id.clone(),
    ));
    profile.add_patch(Patch::from_rig_scene(
        seed_id("rp-d"),
        "D",
        rig_id.clone(),
        scene_id.clone(),
    ));
    profile.add_patch(Patch::from_rig_scene(
        seed_id("rp-e"),
        "E",
        rig_id.clone(),
        scene_id.clone(),
    ));
    signal.profiles().save(profile).await.unwrap();

    signal
        .profiles()
        .reorder_patches(
            seed_id("reorder-profile"),
            &[
                seed_id("rp-e").into(),
                seed_id("rp-c").into(),
                seed_id("rp-a").into(),
                seed_id("rp-d").into(),
                seed_id("rp-b").into(),
            ],
        )
        .await
        .unwrap();

    let loaded = signal
        .profiles()
        .load(seed_id("reorder-profile"))
        .await
        .unwrap()
        .expect("profile");

    let names: Vec<&str> = loaded.patches.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["E", "C", "A", "D", "B"]);
}

/// Reorder song sections via the controller API.
#[tokio::test]
async fn reorder_song_sections() {
    let signal = controller().await;

    let rig_id: RigId = guitar_megarig_id();
    let scene_id: RigSceneId = guitar_megarig_default_scene();

    let mut song = Song::new(
        seed_id("reorder-song"),
        "Reorder Song",
        Section::from_rig_scene(
            seed_id("rs-verse"),
            "Verse",
            rig_id.clone(),
            scene_id.clone(),
        ),
    );
    song.add_section(Section::from_rig_scene(
        seed_id("rs-chorus"),
        "Chorus",
        rig_id.clone(),
        scene_id.clone(),
    ));
    song.add_section(Section::from_rig_scene(
        seed_id("rs-bridge"),
        "Bridge",
        rig_id.clone(),
        scene_id.clone(),
    ));
    song.add_section(Section::from_rig_scene(
        seed_id("rs-outro"),
        "Outro",
        rig_id.clone(),
        scene_id.clone(),
    ));
    signal.songs().save(song).await.unwrap();

    signal
        .songs()
        .reorder_sections(
            seed_id("reorder-song"),
            &[
                seed_id("rs-bridge").into(),
                seed_id("rs-verse").into(),
                seed_id("rs-outro").into(),
                seed_id("rs-chorus").into(),
            ],
        )
        .await
        .unwrap();

    let loaded = signal
        .songs()
        .load(seed_id("reorder-song"))
        .await
        .unwrap()
        .expect("song");

    let names: Vec<&str> = loaded.sections.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Bridge", "Verse", "Outro", "Chorus"]);
}

/// Reorder setlist entries via the controller API.
#[tokio::test]
async fn reorder_setlist_entries() {
    let signal = controller().await;

    // We need valid song IDs — use seeded songs
    let songs = signal.songs().list().await.unwrap();
    assert!(!songs.is_empty(), "need at least one seeded song");
    let song_id = songs[0].id.clone();

    let mut setlist = Setlist::new(
        seed_id("reorder-setlist"),
        "Reorder Setlist",
        SetlistEntry::new(seed_id("rse-1"), "Entry 1", song_id.clone()),
    );
    setlist.add_entry(SetlistEntry::new(
        seed_id("rse-2"),
        "Entry 2",
        song_id.clone(),
    ));
    setlist.add_entry(SetlistEntry::new(
        seed_id("rse-3"),
        "Entry 3",
        song_id.clone(),
    ));
    signal.setlists().save(setlist).await.unwrap();

    signal
        .setlists()
        .reorder_entries(
            seed_id("reorder-setlist"),
            &[
                seed_id("rse-3").into(),
                seed_id("rse-1").into(),
                seed_id("rse-2").into(),
            ],
        )
        .await
        .unwrap();

    let loaded = signal
        .setlists()
        .load(seed_id("reorder-setlist"))
        .await
        .unwrap()
        .expect("setlist");

    let names: Vec<&str> = loaded.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Entry 3", "Entry 1", "Entry 2"]);
}

// ═════════════════════════════════════════════════════════════
//  Group F: Profile + Patch CRUD
// ═════════════════════════════════════════════════════════════

/// Create a profile with multiple patches targeting different rig scenes.
#[tokio::test]
async fn create_profile_with_patches() {
    let signal = controller().await;

    let rig_id = guitar_megarig_id();
    let default_scene = guitar_megarig_default_scene();
    let lead_scene = guitar_megarig_lead_scene();

    let mut profile = Profile::new(
        seed_id("custom-profile"),
        "Custom Guitar Profile",
        Patch::from_rig_scene(
            seed_id("cp-clean"),
            "Clean",
            rig_id.clone(),
            default_scene.clone(),
        ),
    );
    profile.add_patch(Patch::from_rig_scene(
        seed_id("cp-crunch"),
        "Crunch",
        rig_id.clone(),
        default_scene.clone(),
    ));
    profile.add_patch(Patch::from_rig_scene(
        seed_id("cp-lead"),
        "Lead",
        rig_id.clone(),
        lead_scene.clone(),
    ));
    profile.add_patch(Patch::from_rig_scene(
        seed_id("cp-solo"),
        "Solo",
        rig_id.clone(),
        lead_scene.clone(),
    ));
    profile.add_patch(Patch::from_rig_scene(
        seed_id("cp-ambient"),
        "Ambient",
        rig_id.clone(),
        default_scene.clone(),
    ));
    profile.add_patch(Patch::from_rig_scene(
        seed_id("cp-rhythmic"),
        "Rhythmic",
        rig_id.clone(),
        default_scene.clone(),
    ));
    signal.profiles().save(profile).await.unwrap();

    let loaded = signal
        .profiles()
        .load(seed_id("custom-profile"))
        .await
        .unwrap()
        .expect("profile");

    assert_eq!(loaded.name, "Custom Guitar Profile");
    assert_eq!(loaded.patches.len(), 6);
    assert_eq!(
        loaded.default_patch_id.to_string(),
        seed_id("cp-clean").to_string()
    );
}

/// Load a specific patch and verify it targets the correct rig scene.
#[tokio::test]
async fn patch_targets_correct_rig_scene() {
    let signal = controller().await;

    // Use seeded Worship profile
    let profiles = signal.profiles().list().await.unwrap();
    let worship = profiles
        .iter()
        .find(|p| p.name == "Worship")
        .expect("worship profile");

    let lead_patch = worship
        .patches
        .iter()
        .find(|p| p.name == "Lead")
        .expect("lead patch");

    let loaded = signal
        .profiles()
        .load_patch(worship.id.clone(), lead_patch.id.clone())
        .await
        .unwrap()
        .expect("lead patch variant");

    assert_eq!(loaded.name, "Lead");
    match &loaded.target {
        PatchTarget::RigScene { rig_id, .. } => {
            assert_eq!(rig_id.to_string(), guitar_megarig_id().to_string());
        }
        _ => panic!("expected RigScene target"),
    }
}

/// Retarget a patch to a different rig scene via set_patch_preset.
#[tokio::test]
async fn retarget_patch_via_set_patch_preset() {
    let signal = controller().await;

    let rig_id = guitar_megarig_id();
    let scene_a = guitar_megarig_default_scene();
    let scene_b = guitar_megarig_lead_scene();

    let profile = Profile::new(
        seed_id("retarget-profile"),
        "Retarget Profile",
        Patch::from_rig_scene(
            seed_id("retarget-patch"),
            "Target",
            rig_id.clone(),
            scene_a.clone(),
        ),
    );
    signal.profiles().save(profile).await.unwrap();

    // Verify initial target
    let loaded_before = signal
        .profiles()
        .load(seed_id("retarget-profile"))
        .await
        .unwrap()
        .expect("profile");
    match &loaded_before.patches[0].target {
        PatchTarget::RigScene { scene_id, .. } => assert_eq!(*scene_id, scene_a),
        _ => panic!("expected RigScene target"),
    }

    // Retarget
    signal
        .profiles()
        .set_patch_preset(
            seed_id("retarget-profile"),
            seed_id("retarget-patch"),
            rig_id.clone(),
            scene_b.clone(),
        )
        .await
        .unwrap();

    let loaded_after = signal
        .profiles()
        .load(seed_id("retarget-profile"))
        .await
        .unwrap()
        .expect("profile after retarget");
    match &loaded_after.patches[0].target {
        PatchTarget::RigScene { scene_id, .. } => assert_eq!(*scene_id, scene_b),
        _ => panic!("expected RigScene target"),
    }
}

/// Patch overrides affect resolved parameter values.
#[tokio::test]
async fn patch_overrides_affect_resolved_values() {
    let signal = controller().await;

    // The seeded worship profile has patches with overrides
    let profiles = signal.profiles().list().await.unwrap();
    let worship = profiles
        .iter()
        .find(|p| p.name == "Worship")
        .expect("worship profile");

    // Find the Clean patch (known to have gain override ≈ 0.18)
    let clean_patch = worship
        .patches
        .iter()
        .find(|p| p.name == "Clean")
        .expect("clean patch");

    let graph = signal
        .resolve_target(ResolveTarget::ProfilePatch {
            profile_id: worship.id.clone(),
            patch_id: clean_patch.id.clone(),
        })
        .await
        .expect("resolve should succeed");

    assert!(
        !graph.engines.is_empty(),
        "resolved graph should have engines"
    );

    // Find the amp gain param
    if let Some(gain) = graph.find_param("amp", "gain") {
        // Clean patch overrides gain to be low
        assert!(
            gain < 0.3,
            "clean patch gain should be low (overridden), got {gain}"
        );
    }
}

// ═════════════════════════════════════════════════════════════
//  Group G: Song + Section CRUD
// ═════════════════════════════════════════════════════════════

/// Create a song with sections from both patches and rig scenes.
#[tokio::test]
async fn create_song_with_mixed_sections() {
    let signal = controller().await;

    let rig_id = guitar_megarig_id();
    let scene_id = guitar_megarig_default_scene();

    // First create a profile with a patch to reference
    let profile = Profile::new(
        seed_id("song-profile"),
        "Song Profile",
        Patch::from_rig_scene(
            seed_id("song-patch"),
            "Song Patch",
            rig_id.clone(),
            scene_id.clone(),
        ),
    );
    signal.profiles().save(profile).await.unwrap();

    let mut song = Song::new(
        seed_id("mixed-song"),
        "Mixed Song",
        Section::from_patch(seed_id("ms-verse"), "Verse", seed_id("song-patch")),
    );
    song.add_section(Section::from_rig_scene(
        seed_id("ms-chorus"),
        "Chorus",
        rig_id.clone(),
        guitar_megarig_lead_scene(),
    ));
    song.add_section(Section::from_patch(
        seed_id("ms-bridge"),
        "Bridge",
        seed_id("song-patch"),
    ));
    signal.songs().save(song).await.unwrap();

    let loaded = signal
        .songs()
        .load(seed_id("mixed-song"))
        .await
        .unwrap()
        .expect("song");

    assert_eq!(loaded.sections.len(), 3);
    assert!(matches!(
        loaded.sections[0].source,
        SectionSource::Patch { .. }
    ));
    assert!(matches!(
        loaded.sections[1].source,
        SectionSource::RigScene { .. }
    ));
    assert!(matches!(
        loaded.sections[2].source,
        SectionSource::Patch { .. }
    ));
}

/// Switch a section's source from patch to rig scene.
#[tokio::test]
async fn switch_section_source() {
    let signal = controller().await;

    let rig_id = guitar_megarig_id();
    let scene_id = guitar_megarig_default_scene();

    let song = Song::new(
        seed_id("switch-song"),
        "Switch Song",
        Section::from_patch(
            seed_id("switch-section"),
            "Switchable",
            seed_id("song-patch"),
        ),
    );
    signal.songs().save(song).await.unwrap();

    // Verify initial source
    let before = signal
        .songs()
        .load(seed_id("switch-song"))
        .await
        .unwrap()
        .expect("song");
    assert!(matches!(
        before.sections[0].source,
        SectionSource::Patch { .. }
    ));

    // Switch to rig scene
    signal
        .songs()
        .set_section_source(
            seed_id("switch-song"),
            seed_id("switch-section"),
            SectionSource::RigScene {
                rig_id: rig_id.clone(),
                scene_id: scene_id.clone(),
            },
        )
        .await
        .unwrap();

    let after = signal
        .songs()
        .load(seed_id("switch-song"))
        .await
        .unwrap()
        .expect("song");
    assert!(matches!(
        after.sections[0].source,
        SectionSource::RigScene { .. }
    ));
}

/// Resolve all sections in a song.
#[tokio::test]
async fn resolve_all_song_sections() {
    let signal = controller().await;

    // Use seeded songs
    let songs = signal.songs().list().await.unwrap();
    assert!(!songs.is_empty(), "need seeded songs");

    for song in &songs {
        for section in &song.sections {
            let result = signal
                .resolve_target(ResolveTarget::SongSection {
                    song_id: song.id.clone(),
                    section_id: section.id.clone(),
                })
                .await;
            assert!(
                result.is_ok(),
                "failed to resolve section '{}' in song '{}': {:?}",
                section.name,
                song.name,
                result.err()
            );
            let graph = result.unwrap();
            assert!(
                !graph.engines.is_empty(),
                "resolved graph for '{}' should have engines",
                section.name
            );
        }
    }
}

// ═════════════════════════════════════════════════════════════
//  Group H: Setlist Assembly
// ═════════════════════════════════════════════════════════════

/// Create a setlist from songs.
#[tokio::test]
async fn create_setlist_from_songs() {
    let signal = controller().await;

    let songs = signal.songs().list().await.unwrap();
    assert!(!songs.is_empty());
    let song_id = songs[0].id.clone();

    let mut setlist = Setlist::new(
        seed_id("test-setlist"),
        "Test Setlist",
        SetlistEntry::new(seed_id("tse-1"), "Song 1", song_id.clone()),
    );
    setlist.add_entry(SetlistEntry::new(
        seed_id("tse-2"),
        "Song 2",
        song_id.clone(),
    ));
    signal.setlists().save(setlist).await.unwrap();

    let loaded = signal
        .setlists()
        .load(seed_id("test-setlist"))
        .await
        .unwrap()
        .expect("setlist");

    assert_eq!(loaded.entries.len(), 2);
    assert_eq!(loaded.entries[0].name, "Song 1");
    assert_eq!(loaded.entries[1].name, "Song 2");
}

/// Full hierarchy resolve sweep: setlist → songs → sections.
#[tokio::test]
async fn full_hierarchy_resolve_sweep() {
    let signal = controller().await;

    let setlists = signal.setlists().list().await.unwrap();
    assert!(!setlists.is_empty(), "need seeded setlists");

    let mut total_resolved = 0;
    for setlist in &setlists {
        for entry in &setlist.entries {
            let song = signal.songs().load(entry.song_id.clone()).await.unwrap();
            if let Some(song) = song {
                for section in &song.sections {
                    let result = signal
                        .resolve_target(ResolveTarget::SongSection {
                            song_id: song.id.clone(),
                            section_id: section.id.clone(),
                        })
                        .await;
                    assert!(
                        result.is_ok(),
                        "failed to resolve section '{}': {:?}",
                        section.name,
                        result.err()
                    );
                    total_resolved += 1;
                }
            }
        }
    }
    assert!(
        total_resolved > 0,
        "should have resolved at least one section"
    );
}

// ═════════════════════════════════════════════════════════════
//  Group I: Smart Scene Switching (SlotDiff)
// ═════════════════════════════════════════════════════════════

/// Same preset + same snapshot → NoChange.
#[tokio::test]
async fn scene_switch_same_snapshot_no_change() {
    let preset_id = ModulePresetId::new();
    let snapshot_id = ModuleSnapshotId::new();

    let target = ModuleTarget {
        module_type: ModuleType::Amp,
        module_preset_id: preset_id.clone(),
        module_snapshot_id: Some(snapshot_id.clone()),
    };

    let slot = SlotState {
        module_type: ModuleType::Amp,
        current: Some(ResolvedSlot::Active(target.clone())),
        active_handle: Some(InstanceHandle::new(1)),
        is_disabled: false,
    };

    let mut targets = std::collections::HashMap::new();
    targets.insert(ModuleType::Amp, ResolvedSlot::Active(target));

    let diffs = compute_diff(&[slot], &targets, &|_| None);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], SlotDiff::NoChange { .. }));
}

/// Same preset, different snapshot → ApplySnapshot (params only, no reload).
#[tokio::test]
async fn scene_switch_different_snapshot_params_only() {
    let preset_id = ModulePresetId::new();
    let snap_a = ModuleSnapshotId::new();
    let snap_b = ModuleSnapshotId::new();

    let current_target = ModuleTarget {
        module_type: ModuleType::Amp,
        module_preset_id: preset_id.clone(),
        module_snapshot_id: Some(snap_a),
    };
    let new_target = ModuleTarget {
        module_type: ModuleType::Amp,
        module_preset_id: preset_id.clone(),
        module_snapshot_id: Some(snap_b),
    };

    let slot = SlotState {
        module_type: ModuleType::Amp,
        current: Some(ResolvedSlot::Active(current_target)),
        active_handle: Some(InstanceHandle::new(1)),
        is_disabled: false,
    };

    let mut targets = std::collections::HashMap::new();
    targets.insert(ModuleType::Amp, ResolvedSlot::Active(new_target));

    let diffs = compute_diff(&[slot], &targets, &|_| None);
    assert_eq!(diffs.len(), 1);
    assert!(
        matches!(diffs[0], SlotDiff::ApplySnapshot { .. }),
        "same preset + different snapshot should be ApplySnapshot, got {:?}",
        diffs[0]
    );
}

/// Different preset → LoadAndActivate (full load).
#[tokio::test]
async fn scene_switch_different_preset_full_load() {
    let preset_a = ModulePresetId::new();
    let preset_b = ModulePresetId::new();

    let current_target = ModuleTarget {
        module_type: ModuleType::Amp,
        module_preset_id: preset_a,
        module_snapshot_id: None,
    };
    let new_target = ModuleTarget {
        module_type: ModuleType::Amp,
        module_preset_id: preset_b,
        module_snapshot_id: None,
    };

    let slot = SlotState {
        module_type: ModuleType::Amp,
        current: Some(ResolvedSlot::Active(current_target)),
        active_handle: Some(InstanceHandle::new(1)),
        is_disabled: false,
    };

    let mut targets = std::collections::HashMap::new();
    targets.insert(ModuleType::Amp, ResolvedSlot::Active(new_target));

    let diffs = compute_diff(&[slot], &targets, &|_| None);
    assert_eq!(diffs.len(), 1);
    assert!(
        matches!(diffs[0], SlotDiff::LoadAndActivate { .. }),
        "different preset should be LoadAndActivate, got {:?}",
        diffs[0]
    );
}

// ═════════════════════════════════════════════════════════════
//  Group J: Morph Engine Integration
// ═════════════════════════════════════════════════════════════

/// Morph between two domain snapshots at t=0.5 produces midpoint values.
#[tokio::test]
async fn morph_between_resolved_graphs() {
    let snap_a = DawParameterSnapshot::new(vec![
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 0,
            param_name: "gain".into(),
            value: 0.2,
        },
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 1,
            param_name: "tone".into(),
            value: 0.4,
        },
        DawParamValue {
            fx_id: "fx2".into(),
            param_index: 0,
            param_name: "decay".into(),
            value: 0.1,
        },
    ]);
    let snap_b = DawParameterSnapshot::new(vec![
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 0,
            param_name: "gain".into(),
            value: 0.8,
        },
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 1,
            param_name: "tone".into(),
            value: 0.6,
        },
        DawParamValue {
            fx_id: "fx2".into(),
            param_index: 0,
            param_name: "decay".into(),
            value: 0.9,
        },
    ]);

    let mut engine = MorphEngine::new();
    engine.set_a(snap_a);
    engine.set_b(snap_b);

    assert!(engine.is_ready());
    assert_eq!(engine.diff_count(), 3);

    let changes = engine.morph(0.5, signal::easing::EasingCurve::Linear);
    assert_eq!(changes.len(), 3);

    for change in &changes {
        let expected_mid = (change.from_value + change.to_value) / 2.0;
        assert!(
            (change.current_value - expected_mid).abs() < 1e-10,
            "param '{}' at t=0.5 should be midpoint {expected_mid}, got {}",
            change.param_name,
            change.current_value
        );
    }
}

/// Morph only counts params that actually differ.
#[tokio::test]
async fn morph_diff_only_counts_changed_params() {
    let snap_a = DawParameterSnapshot::new(vec![
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 0,
            param_name: "gain".into(),
            value: 0.5,
        },
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 1,
            param_name: "tone".into(),
            value: 0.5,
        },
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 2,
            param_name: "bass".into(),
            value: 0.3,
        },
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 3,
            param_name: "mid".into(),
            value: 0.4,
        },
    ]);
    let snap_b = DawParameterSnapshot::new(vec![
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 0,
            param_name: "gain".into(),
            value: 0.5,
        }, // same
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 1,
            param_name: "tone".into(),
            value: 0.5,
        }, // same
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 2,
            param_name: "bass".into(),
            value: 0.7,
        }, // different
        DawParamValue {
            fx_id: "fx1".into(),
            param_index: 3,
            param_name: "mid".into(),
            value: 0.9,
        }, // different
    ]);

    let mut engine = MorphEngine::new();
    engine.set_a(snap_a);
    engine.set_b(snap_b);

    assert_eq!(engine.diff_count(), 2, "only 2 params should differ");

    let changes = engine.morph(0.5, signal::easing::EasingCurve::Linear);
    assert_eq!(
        changes.len(),
        2,
        "morph should only produce changes for differing params"
    );
}
