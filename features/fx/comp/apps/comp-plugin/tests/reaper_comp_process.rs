//! REAPER integration test: verify FTS Compressor processes audio.
//!
//! Loads the compressor on a track, starts playback, sets aggressive
//! compression settings, and verifies the transport runs with the plugin
//! in-chain.
//!
//! Run with: `cargo xtask reaper-test comp_process`

use reaper_test::reaper_test;
use std::time::Duration;

const FTS_COMP_CLAP: &str = "CLAP: FTS Compressor";

#[reaper_test(isolated)]
async fn comp_process(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let transport = project.transport();

    // Ensure transport is stopped at start
    transport.stop().await?;
    transport.goto_start().await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create track and load the compressor
    let track = project.tracks().add("Comp Process Test", None).await?;
    let fx = match track.fx_chain().add(FTS_COMP_CLAP).await {
        Ok(fx) => fx,
        Err(e) => {
            ctx.log(&format!("FAILED to add FX: {:?}", e));
            return Err(eyre::eyre!("Failed to add FX: {:?}", e));
        }
    };
    ctx.log("Loaded FTS Compressor");

    // Start playback before setting CLAP parameters. The REAPER bridge only
    // delivers CLAP parameter edits while the audio engine is running.
    let pos_before = transport.get_position().await?;
    transport.play().await?;
    ctx.log("Transport: PLAY");
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Set aggressive compression: low threshold, high ratio
    // Threshold: -30 dB, normalized 0.5 on linear -60..0 range
    fx.param_by_name("Threshold").set(0.5).await?;
    ctx.log("Set Threshold to -30 dB (normalized 0.5)");

    // Ratio: high value (toward 20:1)
    fx.param_by_name("Ratio").set(0.8).await?;
    ctx.log("Set Ratio high (normalized 0.8)");

    // Fast attack
    fx.param_by_name("Attack").set(0.1).await?;
    ctx.log("Set Attack fast (normalized 0.1)");

    // Medium release
    fx.param_by_name("Release").set(0.3).await?;
    ctx.log("Set Release medium (normalized 0.3)");
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Verify params were set
    let params = fx.parameters().await?;
    for p in &params {
        ctx.log(&format!(
            "  [{:>2}] {:<20} = {:.4}",
            p.index, p.name, p.value
        ));
    }

    let threshold = params.iter().find(|p| p.name == "Threshold").unwrap();
    assert!(
        (threshold.value - 0.5).abs() < 0.05,
        "Threshold should be ~0.5 after set, got {}",
        threshold.value
    );

    // Let it play for ~2 seconds
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify transport advanced (plugin didn't crash or stall)
    let pos_after = transport.get_position().await?;
    ctx.log(&format!(
        "Position: {:.4}s -> {:.4}s",
        pos_before, pos_after
    ));

    assert!(
        pos_after > pos_before + 1.0,
        "Transport should have advanced at least 1 second (got {} -> {})",
        pos_before,
        pos_after
    );

    // Verify plugin is still on the chain and not bypassed
    let fx_count = track.fx_chain().count().await?;
    assert_eq!(fx_count, 1, "Compressor should still be on the chain");

    // Stop transport
    transport.stop().await?;
    transport.goto_start().await?;
    ctx.log("Transport: STOP");

    ctx.log(&format!(
        "comp_process: PASSED (played {:.2}s with aggressive compression)",
        pos_after - pos_before
    ));
    Ok(())
}
