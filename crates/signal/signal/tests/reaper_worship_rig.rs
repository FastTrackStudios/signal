//! REAPER integration test: Full worship guitar rig load + scene verification.
//!
//! Bootstraps in-memory signal controller, loads the worship guitar rig onto
//! REAPER tracks via `load_rig_to_daw`, and verifies the resulting structure:
//! track hierarchy, module FX loading, and scene override data.
//!
//! Run with:
//!   cargo xtask reaper-test worship_rig

use std::time::Duration;

use reaper_test::reaper_test;
use signal::rig::RigSceneId;
use signal::seed_id;
use signal_live::daw_rig_builder::instantiate_rig;
use signal_proto::overrides::NodeOverrideOp;
use signal_proto::rig_template::{EngineTemplate, LayerTemplate, RigTemplate};

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
// Test: Worship rig track structure — [R]/[E]/[L] hierarchy via instantiate_rig
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn worship_rig_track_structure(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;
    settle().await;

    // Build the worship rig template: 1 engine with 2 layers (matching seed data).
    let template = RigTemplate {
        name: "Worship Rig".to_string(),
        engines: vec![EngineTemplate {
            name: "Guitar Engine".to_string(),
            layers: vec![
                LayerTemplate {
                    name: "Guitar Main".to_string(),
                },
                LayerTemplate {
                    name: "Archetype JM".to_string(),
                },
            ],
            fx_sends: vec![],
        }],
        fx_sends: vec![],
    };

    let instance = instantiate_rig(&template, &project).await?;
    settle().await;

    // ── Verify track hierarchy ──────────────────────────────────────
    let tracks = project.tracks().all().await?;

    // 1 rig + 1 engine + 2 layers = 4 tracks
    assert_eq!(
        tracks.len(),
        4,
        "worship rig should have 4 tracks (rig+engine+2 layers), got {}",
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
    assert!(
        tracks[2].name.starts_with("[L]"),
        "layer 0 should have [L] prefix, got '{}'",
        tracks[2].name
    );
    assert!(
        tracks[2].name.contains("Guitar Main"),
        "layer 0 should contain 'Guitar Main', got '{}'",
        tracks[2].name
    );
    assert!(
        tracks[3].name.starts_with("[L]"),
        "layer 1 should have [L] prefix, got '{}'",
        tracks[3].name
    );
    assert!(
        tracks[3].name.contains("Archetype JM"),
        "layer 1 should contain 'Archetype JM', got '{}'",
        tracks[3].name
    );

    // ── Verify instance structure ───────────────────────────────────
    assert_eq!(instance.engine_instances.len(), 1, "should have 1 engine");
    assert_eq!(
        instance.engine_instances[0].layer_tracks.len(),
        2,
        "engine should have 2 layer tracks"
    );
    assert!(
        instance.fx_send_tracks.is_empty(),
        "worship rig has no rig-level FX sends"
    );

    ctx.log("worship_rig_track_structure: PASS");
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
        ("Drive", 2),      // parametric-od + klone
        ("Tone", 1),       // amp-eq
        ("Room", 1),       // amp-verb
        ("Delay Mix", 2),  // dly-1 + dly-2
        ("Reverb Mix", 2), // verb-1 + verb-2
        ("Mod Depth", 1),  // chorus
        ("Trem Rate", 1),  // tremolo
        ("Master Vol", 1), // master-trim
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

// ---------------------------------------------------------------------------
// Diagnostic: Dump all installed FX plugins (temporary — remove after use)
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn dump_installed_plugins(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let plugins = ctx.daw.installed_plugins().await?;
    ctx.log(&format!(
        "=== Installed FX plugins: {} total ===",
        plugins.len()
    ));
    for p in &plugins {
        ctx.log(&format!("  {} | {}", p.name, p.ident));
    }
    Ok(())
}
