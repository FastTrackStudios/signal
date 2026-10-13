//! REAPER integration tests for edge cases.
//!
//! Tests `SongBuilder` with empty projects, partial markers, and transport
//! seek edge cases (negative position, past project end, exact boundaries).
//!
//! Run with:
//!   cargo xtask reaper-test -- `edge_case`

use daw::rpc::Project;
use daw::test::reaper_test;
use session::{SongBuilder, stamp_demo_into_project};
use std::time::Duration;

/// Remove all markers and regions from a project so each test starts clean.
async fn clear_project(project: &Project) -> eyre::Result<()> {
    let markers = project.markers().all().await?;
    for m in &markers {
        if let Some(id) = m.id {
            project.markers().remove(id).await?;
        }
    }
    let regions = project.regions().all().await?;
    for r in &regions {
        if let Some(id) = r.id {
            project.regions().remove(id).await?;
        }
    }
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════
//  SongBuilder edge cases
// ═════════════════════════════════════════════════════════════════════

/// `SongBuilder` on an empty project (no markers, no regions).
#[reaper_test]
async fn songbuilder_empty_project(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    clear_project(&ctx.project).await?;
    // Project is now empty
    let songs = SongBuilder::build(&ctx.project).await?;

    println!("Songs from empty project: {}", songs.len());
    if songs.is_empty() {
        println!("  (no songs — expected for empty project)");
    } else {
        let song = &songs[0];
        println!(
            "  Song: {} ({:.2}s - {:.2}s)",
            song.name, song.start_seconds, song.end_seconds
        );
        println!("  Sections: {}", song.sections.len());
    }

    // Either no songs or a degenerate song is acceptable
    // The key test is that SongBuilder doesn't panic or error
    Ok(())
}

/// `SongBuilder` with only structural markers (no regions).
/// Should build sections from marker boundaries.
#[reaper_test]
async fn songbuilder_markers_only(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let markers_api = ctx.project.markers();

    // Add structural markers without any regions
    markers_api.add(0.0, "COUNT-IN").await?;
    markers_api.add(4.0, "SONGSTART").await?;
    // Add section markers (SongBuilder should create sections from these)
    markers_api.add(4.0, "Intro").await?;
    markers_api.add(20.0, "Verse").await?;
    markers_api.add(40.0, "Chorus").await?;
    markers_api.add(60.0, "SONGEND").await?;
    markers_api.add(65.0, "=END").await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let songs = SongBuilder::build(&ctx.project).await?;
    println!("Songs from markers-only project: {}", songs.len());

    if songs.is_empty() {
        return Err(eyre::eyre!("Should produce at least 1 song from markers"));
    }

    let song = &songs[0];
    println!(
        "  Song: {} ({:.2}s - {:.2}s)",
        song.name, song.start_seconds, song.end_seconds
    );
    println!("  Sections ({}):", song.sections.len());
    for (i, s) in song.sections.iter().enumerate() {
        println!(
            "    [{i}] {} ({:?}) {:.2}s - {:.2}s",
            s.name, s.section_type, s.start_seconds, s.end_seconds
        );
    }

    if song.sections.len() < 2 {
        return Err(eyre::eyre!("Should have at least 2 sections from markers"));
    }

    Ok(())
}

/// `SongBuilder` with regions but no structural markers (no SONGSTART/SONGEND).
/// Should still produce a song and detect at least some sections from regions.
#[reaper_test]
async fn songbuilder_regions_no_structural_markers(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let regions_api = ctx.project.regions();

    // Add regions without any SONGSTART/SONGEND markers
    regions_api.add(0.0, 30.0, "Intro").await?;
    regions_api.add(30.0, 60.0, "Verse").await?;
    regions_api.add(60.0, 90.0, "Chorus").await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let songs = SongBuilder::build(&ctx.project).await?;
    println!("Songs from regions-only project: {}", songs.len());

    if songs.is_empty() {
        return Err(eyre::eyre!("Should produce at least 1 song from regions"));
    }

    let song = &songs[0];
    println!(
        "  Song: {} ({:.2}s - {:.2}s)",
        song.name, song.start_seconds, song.end_seconds
    );
    println!("  Sections ({}):", song.sections.len());
    for (i, s) in song.sections.iter().enumerate() {
        println!(
            "    [{i}] {} ({:?}) {:.2}s - {:.2}s",
            s.name, s.section_type, s.start_seconds, s.end_seconds
        );
    }

    // Without structural markers, SongBuilder uses heuristics to determine
    // song bounds. It should produce at least some sections from the regions.
    if song.sections.is_empty() {
        return Err(eyre::eyre!("Should have at least 1 section from regions"));
    }

    // The song should span at least part of the region range
    if song.end_seconds <= song.start_seconds {
        return Err(eyre::eyre!("Song should have positive duration"));
    }

    Ok(())
}

/// `SongBuilder` with SONGSTART but no SONGEND.
#[reaper_test]
async fn songbuilder_missing_songend(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let markers_api = ctx.project.markers();
    let regions_api = ctx.project.regions();

    markers_api.add(4.0, "SONGSTART").await?;
    // No SONGEND
    regions_api.add(4.0, 30.0, "Intro").await?;
    regions_api.add(30.0, 60.0, "Verse").await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let songs = SongBuilder::build(&ctx.project).await?;
    println!("Songs with missing SONGEND: {}", songs.len());

    // Should not crash — SongBuilder handles missing markers gracefully
    if !songs.is_empty() {
        let song = &songs[0];
        println!(
            "  Song: {} ({:.2}s - {:.2}s)",
            song.name, song.start_seconds, song.end_seconds
        );
        println!("  Sections: {}", song.sections.len());
    }

    Ok(())
}

/// `SongBuilder` with only SONGSTART and SONGEND (no section regions/markers).
#[reaper_test]
async fn songbuilder_structural_only(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let markers_api = ctx.project.markers();

    markers_api.add(0.0, "COUNT-IN").await?;
    markers_api.add(4.0, "SONGSTART").await?;
    markers_api.add(60.0, "SONGEND").await?;
    markers_api.add(65.0, "=END").await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let songs = SongBuilder::build(&ctx.project).await?;
    if songs.is_empty() {
        return Err(eyre::eyre!("Should produce a song"));
    }

    let song = &songs[0];
    println!("Song from structural markers only:");
    println!(
        "  start={:.2}s end={:.2}s count_in={:?}",
        song.start_seconds, song.end_seconds, song.count_in_seconds
    );
    println!("  Sections: {}", song.sections.len());
    for (i, s) in song.sections.iter().enumerate() {
        println!(
            "    [{i}] {} ({:?}) {:.2}s - {:.2}s",
            s.name, s.section_type, s.start_seconds, s.end_seconds
        );
    }

    // Count-in detection: SongBuilder may or may not detect count-in
    // depending on whether it's looking at lane-specific markers.
    if let Some(ci) = song.count_in_seconds {
        println!("  Count-in detected: {ci:.2}s");
    } else {
        println!("  Count-in not detected (markers not in MARKS lane)");
    }

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════
//  Transport seek edge cases
// ═════════════════════════════════════════════════════════════════════

/// Seek to position 0 (project start).
#[reaper_test]
async fn seek_to_zero(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project.transport();

    transport.stop().await?;
    transport.set_position(100.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    transport.set_position(0.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pos = transport.get_position().await?;
    println!("Position after seek to 0: {pos:.4}s");
    if pos.abs() >= 0.5 {
        return Err(eyre::eyre!("Should be at 0, got {pos:.2}"));
    }

    Ok(())
}

/// Seek to a very large position (past any content).
#[reaper_test]
async fn seek_past_end(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    stamp_demo_into_project(&ctx.project).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let transport = ctx.project.transport();
    transport.stop().await?;

    // Seek way past the last song end (430s)
    transport.set_position(1000.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pos = transport.get_position().await?;
    println!("Position after seek to 1000s: {pos:.2}s");

    // REAPER allows seeking past content — position should be at or near 1000s
    if pos <= 500.0 {
        return Err(eyre::eyre!(
            "Should have seeked to a large position, got {pos:.2}s"
        ));
    }

    Ok(())
}

/// Seek to exact section boundary positions.
#[reaper_test]
async fn seek_exact_boundaries(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    stamp_demo_into_project(&ctx.project).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let transport = ctx.project.transport();
    transport.stop().await?;

    // Song 1 section boundaries: Intro 4, Verse 1 20, Chorus 44, Verse 2 68, Chorus 92, Outro 112, SONGEND 116, =END 120
    let boundaries = [0.0, 4.0, 20.0, 44.0, 68.0, 92.0, 112.0, 116.0, 120.0];

    for &boundary in &boundaries {
        transport.set_position(boundary).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let pos = transport.get_position().await?;
        let diff = (pos - boundary).abs();
        println!("Boundary {boundary:.1}s → {pos:.4}s (diff: {diff:.4}s)");
        if diff >= 0.5 {
            return Err(eyre::eyre!(
                "Seek to boundary {boundary:.1}s failed: got {pos:.2}s"
            ));
        }
    }

    Ok(())
}

/// `goto_start` resets to position 0.
#[reaper_test]
async fn transport_goto_start(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project.transport();

    transport.stop().await?;
    transport.set_position(50.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    transport.goto_start().await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pos = transport.get_position().await?;
    println!("Position after goto_start: {pos:.4}s");
    if pos >= 1.0 {
        return Err(eyre::eyre!("goto_start should go to ~0, got {pos:.2}s"));
    }

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════
//  Rapid state transitions
// ═════════════════════════════════════════════════════════════════════

/// Rapid play/stop/play — no stale state.
#[reaper_test]
async fn rapid_play_stop_play(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    stamp_demo_into_project(&ctx.project).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let transport = ctx.project.transport();
    transport.stop().await?;
    transport.set_position(10.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Rapid transitions
    for i in 0..5 {
        transport.play().await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        transport.stop().await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        println!("  Rapid cycle {i}: play→stop");
    }

    // Final state should be stopped
    let is_playing = transport.is_playing().await?;
    if is_playing {
        return Err(eyre::eyre!("Should be stopped after rapid transitions"));
    }

    // One more play to confirm it still works
    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let is_playing = transport.is_playing().await?;
    if !is_playing {
        return Err(eyre::eyre!("Should be playing after final play()"));
    }

    let pos = transport.get_position().await?;
    println!("Position after rapid transitions: {pos:.2}s");
    if pos <= 0.0 {
        return Err(eyre::eyre!("Should have advanced"));
    }

    transport.stop().await?;
    Ok(())
}

/// Rapid seek across sections.
#[reaper_test]
async fn rapid_seek(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    stamp_demo_into_project(&ctx.project).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let transport = ctx.project.transport();
    transport.stop().await?;

    // Seek rapidly to many positions
    let positions = [0.0, 100.0, 50.0, 200.0, 10.0, 244.0, 4.0];
    for &target in &positions {
        transport.set_position(target).await?;
    }
    // Small settle time after all seeks
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Final position should be the last seek target
    let pos = transport.get_position().await?;
    let diff = (pos - 4.0).abs();
    println!("Position after rapid seeks: {pos:.4}s (expected ~4.0s, diff: {diff:.4}s)");
    if diff >= 0.5 {
        return Err(eyre::eyre!(
            "Should be at last seek position (4.0s), got {pos:.2}s"
        ));
    }

    Ok(())
}

/// Play/pause/play maintains position continuity.
#[reaper_test]
async fn rapid_play_pause_play(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    stamp_demo_into_project(&ctx.project).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let transport = ctx.project.transport();
    transport.stop().await?;
    transport.set_position(10.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Pause/resume rapidly
    for _ in 0..3 {
        transport.pause().await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        transport.play().await?;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let pos = transport.get_position().await?;
    transport.stop().await?;

    println!("Position after rapid pause/play: {pos:.2}s");
    // Should have advanced past 10s (we played for ~1.2s total across cycles)
    if pos <= 10.5 {
        return Err(eyre::eyre!(
            "Position should advance through pause/play cycles, got {pos:.2}s"
        ));
    }

    Ok(())
}
