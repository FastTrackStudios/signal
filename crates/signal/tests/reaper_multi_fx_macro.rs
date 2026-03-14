//! REAPER integration test: Single macro drives parameters across multiple plugins.
//!
//! Tests that one FTS Macros knob can simultaneously control:
//!   - ReaComp: Threshold (inverted) + Ratio (direct)
//!   - ReaEQ:   Band 1 Gain (direct) + Band 1 Frequency (direct)
//!
//! This validates the real-world use case of a "tone shaper" macro that
//! tightens compression and brightens EQ in one knob turn.
//!
//! Run with:
//!   cargo xtask reaper-test multi_fx_macro

use std::time::{Duration, Instant};

use reaper_test::reaper_test;

const REACOMP: &str = "VST: ReaComp (Cockos)";
const REAEQ: &str = "VST: ReaEQ (Cockos)";
const FTS_MACROS_CLAP: &str = "CLAP: FTS Macros";
const FTS_MACROS_NAME: &str = "FTS Macros";
const EXT_STATE_SECTION: &str = "FTS_MACROS";
const POLL_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Build mapping config: Macro 0 drives 4 targets across 2 plugins.
///
/// FX layout on track:
///   FX 0 = FTS Macros (source)
///   FX 1 = ReaComp   (target)
///   FX 2 = ReaEQ     (target)
fn build_mapping_json(
    comp_threshold_idx: u32,
    comp_ratio_idx: u32,
    eq_gain_idx: u32,
    eq_freq_idx: u32,
) -> String {
    serde_json::json!({
        "version": "0.1",
        "mappings": [
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": comp_threshold_idx,
                "mode": {"ScaleRange": {"min": 0.8, "max": 0.1}}
            },
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 1},
                "target_param_index": comp_ratio_idx,
                "mode": {"ScaleRange": {"min": 0.0, "max": 0.8}}
            },
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 2},
                "target_param_index": eq_gain_idx,
                "mode": {"ScaleRange": {"min": 0.5, "max": 1.0}}
            },
            {
                "source_param": 0,
                "target_track": {"ByIndex": 0},
                "target_fx": {"ByIndex": 2},
                "target_param_index": eq_freq_idx,
                "mode": {"ScaleRange": {"min": 0.3, "max": 0.9}}
            }
        ]
    })
    .to_string()
}

async fn poll_ext_state(
    ctx: &reaper_test::ReaperTestContext,
    section: &str,
    key: &str,
    timeout: Duration,
) -> eyre::Result<String> {
    let start = Instant::now();
    loop {
        if let Some(val) = ctx.daw.ext_state().get(section, key).await? {
            if !val.is_empty() {
                return Ok(val);
            }
        }
        if start.elapsed() > timeout {
            return Err(eyre::eyre!(
                "Timed out waiting for ExtState {}/{} (waited {:?})",
                section, key, timeout
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn poll_param_value(
    fx: &daw_control::FxHandle,
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
                param_idx, expected, actual, tolerance, timeout
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn cleanup_ext_state(ctx: &reaper_test::ReaperTestContext) {
    for i in 0..8 {
        let _ = ctx.daw.ext_state().delete(EXT_STATE_SECTION, &format!("macro_{}", i), false).await;
    }
    let _ = ctx.daw.ext_state().delete(EXT_STATE_SECTION, "mapping_config", false).await;
    let _ = ctx.daw.ext_state().delete(EXT_STATE_SECTION, "mapping_config_ack", false).await;
}

/// Find a parameter by name substring (case-insensitive).
fn find_param(params: &[daw_proto::FxParameter], needle: &str) -> eyre::Result<u32> {
    params
        .iter()
        .find(|p| p.name.to_lowercase().contains(&needle.to_lowercase()))
        .map(|p| p.index)
        .ok_or_else(|| eyre::eyre!("Could not find parameter containing '{}'", needle))
}

// ---------------------------------------------------------------------------
// Test: Single macro drives ReaComp + ReaEQ simultaneously
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn multi_fx_macro_drives_comp_and_eq(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== MULTI-FX MACRO TEST ===");
    ctx.log("Macro 0 → ReaComp (Threshold + Ratio) + ReaEQ (Gain + Freq)");
    ctx.log("");

    cleanup_ext_state(ctx).await;

    // ─── 1. Create track and load FX chain ───────────────────────────

    let track = project.tracks().add("Multi-FX Macro Test", None).await?;

    // FX 0: FTS Macros (source)
    let macros_fx = match track.fx_chain().add(FTS_MACROS_CLAP).await {
        Ok(fx) => fx,
        Err(_) => track
            .fx_chain()
            .add(FTS_MACROS_NAME)
            .await
            .map_err(|e| eyre::eyre!("Could not add FTS Macros: {}", e))?,
    };
    ctx.log("FX 0: FTS Macros loaded");

    // FX 1: ReaComp (target)
    let comp_fx = track.fx_chain().add(REACOMP).await?;
    ctx.log("FX 1: ReaComp loaded");

    // FX 2: ReaEQ (target)
    let eq_fx = track.fx_chain().add(REAEQ).await?;
    ctx.log("FX 2: ReaEQ loaded");

    // ─── 2. Discover parameter indices ───────────────────────────────

    let comp_params = comp_fx.parameters().await?;
    let comp_threshold = find_param(&comp_params, "thresh")?;
    let comp_ratio = find_param(&comp_params, "ratio")?;
    ctx.log(&format!(
        "ReaComp: Threshold={}, Ratio={}",
        comp_threshold, comp_ratio
    ));

    let eq_params = eq_fx.parameters().await?;
    ctx.log(&format!(
        "ReaEQ params: {}",
        eq_params
            .iter()
            .take(12)
            .map(|p| format!("{}({})", p.name, p.index))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let eq_gain = find_param(&eq_params, "gain")?;
    let eq_freq = find_param(&eq_params, "freq")?;
    ctx.log(&format!("ReaEQ: Gain={}, Freq={}", eq_gain, eq_freq));

    // ─── 3. Inject mapping config ────────────────────────────────────

    let mapping_json = build_mapping_json(comp_threshold, comp_ratio, eq_gain, eq_freq);
    ctx.daw
        .ext_state()
        .set(EXT_STATE_SECTION, "mapping_config", &mapping_json, false)
        .await?;

    let ack = poll_ext_state(ctx, EXT_STATE_SECTION, "mapping_config_ack", POLL_TIMEOUT).await?;
    ctx.log(&format!("Mapping config acknowledged: {} mappings", ack));

    // Verify FX chain
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(fx_count, 3, "should have 3 FX (macros + comp + eq), got {}", fx_count);

    // ─── 4. Macro 0 = 0.0 → all targets at min ──────────────────────

    ctx.log("");
    ctx.log("--- Macro 0 = 0.0 (all targets at minimum) ---");

    macros_fx.param(0).set(0.0).await?;

    let ct0 = poll_param_value(&comp_fx, comp_threshold, 0.80, 0.05, POLL_TIMEOUT).await?;
    let cr0 = poll_param_value(&comp_fx, comp_ratio, 0.00, 0.05, POLL_TIMEOUT).await?;
    let eg0 = poll_param_value(&eq_fx, eq_gain, 0.50, 0.05, POLL_TIMEOUT).await?;
    let ef0 = poll_param_value(&eq_fx, eq_freq, 0.30, 0.05, POLL_TIMEOUT).await?;

    ctx.log(&format!(
        "  Comp: Threshold={:.4}, Ratio={:.4}",
        ct0, cr0
    ));
    ctx.log(&format!(
        "  EQ:   Gain={:.4}, Freq={:.4}",
        eg0, ef0
    ));
    ctx.log("PASS: All 4 targets at minimum position");

    // ─── 5. Macro 0 = 1.0 → all targets at max ──────────────────────

    ctx.log("");
    ctx.log("--- Macro 0 = 1.0 (all targets at maximum) ---");

    macros_fx.param(0).set(1.0).await?;

    let ct1 = poll_param_value(&comp_fx, comp_threshold, 0.10, 0.05, POLL_TIMEOUT).await?;
    let cr1 = poll_param_value(&comp_fx, comp_ratio, 0.80, 0.05, POLL_TIMEOUT).await?;
    let eg1 = poll_param_value(&eq_fx, eq_gain, 1.00, 0.05, POLL_TIMEOUT).await?;
    let ef1 = poll_param_value(&eq_fx, eq_freq, 0.90, 0.05, POLL_TIMEOUT).await?;

    ctx.log(&format!(
        "  Comp: Threshold={:.4}, Ratio={:.4}",
        ct1, cr1
    ));
    ctx.log(&format!(
        "  EQ:   Gain={:.4}, Freq={:.4}",
        eg1, ef1
    ));
    ctx.log("PASS: All 4 targets at maximum position");

    // ─── 6. Sweep and verify all 4 targets move monotonically ────────

    ctx.log("");
    ctx.log("Sweeping Macro 0 from 0.0 → 1.0 in 5 steps:");

    let mut prev_ct = 1.0_f64;
    let mut prev_cr = -1.0_f64;
    let mut prev_eg = -1.0_f64;
    let mut prev_ef = -1.0_f64;

    for step in 0..=4 {
        let v = step as f64 / 4.0;

        let exp_ct = 0.8 + v * (0.1 - 0.8);  // inverted: 0.8 → 0.1
        let exp_cr = v * 0.8;                  // direct:   0.0 → 0.8
        let exp_eg = 0.5 + v * 0.5;           // direct:   0.5 → 1.0
        let exp_ef = 0.3 + v * 0.6;           // direct:   0.3 → 0.9

        macros_fx.param(0).set(v).await?;

        let ct = poll_param_value(&comp_fx, comp_threshold, exp_ct, 0.05, POLL_TIMEOUT).await?;
        let cr = poll_param_value(&comp_fx, comp_ratio, exp_cr, 0.05, POLL_TIMEOUT).await?;
        let eg = poll_param_value(&eq_fx, eq_gain, exp_eg, 0.05, POLL_TIMEOUT).await?;
        let ef = poll_param_value(&eq_fx, eq_freq, exp_ef, 0.05, POLL_TIMEOUT).await?;

        ctx.log(&format!(
            "  Macro={:.2} → Thresh={:.3}, Ratio={:.3}, Gain={:.3}, Freq={:.3}",
            v, ct, cr, eg, ef
        ));

        if step > 0 {
            assert!(
                ct <= prev_ct + 0.02,
                "Threshold should decrease (step {}): {:.4} → {:.4}",
                step, prev_ct, ct
            );
            assert!(
                cr >= prev_cr - 0.02,
                "Ratio should increase (step {}): {:.4} → {:.4}",
                step, prev_cr, cr
            );
            assert!(
                eg >= prev_eg - 0.02,
                "EQ Gain should increase (step {}): {:.4} → {:.4}",
                step, prev_eg, eg
            );
            assert!(
                ef >= prev_ef - 0.02,
                "EQ Freq should increase (step {}): {:.4} → {:.4}",
                step, prev_ef, ef
            );
        }

        prev_ct = ct;
        prev_cr = cr;
        prev_eg = eg;
        prev_ef = ef;
    }

    ctx.log("PASS: All 4 targets move monotonically across sweep");

    // ─── 7. Show plugin windows ──────────────────────────────────────

    project.run_command("_S&M_WNTSHW1").await
        .map_err(|e| eyre::eyre!("Failed to show plugin windows: {e}"))?;
    ctx.log("Opened all plugin windows floating");

    // ─── Clean up ────────────────────────────────────────────────────

    cleanup_ext_state(ctx).await;

    ctx.log("");
    ctx.log("=== TEST PASSED: Single macro drives 4 params across 2 plugins ===");

    Ok(())
}
