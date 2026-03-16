//! REAPER integration test: Compression macro — FTS Macros autonomously drives ReaComp.
//!
//! Tests the full macro pipeline end-to-end:
//!   1. FTS Macros loaded FIRST (FX index 0) — always present in real workflow
//!   2. ReaComp loaded SECOND as the compressor target (FX index 1)
//!   3. Mapping config stored in track P_EXT (`P_EXT:FTS_MACROS:mapping_config`)
//!   4. Setting FTS Macros parameter values drives ReaComp's Threshold + Ratio
//!
//! The test sets macro values via the FX parameter API (so the plugin knobs
//! visually move) and reads back ReaComp values. The plugin's timer callback
//! picks up the parameter changes and drives the targets.
//!
//! ## Synchronization
//!
//! No arbitrary sleeps. The test uses deterministic polling:
//! - **Mapping config:** stored via track P_EXT, plugin reads on next timer tick
//! - **Param values:** polls target FX params until they reach expected values (with tolerance)
//!
//! Run with:
//!   cargo xtask reaper-test compression_macro

use std::time::{Duration, Instant};

use reaper_test::reaper_test;

/// ReaComp plugin name in REAPER's FX browser.
const REACOMP: &str = "VST: ReaComp (Cockos)";

/// FTS Macros CLAP plugin name — try CLAP ID first, then display name.
const FTS_MACROS_CLAP: &str = "CLAP: FTS Macros";
const FTS_MACROS_NAME: &str = "FTS Macros";

/// P_EXT section used by fts-macros for track-scoped config.
const EXT_STATE_SECTION: &str = "FTS_MACROS";

/// Default timeout for polling operations.
const POLL_TIMEOUT: Duration = Duration::from_secs(5);

/// Polling interval between checks.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Build the mapping config JSON for the compression macro.
///
/// Maps Macro 0 (source_param=0) to two ReaComp targets at FX index 1:
///   - Threshold: inverted ScaleRange {0.8, 0.1} — macro up = threshold down
///   - Ratio:     direct  ScaleRange {0.0, 0.8} — macro up = ratio up
fn build_mapping_json(threshold_param_idx: u32, ratio_param_idx: u32) -> String {
    serde_json::json!({
        "version": "0.1",
        "mappings": [
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": threshold_param_idx,
                "mode": {"ScaleRange": {"min": 0.8, "max": 0.1}}
            },
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": ratio_param_idx,
                "mode": {"ScaleRange": {"min": 0.0, "max": 0.8}}
            }
        ]
    })
    .to_string()
}

/// Poll an FX parameter until it reaches the expected value (within tolerance), or timeout.
/// Returns the actual value when matched.
async fn poll_param_value(
    fx: &daw::FxHandle,
    param_idx: u32,
    expected: f64,
    tolerance: f64,
    timeout: Duration,
) -> eyre::Result<f64> {
    let start = Instant::now();
    loop {
        let actual = fx.param(param_idx).get().await?;
        if (actual - expected).abs() < tolerance {
            return Ok(actual);
        }
        if start.elapsed() > timeout {
            return Err(eyre::eyre!(
                "Timed out waiting for param {} to reach {:.4} (got {:.4}, tolerance {:.4}, waited {:?})",
                param_idx,
                expected,
                actual,
                tolerance,
                timeout
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ---------------------------------------------------------------------------
// Test: FTS Macros autonomously drives ReaComp via timer + FX params
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn compression_macro_drives_threshold_and_ratio(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== COMPRESSION MACRO TEST (autonomous plugin control) ===");
    ctx.log("FTS Macros (source, FX 0) + ReaComp (target, FX 1) on same track");
    ctx.log("");

    // ─── 1. Create track and load FTS Macros FIRST (FX index 0) ──────

    let track = project.tracks().add("Compression Macro Test", None).await?;

    let macros_fx = match track.fx_chain().add(FTS_MACROS_CLAP).await {
        Ok(fx) => fx,
        Err(_) => track
            .fx_chain()
            .add(FTS_MACROS_NAME)
            .await
            .map_err(|e| eyre::eyre!("Could not add FTS Macros plugin: {}", e))?,
    };

    ctx.log("FTS Macros loaded at FX index 0");

    // ─── 2. Load ReaComp SECOND (FX index 1) ─────────────────────────

    let target_fx = track.fx_chain().add(REACOMP).await?;

    ctx.log("ReaComp loaded at FX index 1");

    // ─── 3. Discover ReaComp's Threshold and Ratio param indices ─────

    let reacomp_params = target_fx.parameters().await?;
    ctx.log(&format!(
        "ReaComp params: {}",
        reacomp_params
            .iter()
            .take(8)
            .map(|p| format!("{}({})", p.name, p.index))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let threshold_param = reacomp_params
        .iter()
        .find(|p| p.name.to_lowercase().contains("thresh"))
        .ok_or_else(|| eyre::eyre!("Could not find Threshold parameter on ReaComp"))?;

    let ratio_param = reacomp_params
        .iter()
        .find(|p| p.name.to_lowercase().contains("ratio"))
        .ok_or_else(|| eyre::eyre!("Could not find Ratio parameter on ReaComp"))?;

    ctx.log(&format!(
        "Found: Threshold at param_idx={}, Ratio at param_idx={}",
        threshold_param.index, ratio_param.index
    ));

    // ─── 4. Store mapping config in track P_EXT ──────────────────────

    let mapping_json = build_mapping_json(threshold_param.index, ratio_param.index);
    track
        .set_ext_state(EXT_STATE_SECTION, "mapping_config", &mapping_json)
        .await?;
    ctx.log("Stored mapping config in track P_EXT (FTS_MACROS/mapping_config)");

    // Give the timer a moment to pick up the new config
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify both plugins loaded
    let fx_count = track.fx_chain().count().await?;
    assert!(
        fx_count >= 2,
        "Track should have at least 2 FX (FTS Macros + ReaComp), got {}",
        fx_count
    );

    // ─── 5. Macro 0 = 0.0 → verify ReaComp at low compression ───────

    ctx.log("");
    ctx.log("--- Macro 0 = 0.0 (minimum compression) ---");

    // Set macro via FX parameter API — the knob moves and the timer picks it up
    macros_fx.param(0).set(0.0).await?;

    let thresh_at_0 = poll_param_value(&target_fx, threshold_param.index, 0.80, 0.05, POLL_TIMEOUT).await?;
    let ratio_at_0 = poll_param_value(&target_fx, ratio_param.index, 0.00, 0.05, POLL_TIMEOUT).await?;
    ctx.log(&format!(
        "ReaComp: Threshold={:.4}, Ratio={:.4}",
        thresh_at_0, ratio_at_0
    ));
    ctx.log("PASS: Low compression — high threshold, low ratio");

    // ─── 6. Macro 0 = 1.0 → verify ReaComp at heavy compression ─────

    ctx.log("");
    ctx.log("--- Macro 0 = 1.0 (maximum compression) ---");

    macros_fx.param(0).set(1.0).await?;

    let thresh_at_1 = poll_param_value(&target_fx, threshold_param.index, 0.10, 0.05, POLL_TIMEOUT).await?;
    let ratio_at_1 = poll_param_value(&target_fx, ratio_param.index, 0.80, 0.05, POLL_TIMEOUT).await?;
    ctx.log(&format!(
        "ReaComp: Threshold={:.4}, Ratio={:.4}",
        thresh_at_1, ratio_at_1
    ));
    ctx.log("PASS: Heavy compression — low threshold, high ratio");

    // ─── 7. Sweep Macro 0 from 0.0 → 1.0, verify monotonic ──────────

    ctx.log("");
    ctx.log("Sweeping Macro 0 from 0.0 → 1.0 in 5 steps:");

    let mut prev_threshold = 1.0_f64;
    let mut prev_ratio = -1.0_f64;

    for step in 0..=4 {
        let macro_val = step as f64 / 4.0;

        // Expected values from the ScaleRange mappings
        let expected_thresh = 0.8 + macro_val * (0.1 - 0.8); // inverted
        let expected_ratio = 0.0 + macro_val * 0.8;           // direct

        // Set via FX parameter — knob moves, timer reads it
        macros_fx.param(0).set(macro_val).await?;

        let thresh = poll_param_value(&target_fx, threshold_param.index, expected_thresh, 0.05, POLL_TIMEOUT).await?;
        let ratio = poll_param_value(&target_fx, ratio_param.index, expected_ratio, 0.05, POLL_TIMEOUT).await?;
        ctx.log(&format!(
            "  Macro 0={:.2} → Threshold={:.4}, Ratio={:.4}",
            macro_val, thresh, ratio
        ));

        if step > 0 {
            assert!(
                thresh <= prev_threshold + 0.02,
                "Threshold should decrease (step {}): prev={:.4}, now={:.4}",
                step,
                prev_threshold,
                thresh
            );
            assert!(
                ratio >= prev_ratio - 0.02,
                "Ratio should increase (step {}): prev={:.4}, now={:.4}",
                step,
                prev_ratio,
                ratio
            );
        }
        prev_threshold = thresh;
        prev_ratio = ratio;
    }

    ctx.log("PASS: Monotonic sweep — threshold decreases, ratio increases");

    // ─── 8. Show all plugin windows floating ───────────────────────

    project.run_command("_S&M_WNTSHW1").await
        .map_err(|e| eyre::eyre!("Failed to run show-windows action: {e}"))?;
    ctx.log("Opened all plugin windows floating (SWS: _S&M_WNTSHW1)");

    ctx.log("");
    ctx.log("=== TEST PASSED: FTS Macros autonomously drives ReaComp via timer + FX params ===");

    Ok(())
}
