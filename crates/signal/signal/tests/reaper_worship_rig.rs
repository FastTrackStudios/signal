//! REAPER integration test: Full worship guitar rig load + scene verification.
//!
//! Bootstraps in-memory signal controller, loads the worship guitar rig onto
//! REAPER tracks via `load_rig_to_daw`, and verifies the resulting structure:
//! track hierarchy, module FX loading, and scene override data.
//!
//! Run with:
//!   cargo xtask reaper-test worship_rig

use std::time::{Duration, Instant};

use reaper_test::reaper_test;
use signal::seed_id;
use signal::rig::RigSceneId;
use signal_proto::overrides::NodeOverrideOp;

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

// ---------------------------------------------------------------------------
// Test: Load worship guitar rig — full track hierarchy + module FX
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn worship_rig_load_and_verify(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;
    settle().await;

    // Bootstrap in-memory signal controller with all seed data.
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let svc = signal.service();

    // Load the worship guitar rig from seed data.
    let rig = signal
        .rigs()
        .load(seed_id("worship-guitar-rig"))
        .await?
        .ok_or_else(|| eyre::eyre!("worship-guitar-rig not found in seed data"))?;

    ctx.log(&format!("Loaded rig '{}' with {} scenes", rig.name, rig.variants.len()));

    // ── Phase 1: Load rig to DAW ────────────────────────────────────
    let start = Instant::now();
    let result = svc
        .load_rig_to_daw(&rig, None, &project)
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    let load_time = start.elapsed();
    settle().await;

    ctx.log(&format!("Rig loaded in {:.2}s", load_time.as_secs_f64()));

    // ── Phase 2: Verify track hierarchy ─────────────────────────────
    let tracks = project.tracks().all().await?;

    // Worship rig: 1 rig + 1 engine + 2 layers = 4 tracks
    assert!(
        tracks.len() >= 4,
        "worship rig should have at least 4 tracks (rig+engine+2 layers), got {}",
        tracks.len()
    );

    // Rig track: [R] prefix
    assert!(
        tracks[0].name.starts_with("[R]"),
        "rig track should have [R] prefix, got '{}'",
        tracks[0].name
    );
    assert!(
        tracks[0].name.contains("Worship"),
        "rig track should contain 'Worship', got '{}'",
        tracks[0].name
    );

    // Engine track: [E] prefix
    assert!(
        tracks[1].name.starts_with("[E]"),
        "engine track should have [E] prefix, got '{}'",
        tracks[1].name
    );
    assert!(
        tracks[1].name.contains("Guitar"),
        "engine track should contain 'Guitar', got '{}'",
        tracks[1].name
    );

    // Layer tracks: [L] prefix
    for i in 2..tracks.len().min(4) {
        assert!(
            tracks[i].name.starts_with("[L]"),
            "layer track {} should have [L] prefix, got '{}'",
            i,
            tracks[i].name
        );
    }

    // ── Phase 3: Verify rig instance structure ──────────────────────
    assert_eq!(
        result.rig_instance.engine_instances.len(),
        1,
        "worship rig should have 1 engine"
    );

    let engine_inst = &result.rig_instance.engine_instances[0];
    assert_eq!(
        engine_inst.layer_tracks.len(),
        2,
        "guitar engine should have 2 layer tracks"
    );

    // ── Phase 4: Verify modules loaded on each layer ────────────────
    assert_eq!(
        result.layer_results.len(),
        2,
        "should have layer results for 2 layers"
    );

    // Layer 0 (Guitar Main): 11 modules
    let main_layer = &result.layer_results[0];
    ctx.log(&format!(
        "Guitar Main layer: {} modules loaded",
        main_layer.modules.len()
    ));
    assert_eq!(
        main_layer.modules.len(),
        11,
        "Guitar Main should have 11 modules, got {}",
        main_layer.modules.len()
    );

    // Layer 1 (Archetype JM): 6 modules
    let jm_layer = &result.layer_results[1];
    ctx.log(&format!(
        "Archetype JM layer: {} modules loaded",
        jm_layer.modules.len()
    ));
    assert_eq!(
        jm_layer.modules.len(),
        6,
        "Archetype JM should have 6 modules, got {}",
        jm_layer.modules.len()
    );

    // ── Phase 5: Verify FX exist on layer tracks ────────────────────
    for (idx, layer_track) in engine_inst.layer_tracks.iter().enumerate() {
        let fx_count = layer_track.fx_chain().count().await?;
        assert!(
            fx_count > 0,
            "layer track {} should have FX loaded, got 0",
            idx
        );
        ctx.log(&format!("Layer track {idx}: {fx_count} FX instances"));
    }

    // ── Phase 6: Verify FX tree has module containers ───────────────
    let main_track = &engine_inst.layer_tracks[0];
    let tree = main_track.fx_chain().tree().await?;
    assert!(
        tree.nodes.len() >= 11,
        "Guitar Main FX tree should have at least 11 top-level nodes (modules), got {}",
        tree.nodes.len()
    );

    // Check that module containers have [M] prefix
    for node in &tree.nodes {
        match &node.kind {
            daw_control::FxNodeKind::Container { name, .. } => {
                assert!(
                    name.contains("[M]"),
                    "module container should have [M] prefix, got '{name}'"
                );
            }
            daw_control::FxNodeKind::Plugin(fx) => {
                // Standalone blocks are [B] prefixed
                ctx.log(&format!("Standalone FX: {}", fx.name));
            }
        }
    }

    ctx.log(&format!(
        "worship_rig_load_and_verify: PASS (load: {:.2}s)",
        load_time.as_secs_f64()
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Worship rig scene data — verify Dry/Ambient override structure
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn worship_rig_scene_overrides(ctx: &ReaperTestContext) -> eyre::Result<()> {
    // Bootstrap controller and load rig from seed data.
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let rig = signal
        .rigs()
        .load(seed_id("worship-guitar-rig"))
        .await?
        .ok_or_else(|| eyre::eyre!("worship-guitar-rig not found"))?;

    // Verify 3 scenes: Default, Dry, Ambient
    assert_eq!(
        rig.variants.len(),
        3,
        "worship rig should have 3 scenes (Default + Dry + Ambient), got {}",
        rig.variants.len()
    );

    // ── Default scene ───────────────────────────────────────────────
    let default = rig
        .default_variant()
        .ok_or_else(|| eyre::eyre!("no default scene"))?;
    assert_eq!(default.name, "Default");
    assert_eq!(
        default.engine_selections.len(),
        1,
        "default scene should select 1 engine"
    );

    // ── Dry scene: modules bypassed ─────────────────────────────────
    let dry_id: RigSceneId = seed_id("worship-rig-dry").into();
    let dry = rig
        .variant(&dry_id)
        .ok_or_else(|| eyre::eyre!("Dry scene not found"))?;

    let bypass_paths: Vec<String> = dry
        .overrides
        .iter()
        .filter_map(|ov| match &ov.op {
            NodeOverrideOp::Bypass(true) => Some(ov.path.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        bypass_paths.iter().any(|p| p.contains("gtr-modulation")),
        "Dry scene must bypass gtr-modulation"
    );
    assert!(
        bypass_paths.iter().any(|p| p.contains("time-parallel")),
        "Dry scene must bypass time-parallel"
    );
    assert!(
        bypass_paths.iter().any(|p| p.contains("gtr-motion")),
        "Dry scene must bypass gtr-motion"
    );
    ctx.log(&format!(
        "Dry scene: {} bypass overrides verified",
        bypass_paths.len()
    ));

    // ── Ambient scene: parameter overrides, no bypasses ─────────────
    let ambient_id: RigSceneId = seed_id("worship-rig-ambient").into();
    let ambient = rig
        .variant(&ambient_id)
        .ok_or_else(|| eyre::eyre!("Ambient scene not found"))?;

    let ambient_bypasses: Vec<_> = ambient
        .overrides
        .iter()
        .filter(|ov| matches!(&ov.op, NodeOverrideOp::Bypass(true)))
        .collect();
    assert!(
        ambient_bypasses.is_empty(),
        "Ambient scene must not bypass any modules, found {} bypasses",
        ambient_bypasses.len()
    );

    let set_paths: Vec<String> = ambient
        .overrides
        .iter()
        .filter_map(|ov| match &ov.op {
            NodeOverrideOp::Set(_) => Some(ov.path.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        set_paths.iter().any(|p| p.contains("spring-reverb")),
        "Ambient must set spring-reverb mix"
    );
    assert!(
        set_paths.iter().any(|p| p.contains("dly-1")),
        "Ambient must set delay-1 mix"
    );
    assert!(
        set_paths.iter().any(|p| p.contains("verb-1")),
        "Ambient must set reverb-1 mix"
    );
    assert!(
        set_paths.iter().any(|p| p.contains("tremolo")),
        "Ambient must set tremolo parameters"
    );
    ctx.log(&format!(
        "Ambient scene: {} parameter overrides verified",
        set_paths.len()
    ));

    ctx.log("worship_rig_scene_overrides: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Worship rig macro bank — verify all 8 knobs + bindings
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn worship_rig_macro_bank(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let signal = signal::bootstrap_in_memory_controller_async().await?;
    let rig = signal
        .rigs()
        .load(seed_id("worship-guitar-rig"))
        .await?
        .ok_or_else(|| eyre::eyre!("worship-guitar-rig not found"))?;

    let bank = rig
        .macro_bank
        .as_ref()
        .ok_or_else(|| eyre::eyre!("worship rig should have a macro bank"))?;

    assert_eq!(
        bank.knobs.len(),
        8,
        "macro bank should have 8 knobs, got {}",
        bank.knobs.len()
    );

    // Verify knob names and binding counts
    let expected: &[(&str, usize)] = &[
        ("Drive", 2),       // parametric-od + klone
        ("Tone", 1),        // amp-eq
        ("Room", 1),        // amp-verb
        ("Delay Mix", 2),   // dly-1 + dly-2
        ("Reverb Mix", 2),  // verb-1 + verb-2
        ("Mod Depth", 1),   // chorus
        ("Trem Rate", 1),   // tremolo
        ("Master Vol", 1),  // master-trim
    ];

    for (i, (name, binding_count)) in expected.iter().enumerate() {
        let knob = &bank.knobs[i];
        assert_eq!(
            knob.label, *name,
            "knob {} should be '{}', got '{}'",
            i, name, knob.label
        );
        assert_eq!(
            knob.bindings.len(),
            *binding_count,
            "knob '{}' should have {} bindings, got {}",
            name,
            binding_count,
            knob.bindings.len()
        );
    }

    // Total binding count: 2+1+1+2+2+1+1+1 = 11
    let total_bindings: usize = bank.knobs.iter().map(|k| k.bindings.len()).sum();
    assert_eq!(total_bindings, 11, "total bindings should be 11");

    ctx.log("worship_rig_macro_bank: PASS");
    Ok(())
}
