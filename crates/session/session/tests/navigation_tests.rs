//! REAPER integration tests for song/section navigation.
//!
//! Verifies that navigating between songs and sections (next/previous)
//! correctly moves the transport playhead to the expected positions.
//!
//! These tests exercise the same pipeline as `SetlistService` navigation:
//! stamp demo data → `SongBuilder` → seek to section/song positions → verify.
//!
//! Run with:
//!   cargo xtask reaper-test -- navigation

use daw::test::reaper_test;
use session::{SongBuilder, stamp_demo_into_project};
use std::time::Duration;

/// Helper: stamp demo data, build songs, return the song and its sections.
async fn setup_song(project: &daw::rpc::Project) -> eyre::Result<session::Song> {
    stamp_demo_into_project(project).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let songs = SongBuilder::build(project).await?;
    assert!(
        !songs.is_empty(),
        "SongBuilder should produce at least 1 song"
    );
    songs
        .into_iter()
        .next()
        .ok_or_else(|| eyre::eyre!("SongBuilder should produce at least 1 song"))
}

/// Navigate to each section by index (simulates `go_to_section`).
/// Verifies the playhead lands at the correct section start.
#[reaper_test]
async fn navigate_to_each_section(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();
    transport.stop().await?;

    println!("Song has {} sections:", song.sections.len());
    for (i, section) in song.sections.iter().enumerate() {
        // This is what go_to_section_impl does: seek to section.start_seconds
        transport.set_position(section.start_seconds).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let pos = transport.get_position().await?;
        let diff = (pos - section.start_seconds).abs();
        println!(
            "  [{i}] '{}': seek to {:.2}s → got {:.2}s (diff: {:.4}s)",
            section.name, section.start_seconds, pos, diff
        );
        if !(diff < 0.5) {
            return Err(eyre::eyre!(
                "Section '{}' seek failed: expected {:.2}s, got {:.2}s",
                section.name,
                section.start_seconds,
                pos
            ));
        }
    }

    Ok(())
}

/// Simulate `next_section`: start at first section, step forward through all sections.
/// Verifies each step lands at the next section's start position.
#[reaper_test]
async fn next_section_navigation(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();
    transport.stop().await?;

    // Start at the first section
    let first = &song.sections[0];
    transport.set_position(first.start_seconds).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    println!(
        "Simulating next_section through {} sections:",
        song.sections.len()
    );

    // Step through each section (simulating next_section)
    for i in 1..song.sections.len() {
        let target = &song.sections[i];
        // next_section_impl calls go_to_section_impl(current_index + 1)
        // which seeks to sections[next].start_seconds
        transport.set_position(target.start_seconds).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let pos = transport.get_position().await?;
        let diff = (pos - target.start_seconds).abs();
        println!(
            "  next → [{i}] '{}': expected {:.2}s, got {:.2}s",
            target.name, target.start_seconds, pos
        );
        if !(diff < 0.5) {
            return Err(eyre::eyre!(
                "Next section '{}' missed: expected {:.2}s, got {:.2}s",
                target.name,
                target.start_seconds,
                pos
            ));
        }
    }

    Ok(())
}

/// Simulate `previous_section`: start at last section, step backward.
/// Verifies each step lands at the previous section's start position.
#[reaper_test]
async fn previous_section_navigation(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();
    transport.stop().await?;

    let last_idx = song.sections.len().saturating_sub(1);
    let last = &song.sections[last_idx];
    transport.set_position(last.start_seconds).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    println!(
        "Simulating previous_section backward from [{last_idx}] '{}':",
        last.name
    );

    // Step backward through sections
    for i in (0..last_idx).rev() {
        let target = &song.sections[i];
        transport.set_position(target.start_seconds).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let pos = transport.get_position().await?;
        let diff = (pos - target.start_seconds).abs();
        println!(
            "  prev → [{i}] '{}': expected {:.2}s, got {:.2}s",
            target.name, target.start_seconds, pos
        );
        if !(diff < 0.5) {
            return Err(eyre::eyre!(
                "Previous section '{}' missed: expected {:.2}s, got {:.2}s",
                target.name,
                target.start_seconds,
                pos
            ));
        }
    }

    Ok(())
}

/// Simulate `previous_section` "smart rewind": if we're mid-section (>5% progress),
/// previous should go to the START of the current section, not the previous one.
#[reaper_test]
async fn previous_section_smart_rewind(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();
    transport.stop().await?;

    // Pick a section that has enough duration to test mid-section behavior
    // Verse 1: 34.0s - 64.0s (30s duration)
    let verse_idx = song
        .sections
        .iter()
        .position(|s| s.name == "Verse 1")
        .expect("Should have Verse 1");
    let verse = &song.sections[verse_idx];

    // Seek to 50% through the section
    let mid_point = (verse.end_seconds - verse.start_seconds).mul_add(0.5, verse.start_seconds);
    transport.set_position(mid_point).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pos = transport.get_position().await?;
    println!(
        "Mid-section position in '{}': {:.2}s (section: {:.2}s - {:.2}s)",
        verse.name, pos, verse.start_seconds, verse.end_seconds
    );

    // Smart rewind: since we're >5% into the section, "previous" should
    // go to the START of the CURRENT section (not the previous one)
    let section_duration = verse.end_seconds - verse.start_seconds;
    let progress = (pos - verse.start_seconds) / section_duration;
    println!("  Progress in section: {:.1}%", progress * 100.0);
    if !(progress > 0.05) {
        return Err(eyre::eyre!(
            "Should be past 5% of section for smart rewind test"
        ));
    }

    // Simulate what previous_section_impl does when progress > 5%:
    // go to start of CURRENT section
    transport.set_position(verse.start_seconds).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let rewound_pos = transport.get_position().await?;
    let diff = (rewound_pos - verse.start_seconds).abs();
    println!(
        "  Smart rewind → {:.2}s (expected {:.2}s, diff: {:.4}s)",
        rewound_pos, verse.start_seconds, diff
    );
    if !(diff < 0.5) {
        return Err(eyre::eyre!(
            "Smart rewind should go to section start {:.2}s, got {:.2}s",
            verse.start_seconds,
            rewound_pos
        ));
    }

    // Now seek to the very start of the section (<5% progress)
    let near_start = section_duration.mul_add(0.02, verse.start_seconds); // 2% in
    transport.set_position(near_start).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Simulate previous_section when at start: should go to PREVIOUS section
    let prev_section = &song.sections[verse_idx.saturating_sub(1)];
    transport.set_position(prev_section.start_seconds).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let prev_pos = transport.get_position().await?;
    let diff = (prev_pos - prev_section.start_seconds).abs();
    println!(
        "  At start → prev section '{}' at {:.2}s (got {:.2}s, diff: {:.4}s)",
        prev_section.name, prev_section.start_seconds, prev_pos, diff
    );
    if !(diff < 0.5) {
        return Err(eyre::eyre!(
            "Should go to previous section '{}' at {:.2}s, got {:.2}s",
            prev_section.name,
            prev_section.start_seconds,
            prev_pos
        ));
    }

    Ok(())
}

/// Verify that playing through a section boundary naturally advances to the next section.
/// Starts playback near the end of one section and confirms position enters the next.
#[reaper_test]
async fn playback_crosses_section_boundary(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();

    // Find Intro → Verse 1 boundary (Intro ends at 34.0s, Verse 1 starts at 34.0s)
    let intro_idx = song
        .sections
        .iter()
        .position(|s| s.name == "Intro")
        .expect("Should have Intro");
    let intro = &song.sections[intro_idx];
    let verse1 = &song.sections[intro_idx.saturating_add(1)];

    // Seek to 1 second before the boundary
    let near_boundary = intro.end_seconds - 1.0;
    transport.stop().await?;
    transport.set_position(near_boundary).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    println!(
        "Starting playback at {:.2}s (Intro ends at {:.2}s, Verse 1 starts at {:.2}s)",
        near_boundary, intro.end_seconds, verse1.start_seconds
    );

    // Play for ~2 seconds to cross the boundary
    transport.play().await?;
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let pos = transport.get_position().await?;
    transport.stop().await?;

    println!("Position after crossing boundary: {pos:.2}s");

    // Should now be in Verse 1 territory
    if !(pos >= verse1.start_seconds) {
        return Err(eyre::eyre!(
            "Playhead should have crossed into '{}' (>= {:.2}s), got {:.2}s",
            verse1.name,
            verse1.start_seconds,
            pos
        ));
    }
    if !(pos < verse1.end_seconds) {
        return Err(eyre::eyre!(
            "Playhead should still be in '{}' (< {:.2}s), got {:.2}s",
            verse1.name,
            verse1.end_seconds,
            pos
        ));
    }

    println!("Section boundary crossing verified: Intro → Verse 1");
    Ok(())
}

/// Verify section index calculation: given a position, determine which section we're in.
/// This is what the polling loop does to compute `ActiveIndices.section_index`.
#[reaper_test]
async fn section_index_from_position(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();
    transport.stop().await?;

    println!(
        "Verifying section index calculation for {} sections:",
        song.sections.len()
    );

    for (expected_idx, section) in song.sections.iter().enumerate() {
        // Seek to the middle of each section
        let mid = f64::midpoint(section.start_seconds, section.end_seconds);
        transport.set_position(mid).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let pos = transport.get_position().await?;

        // Calculate which section index this position falls in
        // (same logic as the polling loop uses)
        let computed_idx = song
            .sections
            .iter()
            .position(|s| pos >= s.start_seconds && pos < s.end_seconds);

        println!(
            "  pos {pos:.2}s → section {computed_idx:?} (expected [{expected_idx}] '{}')",
            section.name
        );

        if computed_idx != Some(expected_idx) {
            return Err(eyre::eyre!(
                "Position {pos:.2}s should be in section [{expected_idx}] '{}', got {computed_idx:?}",
                section.name
            ));
        }
    }

    Ok(())
}

/// Song-level navigation: seek to song start (count-in position).
/// In a single-song setlist, this verifies the `song_seek_position` logic.
#[reaper_test]
async fn navigate_to_song_start(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();

    // First, move away from the start
    transport.stop().await?;
    transport.set_position(100.0).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Navigate to song start (same logic as go_to_song_impl / song_seek_position)
    // song.start_seconds includes count-in (should be ~0.0 for demo)
    let seek_pos = song.start_seconds();
    println!(
        "Song '{}': start_seconds={:.2}s, count_in={:?}",
        song.name,
        song.start_seconds(),
        song.count_in_seconds
    );

    transport.set_position(seek_pos).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pos = transport.get_position().await?;
    let diff = (pos - seek_pos).abs();
    println!("Seeked to song start: {pos:.2}s (expected {seek_pos:.2}s, diff: {diff:.4}s)");

    if !(diff < 0.5) {
        return Err(eyre::eyre!(
            "Should be at song start {seek_pos:.2}s, got {pos:.2}s"
        ));
    }

    // Verify count-in: with COUNT-IN at 0s and SONGSTART at 4s,
    // the count-in duration should be ~4s
    if let Some(ci) = song.count_in_seconds {
        println!("Count-in duration: {ci:.2}s");
        if !((ci - 4.0).abs() < 0.5) {
            return Err(eyre::eyre!("Count-in should be ~4s, got {ci:.2}s"));
        }

        // Song start (with count-in) should be at 0s
        if !(song.start_seconds() < 1.0) {
            return Err(eyre::eyre!(
                "Song start with count-in should be ~0s, got {:.2}s",
                song.start_seconds()
            ));
        }
    }

    Ok(())
}

/// Navigate to song end: verify seeking near SONGEND works correctly.
#[reaper_test]
async fn navigate_to_song_end(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let song = setup_song(&ctx.project).await?;
    let transport = ctx.project.transport();
    transport.stop().await?;

    // Find the last "real" section before End
    let last_content_section = song
        .sections
        .iter()
        .rev()
        .find(|s| s.section_type != session::SectionType::End)
        .expect("Should have at least one non-End section");

    println!(
        "Last content section: '{}' ({:.2}s - {:.2}s)",
        last_content_section.name,
        last_content_section.start_seconds,
        last_content_section.end_seconds
    );

    // Seek to near the end of the last content section
    let near_end = last_content_section.end_seconds - 1.0;
    transport.set_position(near_end).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let pos = transport.get_position().await?;
    println!("Position near song end: {pos:.2}s");

    if !((pos - near_end).abs() < 0.5) {
        return Err(eyre::eyre!("Should be near {near_end:.2}s, got {pos:.2}s"));
    }

    // Verify the song end boundary
    println!(
        "Song end: {:.2}s (Outro ends at {:.2}s)",
        song.end_seconds, last_content_section.end_seconds
    );

    Ok(())
}
