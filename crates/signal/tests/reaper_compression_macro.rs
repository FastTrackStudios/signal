//! REAPER integration test: Compression macro — FTS Macros autonomously drives ReaComp.
//!
//! Tests the full macro pipeline end-to-end:
//!   1. FTS Macros loaded FIRST (FX index 0) — always present in real workflow
//!   2. ReaComp loaded SECOND as the compressor target (FX index 1)
//!   3. Mapping config injected via ExtState IPC → stored in plugin's persist field
//!   4. Setting macro values via ExtState autonomously drives ReaComp's Threshold + Ratio
//!
//! The test ONLY sets macro values (via ExtState) and reads back ReaComp values.
//! The plugin's timer callback does all the driving.
//!
//! ## Why ExtState for mappings?
//!
//! Instead of writing a file before plugin load, we inject mappings via ExtState
//! (`FTS_MACROS/mapping_config`). The plugin's timer callback picks up the JSON,
//! updates the shared `Arc<Mutex<MacroMappingBank>>` (which is also the `#[persist]`
//! field), and deletes the key. This makes mappings survive project save/load.
//!
//! Run with:
//!   cargo xtask reaper-test compression_macro

use std::time::Duration;

use reaper_test::reaper_test;

/// Small sleep to let the plugin's timer callback (~30Hz) pick up changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

/// ReaComp plugin name in REAPER's FX browser.
const REACOMP: &str = "VST: ReaComp (Cockos)";

/// FTS Macros CLAP plugin name — try CLAP ID first, then display name.
const FTS_MACROS_CLAP: &str = "CLAP: FTS Macros";
const FTS_MACROS_NAME: &str = "FTS Macros";

/// ExtState section used by fts-macros timer to read macro values and mapping config.
const EXT_STATE_SECTION: &str = "FTS_MACROS";

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

/// Set a macro value via REAPER ExtState (read by fts-macros timer callback).
async fn set_macro_via_ext_state(
    ctx: &reaper_test::ReaperTestContext,
    macro_idx: u32,
    value: f64,
) -> eyre::Result<()> {
    ctx.daw
        .ext_state()
        .set(
            EXT_STATE_SECTION,
            &format!("macro_{}", macro_idx),
            &format!("{:.6}", value),
            false,
        )
        .await?;
    Ok(())
}

/// Clean up ExtState keys.
async fn cleanup_ext_state(ctx: &reaper_test::ReaperTestContext) {
    for i in 0..8 {
        let _ = ctx
            .daw
            .ext_state()
            .delete(EXT_STATE_SECTION, &format!("macro_{}", i), false)
            .await;
    }
    // Also clean up mapping_config key
    let _ = ctx
        .daw
        .ext_state()
        .delete(EXT_STATE_SECTION, "mapping_config", false)
        .await;
}

// ---------------------------------------------------------------------------
// Test: FTS Macros autonomously drives ReaComp via timer + ExtState
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn compression_macro_drives_threshold_and_ratio(
    ctx: &reaper_test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();

    ctx.log("=== COMPRESSION MACRO TEST (autonomous plugin control) ===");
    ctx.log("FTS Macros (source, FX 0) + ReaComp (target, FX 1) on same track");
    ctx.log("");

    // Clean up any stale state from previous runs
    cleanup_ext_state(ctx).await;

    // ─── 1. Create track and load FTS Macros FIRST (FX index 0) ──────

    let track = project.tracks().add("Compression Macro Test", None).await?;
    settle().await;

    let _macros_fx = match track.fx_chain().add(FTS_MACROS_CLAP).await {
        Ok(fx) => fx,
        Err(_) => track
            .fx_chain()
            .add(FTS_MACROS_NAME)
            .await
            .map_err(|e| eyre::eyre!("Could not add FTS Macros plugin: {}", e))?,
    };
    settle().await;

    ctx.log("FTS Macros loaded at FX index 0");

    // ─── 2. Load ReaComp SECOND (FX index 1) ─────────────────────────

    let target_fx = track.fx_chain().add(REACOMP).await?;
    settle().await;

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

    // ─── 4. Inject mapping config via ExtState IPC ───────────────────

    let mapping_json = build_mapping_json(threshold_param.index, ratio_param.index);
    ctx.daw
        .ext_state()
        .set(EXT_STATE_SECTION, "mapping_config", &mapping_json, false)
        .await?;
    ctx.log("Injected mapping config via ExtState (FTS_MACROS/mapping_config)");

    // Wait for timer to pick up the mapping config
    tokio::time::sleep(Duration::from_millis(1500)).await;

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

    set_macro_via_ext_state(ctx, 0, 0.0).await?;
    // Wait for timer to pick up the change (~30Hz = 33ms per tick, wait several ticks)
    tokio::time::sleep(Duration::from_millis(500)).await;

    let thresh_at_0 = target_fx.param(threshold_param.index).get().await?;
    let ratio_at_0 = target_fx.param(ratio_param.index).get().await?;
    ctx.log(&format!(
        "ReaComp: Threshold={:.4} (expect ~0.80), Ratio={:.4} (expect ~0.00)",
        thresh_at_0, ratio_at_0
    ));

    assert!(
        (thresh_at_0 - 0.80).abs() < 0.05,
        "At Macro 0=0.0, threshold should be ~0.80, got {:.4}",
        thresh_at_0
    );
    assert!(
        (ratio_at_0 - 0.00).abs() < 0.05,
        "At Macro 0=0.0, ratio should be ~0.00, got {:.4}",
        ratio_at_0
    );
    ctx.log("PASS: Low compression — high threshold, low ratio");

    // ─── 6. Macro 0 = 1.0 → verify ReaComp at heavy compression ─────

    ctx.log("");
    ctx.log("--- Macro 0 = 1.0 (maximum compression) ---");

    set_macro_via_ext_state(ctx, 0, 1.0).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let thresh_at_1 = target_fx.param(threshold_param.index).get().await?;
    let ratio_at_1 = target_fx.param(ratio_param.index).get().await?;
    ctx.log(&format!(
        "ReaComp: Threshold={:.4} (expect ~0.10), Ratio={:.4} (expect ~0.80)",
        thresh_at_1, ratio_at_1
    ));

    assert!(
        (thresh_at_1 - 0.10).abs() < 0.05,
        "At Macro 0=1.0, threshold should be ~0.10, got {:.4}",
        thresh_at_1
    );
    assert!(
        (ratio_at_1 - 0.80).abs() < 0.05,
        "At Macro 0=1.0, ratio should be ~0.80, got {:.4}",
        ratio_at_1
    );
    ctx.log("PASS: Heavy compression — low threshold, high ratio");

    // ─── 7. Sweep Macro 0 from 0.0 → 1.0, verify monotonic ──────────

    ctx.log("");
    ctx.log("Sweeping Macro 0 from 0.0 → 1.0 in 5 steps:");

    let mut prev_threshold = 1.0_f64;
    let mut prev_ratio = -1.0_f64;

    for step in 0..=4 {
        let macro_val = step as f64 / 4.0;

        set_macro_via_ext_state(ctx, 0, macro_val).await?;
        tokio::time::sleep(Duration::from_millis(300)).await;

        let thresh = target_fx.param(threshold_param.index).get().await?;
        let ratio = target_fx.param(ratio_param.index).get().await?;
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

    // ─── Clean up ────────────────────────────────────────────────────

    cleanup_ext_state(ctx).await;

    ctx.log("");
    ctx.log("=== TEST PASSED: FTS Macros autonomously drives ReaComp via timer + ExtState ===");

    Ok(())
}
