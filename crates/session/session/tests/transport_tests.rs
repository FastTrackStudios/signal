//! REAPER integration tests for transport/playback.
//!
//! Verifies that the DAW transport API works correctly with a stamped
//! demo setlist: play, pause, stop, position queries, and state transitions.
//!
//! Run with:
//!   cargo xtask reaper-test -- transport

use daw::test::reaper_test;
use session::stamp_demo_into_project;
use std::time::Duration;

/// Helper: stamp demo data and wait for it to settle.
async fn setup_demo(project: &daw::rpc::Project) -> eyre::Result<()> {
    stamp_demo_into_project(project).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}

/// Verify transport starts in stopped state with position at 0.
#[reaper_test]
async fn transport_initial_state(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project.transport();

    let is_playing = transport.is_playing().await?;
    eyre::ensure!(!is_playing, "Transport should not be playing initially");

    let pos = transport.get_position().await?;
    println!("Initial position: {pos:.4}s");
    eyre::ensure!(pos < 1.0, "Initial position should be near 0, got {pos:.2}");

    Ok(())
}

/// Verify play starts transport and stop returns to stopped state.
#[reaper_test]
async fn transport_play_stop(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    setup_demo(&ctx.project).await?;
    let transport = ctx.project.transport();

    // Ensure stopped first
    transport.stop().await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Play
    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let is_playing = transport.is_playing().await?;
    eyre::ensure!(is_playing, "Transport should be playing after play()");

    let pos = transport.get_position().await?;
    println!("Position after 300ms of playback: {pos:.4}s");
    eyre::ensure!(pos > 0.0, "Position should have advanced, got {pos:.4}");

    // Stop
    transport.stop().await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let is_playing = transport.is_playing().await?;
    eyre::ensure!(!is_playing, "Transport should be stopped after stop()");

    Ok(())
}

/// Verify pause freezes the playhead and resume continues from the same position.
#[reaper_test]
async fn transport_pause_resume(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    setup_demo(&ctx.project).await?;
    let transport = ctx.project.transport();

    // Start from a known position
    transport.stop().await?;
    transport.set_position(10.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Play for a bit
    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Pause
    transport.pause().await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let paused_pos = transport.get_position().await?;
    println!("Position when paused: {paused_pos:.4}s");
    eyre::ensure!(
        paused_pos > 10.0,
        "Paused position should be past 10s, got {paused_pos:.2}"
    );

    let is_playing = transport.is_playing().await?;
    eyre::ensure!(!is_playing, "Transport should not be playing when paused");

    // Wait and verify position doesn't move while paused
    tokio::time::sleep(Duration::from_millis(300)).await;
    let still_paused_pos = transport.get_position().await?;
    let drift = (still_paused_pos - paused_pos).abs();
    println!("Position after 300ms paused: {still_paused_pos:.4}s (drift: {drift:.4}s)");
    eyre::ensure!(
        drift < 0.1,
        "Position should not move while paused, drifted {drift:.4}s"
    );

    // Resume
    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let resumed_pos = transport.get_position().await?;
    println!("Position after resume: {resumed_pos:.4}s");
    eyre::ensure!(
        resumed_pos > paused_pos,
        "Position should advance after resume: {resumed_pos:.2} > {paused_pos:.2}"
    );

    // Clean up
    transport.stop().await?;
    Ok(())
}

/// Verify `set_position` moves the playhead to the requested location.
#[reaper_test]
async fn transport_seek(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    setup_demo(&ctx.project).await?;
    let transport = ctx.project.transport();

    transport.stop().await?;

    // Seek to various positions within the demo song
    let targets = [0.0, 34.0, 109.0, 200.0];
    for &target in &targets {
        transport.set_position(target).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let pos = transport.get_position().await?;
        let diff = (pos - target).abs();
        println!("Seek to {target:.1}s → got {pos:.4}s (diff: {diff:.4}s)");
        eyre::ensure!(
            diff < 0.5,
            "Position should be near {target:.1}s, got {pos:.2}s"
        );
    }

    Ok(())
}

/// Verify playhead advances through demo song sections during playback.
#[reaper_test]
async fn transport_playback_advances(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    setup_demo(&ctx.project).await?;
    let transport = ctx.project.transport();

    // Start from the Intro section (4.0s = SONGSTART)
    transport.stop().await?;
    transport.set_position(4.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    transport.play().await?;

    // Sample positions over 2 seconds to verify advancement
    let mut positions = Vec::new();
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let pos = transport.get_position().await?;
        positions.push(pos);
    }

    transport.stop().await?;

    println!("Sampled positions during playback:");
    for (i, pos) in positions.iter().enumerate() {
        println!("  sample {i}: {pos:.4}s");
    }

    // Verify positions are monotonically increasing
    for w in positions.windows(2) {
        eyre::ensure!(
            w[1] > w[0],
            "Playhead should advance: {:.4}s should be > {:.4}s",
            w[1],
            w[0]
        );
    }

    // Verify we advanced at least ~1.5s over 2s of playback
    let total_advance = positions.last().unwrap() - positions.first().unwrap();
    println!("Total advance: {total_advance:.4}s");
    eyre::ensure!(
        total_advance > 1.0,
        "Should advance at least 1s during 2s of playback, got {total_advance:.2}s"
    );

    Ok(())
}

/// Verify `get_state` returns comprehensive transport information.
#[reaper_test]
async fn transport_state_query(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    setup_demo(&ctx.project).await?;
    let transport = ctx.project.transport();

    // Check state while stopped
    transport.stop().await?;
    transport.set_position(20.0).await?; // Verse 1 start (Song 1)
    tokio::time::sleep(Duration::from_millis(100)).await;

    let state = transport.get_state().await?;
    println!("Transport state (stopped at 34s):");
    println!("  play_state: {:?}", state.play_state);
    println!("  tempo: {:.1} BPM", state.tempo.bpm());
    println!(
        "  time_sig: {}/{}",
        state.time_signature.numerator, state.time_signature.denominator
    );

    eyre::ensure!(!state.is_playing(), "Should not be playing when stopped");

    // Check state while playing
    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let state = transport.get_state().await?;
    let pos = transport.get_position().await?;
    println!("\nTransport state (playing):");
    println!("  play_state: {:?}", state.play_state);
    println!("  position: {pos:.4}s");
    println!("  tempo: {:.1} BPM", state.tempo.bpm());

    eyre::ensure!(state.is_playing(), "Should be playing after play()");
    eyre::ensure!(
        pos > 20.0,
        "Position should have advanced past 20s, got {pos:.2}"
    );

    transport.stop().await?;
    Ok(())
}
