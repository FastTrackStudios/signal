//! Integration tests for the Transport service.
//!
//! Each test exercises a single transport action against a live REAPER
//! instance, verifying that every method in the `TransportService` trait
//! works end-to-end through the vox RPC layer.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- reaper_transport

use daw::test::reaper_test;
use daw_proto::PlayState;

// ─── Playback Control ───────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn transport_play(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.play().await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(
        transport.is_playing().await?,
        "should be playing after play()"
    );

    transport.stop().await?;
    Ok(())
}

#[reaper_test(isolated)]
async fn transport_pause(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.play().await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    transport.pause().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let state = transport.get_play_state().await?;
    assert_eq!(state, PlayState::Paused, "should be paused after pause()");

    transport.stop().await?;
    Ok(())
}

#[reaper_test(isolated)]
async fn transport_stop(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.play().await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    transport.stop().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(
        !transport.is_playing().await?,
        "should not be playing after stop()"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_play_pause_toggle(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    // First toggle: stopped → playing
    transport.play_pause().await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        transport.is_playing().await?,
        "first toggle should start playing"
    );

    // Second toggle: playing → paused
    transport.play_pause().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let state = transport.get_play_state().await?;
    assert_eq!(state, PlayState::Paused, "second toggle should pause");

    transport.stop().await?;
    Ok(())
}

#[reaper_test(isolated)]
async fn transport_play_stop_toggle(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    // First toggle: stopped → playing
    transport.play_stop().await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        transport.is_playing().await?,
        "first toggle should start playing"
    );

    // Second toggle: playing → stopped
    transport.play_stop().await?;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!transport.is_playing().await?, "second toggle should stop");

    Ok(())
}

// ─── Position Control ───────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn transport_set_and_get_position(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.set_position(5.0).await?;
    let pos = transport.get_position().await?;
    assert!(
        (pos - 5.0).abs() < 0.1,
        "position should be ~5.0s, got {pos}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_goto_start(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.set_position(10.0).await?;
    transport.goto_start().await?;
    let pos = transport.get_position().await?;
    assert!(
        pos < 0.1,
        "position should be ~0 after goto_start(), got {pos}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_goto_end(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.goto_start().await?;
    let start_pos = transport.get_position().await?;
    transport.goto_end().await?;
    let end_pos = transport.get_position().await?;

    assert!(
        end_pos >= start_pos,
        "end position ({end_pos}) should be >= start ({start_pos})"
    );

    Ok(())
}

// ─── State Queries ──────────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn transport_get_state(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    let state = transport.get_state().await?;
    // Freshly created project should be stopped
    assert_eq!(
        state.play_state,
        PlayState::Stopped,
        "fresh project should be stopped"
    );
    assert!(state.tempo.bpm > 0.0, "tempo should be positive");

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_get_play_state(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    let state = transport.get_play_state().await?;
    assert_eq!(state, PlayState::Stopped, "should start stopped");

    transport.play().await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let state = transport.get_play_state().await?;
    assert_eq!(state, PlayState::Playing, "should be playing");

    transport.stop().await?;
    Ok(())
}

#[reaper_test(isolated)]
async fn transport_is_playing(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    assert!(
        !transport.is_playing().await?,
        "should not be playing initially"
    );

    transport.play().await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        transport.is_playing().await?,
        "should be playing after play()"
    );

    transport.stop().await?;
    assert!(
        !transport.is_playing().await?,
        "should not be playing after stop()"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_is_recording(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    assert!(
        !transport.is_recording().await?,
        "should not be recording initially"
    );

    Ok(())
}

// ─── Tempo Control ──────────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn transport_get_tempo(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    let bpm = transport.get_tempo().await?;
    assert!(bpm > 0.0, "tempo should be positive, got {bpm}");

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_set_tempo(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    let original = transport.get_tempo().await?;

    transport.set_tempo(140.0).await?;
    let bpm = transport.get_tempo().await?;
    assert!(
        (bpm - 140.0).abs() < 0.5,
        "tempo should be ~140 BPM, got {bpm}"
    );

    // Restore original tempo
    transport.set_tempo(original).await?;

    Ok(())
}

// ─── Loop Control ───────────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn transport_set_loop(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.set_loop(true).await?;
    assert!(transport.is_looping().await?, "loop should be enabled");

    transport.set_loop(false).await?;
    assert!(!transport.is_looping().await?, "loop should be disabled");

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_toggle_loop(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    let initial = transport.is_looping().await?;
    transport.toggle_loop().await?;
    let toggled = transport.is_looping().await?;
    assert_ne!(initial, toggled, "toggle_loop should flip the state");

    // Restore
    transport.toggle_loop().await?;

    Ok(())
}

// ─── Playrate Control ───────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn transport_get_playrate(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    let rate = transport.get_playrate().await?;
    assert!(
        (rate - 1.0).abs() < 0.01,
        "default playrate should be 1.0, got {rate}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn transport_set_playrate(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    transport.set_playrate(0.5).await?;
    let rate = transport.get_playrate().await?;
    assert!(
        (rate - 0.5).abs() < 0.05,
        "playrate should be ~0.5, got {rate}"
    );

    // Restore
    transport.set_playrate(1.0).await?;

    Ok(())
}

// ─── Time Signature ─────────────────────────────────────────────────────

#[reaper_test(isolated)]
async fn transport_get_time_signature(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    let ts = transport.get_time_signature().await?;
    assert!(ts.numerator > 0, "numerator should be positive");
    assert!(ts.denominator > 0, "denominator should be positive");

    Ok(())
}

// Musical-position navigation (set_position_musical, goto_measure)
// and subscribe_state retired alongside the architect::rpc port — the
// sync `Transport` trait carries only the canonical verbs. Tests
// covering those surfaces will return on a sibling trait.
