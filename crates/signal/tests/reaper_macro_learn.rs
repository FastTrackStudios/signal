//! REAPER integration tests for the macro system and demo setlist.
//!
//! Tests use REAPER actions (same as the user would) rather than
//! programmatic setup, verifying the full signal-extension pipeline.
//!
//! Run with:
//!   cargo xtask reaper-test reaper_macro_learn

use std::time::Duration;

use reaper_test::reaper_test;

const REACOMP: &str = "VST: ReaComp (Cockos)";

async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Signal controller CLAP names to try.
const SIGNAL_CONTROLLER_NAMES: &[&str] = &[
    "CLAP: FTS Signal Controller (FastTrackStudio)",
    "CLAP: FTS Signal Controller",
    "FTS Signal Controller",
];

async fn add_signal_controller(track: &daw::TrackHandle) -> eyre::Result<daw::FxHandle> {
    for name in SIGNAL_CONTROLLER_NAMES {
        if let Ok(fx) = track.fx_chain().add(name).await {
            return Ok(fx);
        }
    }
    Err(eyre::eyre!("FTS Signal Controller not available"))
}

/// Wait for signal-extension to be ready (it writes "ready" to ExtState on startup).
async fn wait_for_signal_extension(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = ext.get("FTS_SIGNAL_EXT", "status").await? {
            if status == "ready" {
                return Ok(());
            }
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(eyre::eyre!("Signal extension not ready after 10s"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Run a signal action by command name, retrying until the action is registered.
async fn run_signal_action(ctx: &reaper_test::ReaperTestContext, action: &str) -> eyre::Result<()> {
    wait_for_signal_extension(ctx).await?;

    let project = ctx.project();
    let start = std::time::Instant::now();
    loop {
        let ok = project.run_command(action).await?;
        if ok {
            // Give the async action handler time to execute
            tokio::time::sleep(Duration::from_millis(500)).await;
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(10) {
            return Err(eyre::eyre!("Action not found after 10s: {action}"));
        }
        // Action not registered yet — wait and retry
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Poll an FX parameter until it reaches the expected value.
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
                param_idx, expected, actual,
            ));
        }
        tokio::time::sleep(interval).await;
    }
}

// ---------------------------------------------------------------------------
// Test: Load signal controller onto a track
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn load_signal_controller_on_track(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Controller Load Test", None).await?;

    ctx.log("Loading FTS Signal Controller...");
    let fx = add_signal_controller(&track).await?;
    let info = fx.info().await?;
    ctx.log(&format!("Loaded: {}", info.name));

    let params = fx.parameters().await?;
    ctx.log(&format!("{} parameters", params.len()));
    for p in params.iter().take(8) {
        ctx.log(&format!("  {} (idx {})", p.name, p.index));
    }

    assert!(!params.is_empty());
    ctx.log("=== PASS: Signal Controller loaded ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: last_touched_fx detects parameter changes
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn last_touched_fx_detects_reacomp_parameter(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Last Touched Test", None).await?;
    let fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    let params = fx.parameters().await?;
    let threshold_idx = params.iter().find(|p| p.name.to_lowercase().contains("thresh"))
        .map(|p| p.index).ok_or_else(|| eyre::eyre!("Threshold not found"))?;

    fx.param(threshold_idx).set(0.3).await?;
    settle().await;

    let lt = ctx.daw.last_touched_fx().await?
        .ok_or_else(|| eyre::eyre!("last_touched_fx returned None"))?;

    assert_eq!(lt.param_index, threshold_idx);
    ctx.log("=== PASS: last_touched_fx detection works ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: ReaComp parameter round-trip
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn reacomp_param_set_and_readback(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Param Readback", None).await?;
    let fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

    let params = fx.parameters().await?;
    let test_values = [0.1, 0.3, 0.5, 0.7, 0.9];
    let count = test_values.len().min(params.len());

    for (i, &val) in test_values.iter().enumerate().take(count) {
        fx.param(params[i].index).set(val).await?;
    }
    settle().await;

    for (i, &expected) in test_values.iter().enumerate().take(count) {
        let actual = fx.param(params[i].index).get().await?;
        assert!((actual - expected).abs() < 0.05, "Param '{}' should be ~{:.2}, got {:.4}", params[i].name, expected, actual);
    }

    ctx.log("=== PASS: Parameter round-trip verified ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Macro set min/max via signal controller FX params + mapping config
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_drives_reacomp_via_fx_params(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== MACRO DRIVES REACOMP VIA FX PARAMS ===");

    // 1. Create track with Signal Controller (FX 0) + ReaComp (FX 1)
    let track = project.tracks().add("Macro FX Test", None).await?;
    let controller = add_signal_controller(&track).await?;
    ctx.log("FX 0: Signal Controller loaded");
    let target_fx = track.fx_chain().add(REACOMP).await?;
    ctx.log("FX 1: ReaComp loaded");

    let params = target_fx.parameters().await?;
    let threshold_idx = params.iter().find(|p| p.name.to_lowercase().contains("thresh"))
        .map(|p| p.index).ok_or_else(|| eyre::eyre!("Threshold not found"))?;
    let ratio_idx = params.iter().find(|p| p.name.to_lowercase().contains("ratio"))
        .map(|p| p.index).ok_or_else(|| eyre::eyre!("Ratio not found"))?;

    // 2. Store mapping config (Signal Controller=FX 0, ReaComp=FX 1)
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
    }).to_string();

    track.set_ext_state("FTS_MACROS", "mapping_config", &mapping_json).await?;
    ctx.log("Stored mapping config");

    // Wait for timer to discover
    tokio::time::sleep(Duration::from_secs(3)).await;

    let poll_timeout = Duration::from_secs(8);
    let poll_interval = Duration::from_millis(50);

    // 3. Set Macro 0 = 0.0 via FX param
    ctx.log("--- Macro 0 = 0.0 ---");
    controller.param(0).set(0.0).await?;

    let t0 = poll_param(&target_fx, threshold_idx, 0.8, 0.05, poll_timeout, poll_interval).await?;
    let r0 = poll_param(&target_fx, ratio_idx, 0.1, 0.05, poll_timeout, poll_interval).await?;
    ctx.log(&format!("  Threshold={:.4}, Ratio={:.4}", t0, r0));

    // 4. Set Macro 0 = 1.0 via FX param
    ctx.log("--- Macro 0 = 1.0 ---");
    controller.param(0).set(1.0).await?;

    let t1 = poll_param(&target_fx, threshold_idx, 0.2, 0.05, poll_timeout, poll_interval).await?;
    let r1 = poll_param(&target_fx, ratio_idx, 0.9, 0.05, poll_timeout, poll_interval).await?;
    ctx.log(&format!("  Threshold={:.4}, Ratio={:.4}", t1, r1));

    ctx.log("=== PASS: Macro drives ReaComp via FX params ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: 4-point stage macro with MultiPoint curve
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn four_point_stage_macro(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== 4-POINT STAGE MACRO ===");

    let track = project.tracks().add("Stage Macro", None).await?;
    let controller = add_signal_controller(&track).await?;
    let target_fx = track.fx_chain().add(REACOMP).await?;

    let params = target_fx.parameters().await?;
    let threshold_idx = params.iter().find(|p| p.name.to_lowercase().contains("thresh"))
        .map(|p| p.index).ok_or_else(|| eyre::eyre!("Threshold not found"))?;
    let ratio_idx = params.iter().find(|p| p.name.to_lowercase().contains("ratio"))
        .map(|p| p.index).ok_or_else(|| eyre::eyre!("Ratio not found"))?;

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
    }).to_string();

    track.set_ext_state("FTS_MACROS", "mapping_config", &mapping_json).await?;
    tokio::time::sleep(Duration::from_secs(3)).await;

    let poll_timeout = Duration::from_secs(8);
    let poll_interval = Duration::from_millis(50);

    let stages = [
        ("Clean", 0.0, 0.9, 0.05),
        ("Crunch", 0.33, 0.6, 0.3),
        ("Drive", 0.66, 0.3, 0.6),
        ("Lead", 1.0, 0.1, 0.9),
    ];

    for (name, macro_val, exp_thresh, exp_ratio) in &stages {
        ctx.log(&format!("--- {name} (macro={macro_val:.2}) ---"));
        controller.param(0).set(*macro_val).await?;
        let t = poll_param(&target_fx, threshold_idx, *exp_thresh, 0.06, poll_timeout, poll_interval).await?;
        let r = poll_param(&target_fx, ratio_idx, *exp_ratio, 0.06, poll_timeout, poll_interval).await?;
        ctx.log(&format!("  Threshold={t:.4}, Ratio={r:.4}"));
    }

    // Interpolation
    controller.param(0).set(0.165).await?;
    let t = poll_param(&target_fx, threshold_idx, 0.75, 0.08, poll_timeout, poll_interval).await?;
    let r = poll_param(&target_fx, ratio_idx, 0.175, 0.08, poll_timeout, poll_interval).await?;
    ctx.log(&format!("Interpolation: Threshold={t:.4}, Ratio={r:.4}"));

    ctx.log("=== PASS: 4-point stage macro ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Demo setlist action creates expected track structure
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn z_demo_setlist_action_creates_tracks(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    ctx.log("=== DEMO SETLIST VIA ACTION ===");

    // Run the demo setlist action
    ctx.log("Running FTS_SIGNAL_DEV_LOAD_DEMO_SETLIST...");
    run_signal_action(ctx, "FTS_SIGNAL_DEV_LOAD_DEMO_SETLIST").await?;

    // Give it time to create all tracks (demo setlist creates many tracks sequentially)
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify tracks were created
    let project = ctx.project().clone();
    let all_tracks = project.tracks().all().await?;

    ctx.log(&format!("{} tracks created:", all_tracks.len()));
    for t in &all_tracks {
        let folder = if t.is_folder { " [folder]" } else { "" };
        let parent = t.parent_guid.as_deref().unwrap_or("root");
        ctx.log(&format!("  {} (parent={}){}", t.name, parent, folder));
    }

    // Should have at least a rig folder + input + song folders + sections
    assert!(
        all_tracks.len() >= 5,
        "Demo setlist should create at least 5 tracks, got {}",
        all_tracks.len()
    );

    // Check for the rig folder
    let rig = all_tracks.iter().find(|t| t.name.contains("Guitar Rig") || t.name.contains("Rig"));
    assert!(rig.is_some(), "Should have a Guitar Rig folder");

    // Check for at least one song folder
    let songs: Vec<_> = all_tracks.iter().filter(|t| t.is_folder && t.parent_guid.is_some()).collect();
    assert!(!songs.is_empty(), "Should have at least one song folder");

    // ── Verify sends from Guitar Input to section tracks ──────────────
    let rig = rig.unwrap();
    let guitar_input = all_tracks.iter().find(|t| t.name == "Guitar Input");
    assert!(guitar_input.is_some(), "Should have Guitar Input track");
    let guitar_input = guitar_input.unwrap();

    let gi_track = project.tracks().by_guid(&guitar_input.guid).await?
        .ok_or_else(|| eyre::eyre!("Guitar Input track not found by GUID"))?;
    let sends = gi_track.sends().all().await?;
    ctx.log(&format!("Guitar Input has {} sends", sends.len()));

    // Should have at least 3 sends (Belief has 3 sections)
    assert!(
        sends.len() >= 3,
        "Guitar Input should have at least 3 sends, got {}",
        sends.len()
    );

    // Verify input_track_guid is stored in P_EXT
    let rig_track = project.tracks().by_guid(&rig.guid).await?
        .ok_or_else(|| eyre::eyre!("Rig folder not found"))?;
    let stored_guid = rig_track.get_ext_state("fts_signal", "input_track_guid").await?;
    assert!(stored_guid.is_some(), "Should have input_track_guid in P_EXT");
    assert_eq!(
        stored_guid.as_deref(), Some(guitar_input.guid.as_str()),
        "Stored input GUID should match Guitar Input"
    );
    ctx.log(&format!("input_track_guid stored correctly: {}", guitar_input.guid));

    // ── Verify scene switching mutes sends (not tracks) ───────────────
    // Wait for scene timer to initialize
    tokio::time::sleep(Duration::from_secs(6)).await;

    // Move transport to position 0 (first section of first song)
    let transport = project.transport();
    transport.set_position(0.5).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Check that section tracks are NOT muted (all should be unmuted)
    let first_song = songs[0];
    let section_tracks: Vec<_> = all_tracks.iter()
        .filter(|t| t.parent_guid.as_deref() == Some(&first_song.guid) && !t.is_folder)
        .collect();

    for sec in &section_tracks {
        let sec_track = project.tracks().by_guid(&sec.guid).await?
            .ok_or_else(|| eyre::eyre!("Section track not found"))?;
        let muted = sec_track.is_muted().await?;
        ctx.log(&format!("  Section '{}' muted={}", sec.name, muted));
        assert!(!muted, "Section track '{}' should NOT be muted (sends control routing)", sec.name);
    }

    ctx.log("=== PASS: Demo setlist with send-based routing ===");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Macro Set Min/Max via REAPER actions
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn macro_set_min_max_via_actions(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== MACRO SET MIN/MAX VIA ACTIONS ===");

    // 1. Create track with Signal Controller + ReaComp
    let track = project.tracks().add("Action Macro Test", None).await?;
    let controller = add_signal_controller(&track).await?;
    ctx.log("Signal Controller loaded");
    let target_fx = track.fx_chain().add(REACOMP).await?;
    ctx.log("ReaComp loaded");

    let params = target_fx.parameters().await?;
    let threshold_idx = params.iter().find(|p| p.name.to_lowercase().contains("thresh"))
        .map(|p| p.index).ok_or_else(|| eyre::eyre!("Threshold not found"))?;

    // 2. Move Macro 1 on signal controller (so timer writes last_macro_index=0)
    controller.param(0).set(0.5).await?;
    settle().await;

    // 3. Set ReaComp Threshold to min position and touch it (last_touched = threshold)
    //    Note: no mapping config exists yet, so the timer won't override this value
    target_fx.param(threshold_idx).set(0.8).await?;
    settle().await;

    // 4. Run Set Min action — captures threshold=0.8 at macro=0.0
    ctx.log("Running FTS_SIGNAL_MACRO_SET_MIN...");
    run_signal_action(ctx, "FTS_SIGNAL_MACRO_SET_MIN").await?;

    // 5. Set Threshold to max position and touch it
    target_fx.param(threshold_idx).set(0.2).await?;
    settle().await;

    // 6. Run Set Max action — captures threshold=0.2 at macro=1.0
    //    The timer won't override because last_touched_fx is the threshold
    ctx.log("Running FTS_SIGNAL_MACRO_SET_MAX...");
    run_signal_action(ctx, "FTS_SIGNAL_MACRO_SET_MAX").await?;

    // 7. Verify mapping was saved to P_EXT
    let config = track.get_ext_state("FTS_MACROS", "mapping_config").await?;
    ctx.log(&format!("Mapping config: {:?}", config.as_deref().map(|s| &s[..s.len().min(100)])));

    assert!(config.is_some(), "Mapping config should be saved to P_EXT");
    let config_json = config.unwrap();
    assert!(config_json.contains("ScaleRange") || config_json.contains("MultiPoint"),
        "Config should have a mapping mode");

    // 8. Now move the macro and verify the parameter responds
    ctx.log("Testing macro drives parameter...");
    tokio::time::sleep(Duration::from_secs(3)).await; // Wait for timer to pick up config

    controller.param(0).set(0.0).await?;
    let poll_timeout = Duration::from_secs(8);
    let poll_interval = Duration::from_millis(50);

    let val_at_0 = poll_param(&target_fx, threshold_idx, 0.8, 0.1, poll_timeout, poll_interval).await?;
    ctx.log(&format!("Macro=0.0 → Threshold={val_at_0:.4} (expect ~0.8)"));

    controller.param(0).set(1.0).await?;
    let val_at_1 = poll_param(&target_fx, threshold_idx, 0.2, 0.1, poll_timeout, poll_interval).await?;
    ctx.log(&format!("Macro=1.0 → Threshold={val_at_1:.4} (expect ~0.2)"));

    ctx.log("=== PASS: Macro set min/max via actions ===");
    Ok(())
}
