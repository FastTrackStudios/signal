//! REAPER integration test: open FTS Compressor GUI and verify it renders.
//!
//! Loads the compressor onto a track, opens the plugin window, waits for
//! a few frames, then checks the plugin is still alive (i.e. didn't crash).
//!
//! Run with: `cargo xtask reaper-test comp_gui --gui`

use reaper_test::reaper_test;
use std::time::Duration;

const FTS_COMP_CLAP: &str = "CLAP: FTS Compressor";

#[reaper_test(isolated)]
async fn comp_gui(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let transport = project.transport();

    let track = project.tracks().add("Comp GUI Test", None).await?;
    ctx.log("Created track");

    let fx = match track.fx_chain().add(FTS_COMP_CLAP).await {
        Ok(fx) => fx,
        Err(e) => {
            ctx.log(&format!("FAILED to add '{}': {:?}", FTS_COMP_CLAP, e));
            return Err(eyre::eyre!("Failed to add FX: {:?}", e));
        }
    };
    ctx.log("Plugin loaded");

    // Open the plugin GUI window
    ctx.log("Opening GUI...");
    fx.open_ui().await?;
    ctx.log("GUI open — waiting for frames to render...");

    // Give the GUI thread time to initialize wgpu and render
    tokio::time::sleep(Duration::from_secs(2)).await;

    // If the GUI crashed it would typically take down the plugin; verify it's still alive
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(
        fx_count, 1,
        "Plugin should still be on the chain after GUI open"
    );

    // CLAP parameter edits are delivered while REAPER's audio engine is running.
    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Verify a parameter round-trips through the now-open GUI.
    fx.param(0).set(0.5).await?;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let val = fx.param(0).get().await?;
    assert!(
        (val - 0.5).abs() < 0.05,
        "Threshold param should round-trip while GUI is open, got {val:.4}"
    );
    ctx.log(&format!("Param round-trip OK (got {val:.4})"));

    // Close the GUI
    fx.open_ui().await.ok(); // toggle off — ignore errors
    ctx.log("GUI closed");
    transport.stop().await?;

    ctx.log("=== PASS: GUI opened and rendered without crashing ===");
    Ok(())
}
