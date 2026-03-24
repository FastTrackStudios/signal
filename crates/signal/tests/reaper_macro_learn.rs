//! REAPER integration test: Macro learn workflow.
//!
//! Tests the macro learn pipeline:
//!   1. Touch an FX parameter → verify `last_touched_fx()` detects it
//!   2. Arm a macro, set curve points via the learn state machine
//!   3. Disarm and verify the bindings are captured correctly
//!
//! Also tests direct FX parameter manipulation:
//!   - Set ReaComp parameters, read them back, verify round-trip
//!   - Multiple parameters on the same plugin
//!
//! Run with:
//!   cargo xtask reaper-test macro_learn

use std::time::Duration;

use reaper_test::reaper_test;

const REACOMP: &str = "VST: ReaComp (Cockos)";

async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

// ---------------------------------------------------------------------------
// Test: last_touched_fx detects parameter changes
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn last_touched_fx_detects_reacomp_parameter(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();

    ctx.log("=== LAST TOUCHED FX DETECTION TEST ===");

    // 1. Create a track with ReaComp
    let track = tracks.add("Last Touched Test", None).await?;
    let fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    // 2. Discover parameter indices
    let params = fx.parameters().await?;
    ctx.log(&format!(
        "ReaComp params: {}",
        params
            .iter()
            .take(8)
            .map(|p| format!("{}({})", p.name, p.index))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let threshold_idx = params
        .iter()
        .find(|p| p.name.to_lowercase().contains("thresh"))
        .map(|p| p.index)
        .ok_or_else(|| eyre::eyre!("Threshold param not found"))?;

    let ratio_idx = params
        .iter()
        .find(|p| p.name.to_lowercase().contains("ratio"))
        .map(|p| p.index)
        .ok_or_else(|| eyre::eyre!("Ratio param not found"))?;

    ctx.log(&format!(
        "Threshold idx={}, Ratio idx={}",
        threshold_idx, ratio_idx
    ));

    // 3. Touch the Threshold parameter by setting it
    ctx.log("Touching Threshold parameter...");
    fx.param(threshold_idx).set(0.3).await?;
    settle().await;

    // 4. Verify last_touched_fx returns the correct parameter
    let last_touched = ctx
        .daw
        .last_touched_fx()
        .await?
        .ok_or_else(|| eyre::eyre!("last_touched_fx returned None after touching Threshold"))?;

    let track_guid = track.guid().to_string();
    assert_eq!(
        last_touched.track_guid, track_guid,
        "Last touched should be on our track (got {} vs {})",
        last_touched.track_guid, track_guid
    );
    assert_eq!(
        last_touched.param_index, threshold_idx,
        "Last touched param should be Threshold (idx {}), got {}",
        threshold_idx, last_touched.param_index
    );
    assert!(!last_touched.is_input_fx, "Should be normal FX chain");

    ctx.log(&format!(
        "PASS: last_touched_fx detected Threshold (track={}, fx={}, param={})",
        last_touched.track_guid, last_touched.fx_index, last_touched.param_index
    ));

    // 5. Now touch Ratio and verify it updates
    ctx.log("Touching Ratio parameter...");
    fx.param(ratio_idx).set(0.6).await?;
    settle().await;

    let last_touched = ctx
        .daw
        .last_touched_fx()
        .await?
        .ok_or_else(|| eyre::eyre!("last_touched_fx returned None after touching Ratio"))?;

    assert_eq!(
        last_touched.param_index, ratio_idx,
        "Last touched param should now be Ratio (idx {}), got {}",
        ratio_idx, last_touched.param_index
    );

    ctx.log("PASS: last_touched_fx updated to Ratio after second touch");

    // 6. Verify the values actually stuck
    let thresh_val = fx.param(threshold_idx).get().await?;
    let ratio_val = fx.param(ratio_idx).get().await?;

    assert!(
        (thresh_val - 0.3).abs() < 0.05,
        "Threshold should be ~0.3, got {:.4}",
        thresh_val
    );
    assert!(
        (ratio_val - 0.6).abs() < 0.05,
        "Ratio should be ~0.6, got {:.4}",
        ratio_val
    );

    ctx.log(&format!(
        "PASS: Parameter values round-trip: Threshold={:.4}, Ratio={:.4}",
        thresh_val, ratio_val
    ));

    ctx.log("=== TEST PASSED: last_touched_fx detection works ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: last_touched_fx works across multiple tracks
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn last_touched_fx_across_tracks(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();

    ctx.log("=== LAST TOUCHED FX ACROSS TRACKS TEST ===");

    // Create two tracks with ReaComp on each
    let track1 = tracks.add("Track A", None).await?;
    let fx1 = track1.fx_chain().add(REACOMP).await?;
    settle().await;

    let track2 = tracks.add("Track B", None).await?;
    let fx2 = track2.fx_chain().add(REACOMP).await?;
    settle().await;

    // Touch param on track 1
    fx1.param(0).set(0.5).await?;
    settle().await;

    let lt = ctx.daw.last_touched_fx().await?.expect("should have last touched");
    assert_eq!(
        lt.track_guid,
        track1.guid().to_string(),
        "Should detect track A"
    );
    ctx.log("PASS: Detected touch on Track A");

    // Touch param on track 2
    fx2.param(1).set(0.7).await?;
    settle().await;

    let lt = ctx.daw.last_touched_fx().await?.expect("should have last touched");
    assert_eq!(
        lt.track_guid,
        track2.guid().to_string(),
        "Should detect track B"
    );
    assert_eq!(lt.param_index, 1, "Should detect param index 1");
    ctx.log("PASS: Detected touch on Track B, param 1");

    ctx.log("=== TEST PASSED: last_touched_fx works across tracks ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Set and read back multiple ReaComp parameters
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn reacomp_param_set_and_readback(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== REACOMP PARAMETER SET/READBACK TEST ===");

    let track = project.tracks().add("Param Readback", None).await?;
    let fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    let params = fx.parameters().await?;
    ctx.log(&format!("ReaComp has {} parameters", params.len()));

    // Set the first 5 parameters to known values
    let test_values = [0.1, 0.3, 0.5, 0.7, 0.9];
    let count = test_values.len().min(params.len());

    for (i, &val) in test_values.iter().enumerate().take(count) {
        fx.param(params[i].index).set(val).await?;
    }
    settle().await;

    // Read them all back and verify
    for (i, &expected) in test_values.iter().enumerate().take(count) {
        let actual = fx.param(params[i].index).get().await?;
        ctx.log(&format!(
            "  {} (idx {}): set {:.2}, got {:.4}",
            params[i].name, params[i].index, expected, actual
        ));
        assert!(
            (actual - expected).abs() < 0.05,
            "Param '{}' should be ~{:.2}, got {:.4}",
            params[i].name,
            expected,
            actual
        );
    }

    ctx.log("PASS: All parameters set and read back correctly");

    ctx.log("=== TEST PASSED: Parameter round-trip verified ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Load FTS Signal Controller CLAP onto a track
// ---------------------------------------------------------------------------

/// Names to try when loading the signal controller.
const SIGNAL_CONTROLLER_NAMES: &[&str] = &[
    "CLAP: FTS Signal Controller (FastTrackStudio)",
    "CLAP: FTS Signal Controller",
    "FTS Signal Controller",
];

#[reaper_test(isolated)]
async fn load_signal_controller_on_track(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Controller Load Test", None).await?;

    ctx.log("Trying to load FTS Signal Controller...");
    for name in SIGNAL_CONTROLLER_NAMES {
        ctx.log(&format!("  Trying: {name}"));
        match track.fx_chain().add(name).await {
            Ok(fx) => {
                let info = fx.info().await?;
                ctx.log(&format!("  Loaded as: {}", info.name));

                let params = fx.parameters().await?;
                ctx.log(&format!("  {} parameters", params.len()));
                for p in params.iter().take(8) {
                    ctx.log(&format!("    {} (idx {})", p.name, p.index));
                }

                let fx_count = track.fx_chain().count().await?;
                assert!(fx_count >= 1, "Track should have at least 1 FX");
                ctx.log("=== PASS: FTS Signal Controller loaded successfully ===");
                return Ok(());
            }
            Err(e) => {
                ctx.log(&format!("  Failed: {e}"));
            }
        }
    }

    ctx.log("=== FAIL: Could not load FTS Signal Controller with any name ===");
    Err(eyre::eyre!("FTS Signal Controller not found in REAPER's FX list"))
}

// ---------------------------------------------------------------------------
// Helper: write macro values to P_EXT (read by signal-controller's macro_timer)
// ---------------------------------------------------------------------------

/// Load signal controller on a track. Required for macro_timer to run.
async fn add_signal_controller(track: &daw::TrackHandle) -> eyre::Result<daw::FxHandle> {
    for name in SIGNAL_CONTROLLER_NAMES {
        if let Ok(fx) = track.fx_chain().add(name).await {
            return Ok(fx);
        }
    }
    Err(eyre::eyre!("FTS Signal Controller not available"))
}

/// Write macro knob values to track P_EXT for the macro_timer to read.
async fn set_macro_value(track: &daw::TrackHandle, macro_idx: usize, value: f64) -> eyre::Result<()> {
    // Read current values, update one, write back
    let current = track.get_ext_state("FTS_MACROS", "macro_values").await?;
    let mut values: Vec<f64> = match &current {
        Some(json) => serde_json::from_str(json).unwrap_or_else(|_| vec![0.0; 8]),
        None => vec![0.0; 8],
    };
    while values.len() < 8 {
        values.push(0.0);
    }
    values[macro_idx] = value;
    let json = serde_json::to_string(&values)?;
    track.set_ext_state("FTS_MACROS", "macro_values", &json).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Signal Controller macro_timer drives ReaComp via P_EXT
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_assign_to_reacomp_via_ext_state(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== MACRO ASSIGN VIA SIGNAL CONTROLLER TIMER ===");
    ctx.log("Signal Controller (FX 0) + ReaComp (FX 1), mapping via P_EXT");
    ctx.log("");

    // 1. Create track with Signal Controller (FX 0) + ReaComp (FX 1)
    let track = project.tracks().add("Macro Assign Test", None).await?;
    let _controller = add_signal_controller(&track).await?;
    ctx.log("FX 0: Signal Controller loaded");
    let target_fx = track.fx_chain().add(REACOMP).await?;
    ctx.log("FX 1: ReaComp loaded");

    // 2. Discover Threshold and Ratio param indices
    let params = target_fx.parameters().await?;
    let threshold_idx = params
        .iter()
        .find(|p| p.name.to_lowercase().contains("thresh"))
        .map(|p| p.index)
        .ok_or_else(|| eyre::eyre!("Threshold not found"))?;
    let ratio_idx = params
        .iter()
        .find(|p| p.name.to_lowercase().contains("ratio"))
        .map(|p| p.index)
        .ok_or_else(|| eyre::eyre!("Ratio not found"))?;

    ctx.log(&format!("ReaComp: Threshold={}, Ratio={}", threshold_idx, ratio_idx));

    // 3. Store mapping config — Macro 0 drives Threshold (inverted) and Ratio (direct)
    //    target_fx is ByIndex(1) since Signal Controller is FX 0, ReaComp is FX 1
    let mapping_json = serde_json::json!({
        "version": "0.1",
        "mappings": [
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": threshold_idx,
                "mode": {"ScaleRange": {"min": 0.8, "max": 0.2}}
            },
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": ratio_idx,
                "mode": {"ScaleRange": {"min": 0.1, "max": 0.9}}
            }
        ]
    })
    .to_string();

    track.set_ext_state("FTS_MACROS", "mapping_config", &mapping_json).await?;
    ctx.log("Stored mapping config in track P_EXT");

    // Wait for macro timer to discover this track (~1s config refresh)
    tokio::time::sleep(Duration::from_secs(3)).await;

    let poll_timeout = Duration::from_secs(8);
    let poll_interval = Duration::from_millis(50);

    // 4. Set Macro 0 = 0.0 → Threshold ~0.8, Ratio ~0.1
    ctx.log("");
    ctx.log("--- Macro 0 = 0.0 ---");
    set_macro_value(&track, 0, 0.0).await?;

    let thresh_at_0 = poll_param(&target_fx, threshold_idx, 0.8, 0.05, poll_timeout, poll_interval).await?;
    let ratio_at_0 = poll_param(&target_fx, ratio_idx, 0.1, 0.05, poll_timeout, poll_interval).await?;

    ctx.log(&format!("  Threshold={:.4}, Ratio={:.4}", thresh_at_0, ratio_at_0));
    ctx.log("PASS: Macro 0 = 0.0 → correct min values");

    // 5. Set Macro 0 = 1.0 → Threshold ~0.2, Ratio ~0.9
    ctx.log("");
    ctx.log("--- Macro 0 = 1.0 ---");
    set_macro_value(&track, 0, 1.0).await?;

    let thresh_at_1 = poll_param(&target_fx, threshold_idx, 0.2, 0.05, poll_timeout, poll_interval).await?;
    let ratio_at_1 = poll_param(&target_fx, ratio_idx, 0.9, 0.05, poll_timeout, poll_interval).await?;

    ctx.log(&format!("  Threshold={:.4}, Ratio={:.4}", thresh_at_1, ratio_at_1));
    ctx.log("PASS: Macro 0 = 1.0 → correct max values");

    ctx.log("");
    ctx.log("=== TEST PASSED: macro_timer drives ReaComp via P_EXT ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: 4-point stage macro — non-linear multi-point curve drives ReaComp
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn four_point_stage_macro(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== 4-POINT STAGE MACRO TEST ===");
    ctx.log("MultiPoint curve: Clean → Crunch → Drive → Lead");
    ctx.log("");

    // 1. Create track with Signal Controller (FX 0) + ReaComp (FX 1)
    let track = project.tracks().add("Stage Macro Test", None).await?;
    let _controller = add_signal_controller(&track).await?;
    ctx.log("FX 0: Signal Controller loaded");
    let target_fx = track.fx_chain().add(REACOMP).await?;
    ctx.log("FX 1: ReaComp loaded");

    // 2. Discover ReaComp params
    let params = target_fx.parameters().await?;
    let threshold_idx = params.iter().find(|p| p.name.to_lowercase().contains("thresh")).map(|p| p.index)
        .ok_or_else(|| eyre::eyre!("Threshold not found"))?;
    let ratio_idx = params.iter().find(|p| p.name.to_lowercase().contains("ratio")).map(|p| p.index)
        .ok_or_else(|| eyre::eyre!("Ratio not found"))?;

    ctx.log(&format!("ReaComp: Threshold={}, Ratio={}", threshold_idx, ratio_idx));

    // 3. Store MultiPoint mapping config (Signal Controller=FX 0, ReaComp=FX 1)
    let mapping_json = serde_json::json!({
        "version": "0.1",
        "mappings": [
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": threshold_idx,
                "mode": {"MultiPoint": {"points": [
                    {"macro_value": 0.0, "param_value": 0.9},
                    {"macro_value": 0.33, "param_value": 0.6},
                    {"macro_value": 0.66, "param_value": 0.3},
                    {"macro_value": 1.0, "param_value": 0.1}
                ]}}
            },
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": ratio_idx,
                "mode": {"MultiPoint": {"points": [
                    {"macro_value": 0.0, "param_value": 0.05},
                    {"macro_value": 0.33, "param_value": 0.3},
                    {"macro_value": 0.66, "param_value": 0.6},
                    {"macro_value": 1.0, "param_value": 0.9}
                ]}}
            }
        ]
    })
    .to_string();

    track.set_ext_state("FTS_MACROS", "mapping_config", &mapping_json).await?;
    ctx.log("Stored MultiPoint mapping config in track P_EXT");

    // Wait for macro timer to discover this track
    tokio::time::sleep(Duration::from_secs(3)).await;

    let poll_timeout = Duration::from_secs(8);
    let poll_interval = Duration::from_millis(50);

    // 4. Test each stage
    struct Stage { name: &'static str, macro_val: f64, expected_threshold: f64, expected_ratio: f64 }

    let stages = [
        Stage { name: "Clean",  macro_val: 0.0,  expected_threshold: 0.9,  expected_ratio: 0.05 },
        Stage { name: "Crunch", macro_val: 0.33, expected_threshold: 0.6,  expected_ratio: 0.3 },
        Stage { name: "Drive",  macro_val: 0.66, expected_threshold: 0.3,  expected_ratio: 0.6 },
        Stage { name: "Lead",   macro_val: 1.0,  expected_threshold: 0.1,  expected_ratio: 0.9 },
    ];

    for stage in &stages {
        ctx.log("");
        ctx.log(&format!("--- Stage: {} (macro={:.2}) ---", stage.name, stage.macro_val));

        set_macro_value(&track, 0, stage.macro_val).await?;

        let thresh = poll_param(&target_fx, threshold_idx, stage.expected_threshold, 0.06, poll_timeout, poll_interval).await?;
        let ratio = poll_param(&target_fx, ratio_idx, stage.expected_ratio, 0.06, poll_timeout, poll_interval).await?;

        ctx.log(&format!("  Threshold={:.4} (expect ~{:.2}), Ratio={:.4} (expect ~{:.2})",
            thresh, stage.expected_threshold, ratio, stage.expected_ratio));
        ctx.log(&format!("PASS: {} stage verified", stage.name));
    }

    // 5. Test interpolation between stages
    ctx.log("");
    ctx.log("--- Interpolation: midpoint between Clean and Crunch (macro=0.165) ---");
    set_macro_value(&track, 0, 0.165).await?;

    let thresh_mid = poll_param(&target_fx, threshold_idx, 0.75, 0.08, poll_timeout, poll_interval).await?;
    let ratio_mid = poll_param(&target_fx, ratio_idx, 0.175, 0.08, poll_timeout, poll_interval).await?;

    ctx.log(&format!("  Threshold={:.4} (expect ~0.75), Ratio={:.4} (expect ~0.175)", thresh_mid, ratio_mid));
    ctx.log("PASS: Interpolation between stages verified");

    ctx.log("");
    ctx.log("=== TEST PASSED: 4-point stage macro with MultiPoint curve ===");
    Ok(())
}

/// Poll an FX parameter until it reaches the expected value (within tolerance), or timeout.
async fn poll_param(
    fx: &daw::FxHandle,
    param_idx: u32,
    expected: f64,
    tolerance: f64,
    timeout: Duration,
    interval: Duration,
) -> eyre::Result<f64> {
    let start = std::time::Instant::now();
    loop {
        let actual = fx.param(param_idx).get().await?;
        if (actual - expected).abs() < tolerance {
            return Ok(actual);
        }
        if start.elapsed() > timeout {
            return Err(eyre::eyre!(
                "Timed out waiting for param {} to reach {:.4} (got {:.4})",
                param_idx,
                expected,
                actual,
            ));
        }
        tokio::time::sleep(interval).await;
    }
}
