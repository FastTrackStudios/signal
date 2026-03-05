//! REAPER integration tests: Rig structure template instantiation.
//!
//! Tests that `RigTemplate` and `RackTemplate` produce correct REAPER track
//! hierarchies with proper folder depth, naming prefixes, and send routing.
//!
//! Run with:
//!   cargo xtask reaper-test rig_structure

use std::time::Duration;

use reaper_test::reaper_test;
use signal_live::daw_rig_builder::{instantiate_rack, instantiate_rig};
use signal_proto::rig_template::{RackTemplate, RigTemplate};

/// Small sleep to let REAPER process track changes.
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
// Test: Guitar rig — minimal structure (1 engine, 1 layer, no FX sends)
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn signal_guitar_rig_structure(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;
    settle().await;

    let template = RigTemplate::guitar_rig();
    let instance = instantiate_rig(&template, &project).await?;
    settle().await;

    // Verify track count: 1 rig + 1 engine + 1 layer = 3
    let tracks = project.tracks().all().await?;
    assert_eq!(tracks.len(), 3, "guitar rig should have 3 tracks, got {}", tracks.len());

    // Verify rig track has [R] prefix
    assert!(
        tracks[0].name.starts_with("[R]"),
        "rig track should have [R] prefix, got '{}'",
        tracks[0].name
    );

    // Verify engine track has [E] prefix
    assert!(
        tracks[1].name.starts_with("[E]"),
        "engine track should have [E] prefix, got '{}'",
        tracks[1].name
    );

    // Verify layer track has [L] prefix
    assert!(
        tracks[2].name.starts_with("[L]"),
        "layer track should have [L] prefix, got '{}'",
        tracks[2].name
    );

    // Verify the instance structure
    assert_eq!(instance.engine_instances.len(), 1);
    assert_eq!(instance.engine_instances[0].layer_tracks.len(), 1);
    assert!(instance.fx_send_tracks.is_empty());

    ctx.log("signal_guitar_rig_structure: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Keys megarig — 3 engines with layers and FX sends
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn signal_keys_megarig_structure(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;
    settle().await;

    let template = RigTemplate::keys_megarig();
    let instance = instantiate_rig(&template, &project).await?;
    settle().await;

    let tracks = project.tracks().all().await?;

    // Expected: 1 rig + 3 engines + 7 layers + 3 engine send folders + 6 engine sends
    //         + 1 rig send folder + 2 rig sends = 23
    assert_eq!(tracks.len(), 23, "keys megarig should have 23 tracks, got {}", tracks.len());

    // Verify [R] prefix on rig track
    assert!(
        tracks[0].name.starts_with("[R]"),
        "rig track should have [R] prefix, got '{}'",
        tracks[0].name
    );
    assert!(tracks[0].name.contains("Keys Rig"));

    // Verify instance structure
    assert_eq!(instance.engine_instances.len(), 3, "should have 3 engines");

    // Keys Engine: 2 layers, 2 FX sends
    let keys = &instance.engine_instances[0];
    assert_eq!(keys.layer_tracks.len(), 2, "Keys engine should have 2 layers");
    assert_eq!(keys.fx_send_tracks.len(), 2, "Keys engine should have 2 FX sends");

    // Synth Engine: 3 layers, 2 FX sends
    let synth = &instance.engine_instances[1];
    assert_eq!(synth.layer_tracks.len(), 3, "Synth engine should have 3 layers");
    assert_eq!(synth.fx_send_tracks.len(), 2, "Synth engine should have 2 FX sends");

    // Organ Engine: 2 layers, 2 FX sends
    let organ = &instance.engine_instances[2];
    assert_eq!(organ.layer_tracks.len(), 2, "Organ engine should have 2 layers");
    assert_eq!(organ.fx_send_tracks.len(), 2, "Organ engine should have 2 FX sends");

    // Rig-level FX sends
    assert_eq!(instance.fx_send_tracks.len(), 2, "rig should have 2 FX sends");

    // Verify engine tracks have [E] prefix
    for engine_inst in &instance.engine_instances {
        let info = engine_inst.engine_track.info().await?;
        assert!(
            info.name.starts_with("[E]"),
            "engine track should have [E] prefix, got '{}'",
            info.name
        );
    }

    // Verify layer tracks have [L] prefix
    for engine_inst in &instance.engine_instances {
        for layer_track in &engine_inst.layer_tracks {
            let info = layer_track.info().await?;
            assert!(
                info.name.starts_with("[L]"),
                "layer track should have [L] prefix, got '{}'",
                info.name
            );
        }
    }

    // Verify send routing: each layer should have sends to its engine's FX send tracks
    for engine_inst in &instance.engine_instances {
        if engine_inst.fx_send_tracks.is_empty() {
            continue;
        }
        for layer_track in &engine_inst.layer_tracks {
            let sends = layer_track.sends().all().await?;
            assert_eq!(
                sends.len(),
                engine_inst.fx_send_tracks.len(),
                "each layer should have {} sends, got {}",
                engine_inst.fx_send_tracks.len(),
                sends.len()
            );
        }
    }

    ctx.log("signal_keys_megarig_structure: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Vocal rack — 3 rigs with shared AUX/TIME send groups
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn signal_vocal_rack_rig_structure(ctx: &ReaperTestContext) -> eyre::Result<()> {
    ensure_audio(ctx).await;
    let project = ctx.project().clone();
    project.tracks().remove_all().await?;
    settle().await;

    let template = RackTemplate::vocal_rack();
    let instance = instantiate_rack(&template, &project).await?;
    settle().await;

    let tracks = project.tracks().all().await?;

    // Verify input tracks
    assert_eq!(instance.input_tracks.len(), 3, "should have 3 input tracks");

    // Verify rig instances
    assert_eq!(instance.rig_instances.len(), 3, "should have 3 vocal rigs");

    // Each vocal rig: 1 engine with 1 layer and 4 engine-level FX sends
    for (i, rig_inst) in instance.rig_instances.iter().enumerate() {
        let rig_info = rig_inst.rig_track.info().await?;
        assert!(
            rig_info.name.contains(&format!("Vocal {}", i + 1)),
            "rig {} should contain 'Vocal {}', got '{}'",
            i + 1,
            i + 1,
            rig_info.name
        );

        assert_eq!(
            rig_inst.engine_instances.len(),
            1,
            "vocal rig {} should have 1 engine",
            i + 1
        );

        let engine = &rig_inst.engine_instances[0];
        assert_eq!(
            engine.layer_tracks.len(),
            1,
            "vocal engine {} should have 1 layer",
            i + 1
        );
        assert_eq!(
            engine.fx_send_tracks.len(),
            4,
            "vocal engine {} should have 4 FX sends",
            i + 1
        );

        // Verify layer → send routing
        let layer_sends = engine.layer_tracks[0].sends().all().await?;
        assert_eq!(
            layer_sends.len(),
            4,
            "vocal layer {} should have 4 sends, got {}",
            i + 1,
            layer_sends.len()
        );
    }

    // Verify rack-level FX send groups (AUX + TIME)
    assert_eq!(
        instance.fx_send_group_tracks.len(),
        2,
        "should have 2 FX send groups (AUX + TIME)"
    );
    assert_eq!(
        instance.fx_send_group_tracks[0].len(),
        4,
        "AUX group should have 4 send tracks"
    );
    assert_eq!(
        instance.fx_send_group_tracks[1].len(),
        4,
        "TIME group should have 4 send tracks"
    );

    // Verify total track count:
    // 1 rack + 3 inputs
    // + 3 × (1 rig + 1 engine + 1 layer + 1 sends folder + 4 sends) = 3 × 8 = 24
    // + 2 send groups × (1 folder + 4 sends) = 10
    // Total: 1 + 3 + 24 + 10 = 38
    // Wait — vocal rigs have no rig-level sends, just engine-level.
    // Per rig: 1 rig_track + 1 engine + 1 layer + 1 engine_sends_folder + 4 sends = 8
    // 3 rigs = 24, + 1 rack + 3 inputs + 2 group folders + 8 group sends = 38
    let expected_count = 1 + 3 + 3 * 8 + 2 + 8;
    assert_eq!(
        tracks.len(),
        expected_count,
        "vocal rack should have {} tracks, got {}",
        expected_count,
        tracks.len()
    );

    ctx.log("signal_vocal_rack_rig_structure: PASS");
    Ok(())
}
