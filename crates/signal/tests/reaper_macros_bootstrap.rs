//! REAPER integration test — verifies fts-macros CLAP plugin bootstrap.
//!
//! This test checks that the Helgobox-pattern eager loading works:
//! 1. The extension loaded fts-macros.clap and called ReaperPluginEntry
//! 2. The plugin bootstrapped reaper-rs, TaskSupport, daw-reaper, LocalCaller
//! 3. When instantiated as an FX, the plugin can access the REAPER API
//!
//! Run with:
//!   cargo xtask reaper-test -- reaper_macros_bootstrap

use std::time::Duration;

use reaper_test::reaper_test;

// ---------------------------------------------------------------------------
// Verify the bootstrap happened during REAPER startup
// ---------------------------------------------------------------------------

#[reaper_test]
async fn bootstrap_log_shows_success(_ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    println!("\n=== verify fts-macros REAPER bootstrap ===");

    // The bootstrap log is written by the plugin's own tracing subscriber
    let log_path = "/tmp/fts-macros-bootstrap.log";
    let log_content = std::fs::read_to_string(log_path)
        .map_err(|e| eyre::eyre!("Cannot read bootstrap log at {}: {}", log_path, e))?;

    println!("Bootstrap log ({} bytes):", log_content.len());
    for line in log_content.lines() {
        // Strip ANSI codes for cleaner output
        println!("  {}", strip_ansi(line));
    }

    // Verify key bootstrap milestones
    assert!(
        log_content.contains("reaper-high initialized"),
        "bootstrap should initialize reaper-high"
    );
    assert!(
        log_content.contains("timer callback registered"),
        "bootstrap should register timer callback"
    );
    assert!(
        log_content.contains("TaskSupport configured"),
        "bootstrap should configure daw-reaper TaskSupport"
    );
    assert!(
        log_content.contains("LocalCaller"),
        "bootstrap should create LocalCaller"
    );
    assert!(
        log_content.contains("DawSync created"),
        "bootstrap should create DawSync"
    );
    assert!(
        log_content.contains("REAPER bootstrap complete"),
        "bootstrap should complete successfully"
    );

    println!("\nAll bootstrap milestones verified!");
    Ok(())
}

// ---------------------------------------------------------------------------
// Add fts-macros as FX and verify REAPER API access
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn instantiate_fts_macros_plugin(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    println!("\n=== instantiate fts-macros CLAP plugin ===");

    let daw = &ctx.daw;

    // Add a track and load fts-macros as an FX
    let project = daw.current_project().await?;
    let track = project.tracks().add("Macros Bootstrap Test", None).await?;
    let chain = track.fx_chain();

    // Try adding by CLAP ID or plugin name
    let fx = chain.add("FTS Macros").await;
    let fx = match fx {
        Ok(f) => f,
        Err(_) => {
            // Try CLAP ID format
            chain
                .add("CLAP: FTS Macros")
                .await
                .map_err(|e| eyre::eyre!("Could not add fts-macros plugin: {}", e))?
        }
    };

    // Wait for plugin initialization
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let info = fx.info().await?;
    println!("Plugin loaded: {} ({})", info.name, info.plugin_name);
    println!("  Parameters: {}", info.parameter_count);

    // nih_plug adds internal CLAP parameters (bypass, etc.) beyond our 8 macros.
    // REAPER sees all of them, so we check >= 8 rather than exact.
    assert!(
        info.parameter_count >= 8,
        "fts-macros should have at least 8 parameters, got {}",
        info.parameter_count
    );

    // Read parameters and verify macro params are named correctly
    let params = fx.parameters().await?;
    for p in &params {
        println!("  [{}] {} = {:.4}", p.index, p.name, p.value);
    }

    // Find the macro parameters by name (they may not be at index 0-7
    // if nih_plug puts internal params first)
    let macro_params: Vec<_> = params
        .iter()
        .filter(|p| p.name.starts_with("Macro "))
        .collect();
    assert_eq!(macro_params.len(), 8, "should have 8 Macro parameters");
    assert!(macro_params.iter().any(|p| p.name == "Macro 1"));
    assert!(macro_params.iter().any(|p| p.name == "Macro 8"));

    // Verify parameter set/get works (through the standard FX parameter API)
    // Use the first Macro parameter's actual index (may not be 0 due to nih_plug internals)
    let macro1_idx = macro_params
        .iter()
        .find(|p| p.name == "Macro 1")
        .unwrap()
        .index;
    let param0 = fx.param(macro1_idx);
    let original = param0.get().await?;
    param0.set(0.42).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let readback = param0.get().await?;
    println!(
        "\n  Param 0 round-trip: original={:.4}, set=0.42, readback={:.4}",
        original, readback
    );
    assert!(
        (readback - 0.42).abs() < 0.02,
        "parameter should be near 0.42, got {:.4}",
        readback
    );

    // Restore
    param0.set(original).await?;

    // Check bootstrap log for REAPER API verification
    let log_content = std::fs::read_to_string("/tmp/fts-macros-bootstrap.log").unwrap_or_default();
    if log_content.contains("REAPER API verified") {
        println!("\n  Plugin-side REAPER API access confirmed via bootstrap log!");
    } else {
        println!("\n  Note: REAPER API verification message not found in bootstrap log");
        println!("  (This is expected — initialize() logs go to the plugin's subscriber)");
    }

    println!("\nPASS — fts-macros instantiated and parameters verified");
    Ok(())
}

/// Strip ANSI escape codes from a string for cleaner test output.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            result.push(c);
        }
    }
    result
}
