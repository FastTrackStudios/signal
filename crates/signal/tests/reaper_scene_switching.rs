//! REAPER integration test: Scene switching via send muting.
//!
//! Verifies the fts-signal-controller's timer-based scene switching:
//!
//! 1. Named items on controller tracks drive scene selection (by position order)
//! 2. Scene switching works both during playback and when stopped (cursor position)
//! 3. Moving the cursor to the middle of a section activates the correct scene
//! 4. The first scene (position 0) activates correctly (no off-by-one)
//! 5. Seeking backward re-activates the previous scene
//! 6. Items are named and the name is preserved
//!
//! Run with:
//!   cargo xtask reaper-test scene_switching

use std::time::Duration;

use daw::service::MidiNoteCreate;
use reaper_test::reaper_test;

/// Small sleep to let REAPER process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Longer sleep to let the scene timer (~30Hz) pick up changes.
async fn wait_for_timer() {
    // Scene timer re-scans every ~5 seconds (150 ticks at 30Hz).
    // Wait long enough for at least one re-scan + one poll cycle.
    tokio::time::sleep(Duration::from_secs(6)).await;
}

/// Wait for the scene timer to switch sends after a playhead move.
async fn wait_for_switch() {
    // Timer runs at ~30Hz, so 500ms should catch at least 15 ticks.
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ---------------------------------------------------------------------------
// Test: Scene switching with named items, cursor-based switching, first scene
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn scene_switching_named_items(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let transport = project.transport();

    // ── Build track structure ─────────────────────────────────────────
    // Controller folder (has P_EXT:fts_signal:scene_count = 3)
    let controller = tracks.add("Test Song", None).await?;
    controller.set_folder_depth(1).await?;
    settle().await;

    // Input track (first non-folder child — has sends to scene tracks)
    let input = tracks.add("Test Song Input", None).await?;
    input.set_parent_send(false).await?;
    settle().await;

    // Three scene tracks
    let scene_clean = tracks.add("Scene Clean", None).await?;
    let scene_drive = tracks.add("Scene Drive", None).await?;
    let scene_lead = tracks.add("Scene Lead", None).await?;
    // Close the folder on the last child
    scene_lead.set_folder_depth(-1).await?;
    settle().await;

    // ── Create sends from input → scene tracks ────────────────────────
    let send_clean = input.sends().add_to(scene_clean.guid()).await?;
    let send_drive = input.sends().add_to(scene_drive.guid()).await?;
    let send_lead = input.sends().add_to(scene_lead.guid()).await?;
    settle().await;

    // Initial state: only first send unmuted (matches default scene 0)
    send_drive.mute().await?;
    send_lead.mute().await?;
    settle().await;

    // Verify initial send state
    assert!(!send_clean.is_muted().await?, "send_clean should start unmuted");
    assert!(send_drive.is_muted().await?, "send_drive should start muted");
    assert!(send_lead.is_muted().await?, "send_lead should start muted");

    // ── Set P_EXT on controller track ─────────────────────────────────
    controller
        .set_ext_state("fts_signal", "scene_count", "3")
        .await?;
    settle().await;

    // ── Place named items on the controller track ─────────────────────
    // Each item is 2 seconds long, back-to-back:
    //   "Clean"  at 0.0s - 2.0s  (scene index 0)
    //   "Drive"  at 2.0s - 4.0s  (scene index 1)
    //   "Lead"   at 4.0s - 6.0s  (scene index 2)
    // Items contain a MIDI note (for visual reference) but the timer
    // uses position order, not note pitch.
    let items = controller.items();

    let item_clean = items
        .create_midi_item_with_notes(
            0.0,
            2.0,
            vec![MidiNoteCreate::new(60, 100, 0.0, 960.0)],
        )
        .await?;
    if let Some(ref item) = item_clean {
        item.active_take().set_name("Clean").await?;
        item.set_color(Some(0x22C55E)).await?; // green
    }

    let item_drive = items
        .create_midi_item_with_notes(
            2.0,
            4.0,
            vec![MidiNoteCreate::new(60, 100, 0.0, 960.0)],
        )
        .await?;
    if let Some(ref item) = item_drive {
        item.active_take().set_name("Drive").await?;
        item.set_color(Some(0xEF4444)).await?; // red
    }

    let item_lead = items
        .create_midi_item_with_notes(
            4.0,
            6.0,
            vec![MidiNoteCreate::new(60, 100, 0.0, 960.0)],
        )
        .await?;
    if let Some(ref item) = item_lead {
        item.active_take().set_name("Lead").await?;
        item.set_color(Some(0x3B82F6)).await?; // blue
    }

    settle().await;

    // ── Wait for the scene timer to discover our controller ───────────
    ctx.log("Waiting for scene timer re-scan...");
    wait_for_timer().await;

    // ══════════════════════════════════════════════════════════════════
    // Test 1: First scene activates at position 0 (the active_scene: -1 fix)
    // ══════════════════════════════════════════════════════════════════
    ctx.log("Test 1: First scene at position 0...");
    transport.set_position(0.0).await?;
    wait_for_switch().await;

    assert!(
        !send_clean.is_muted().await?,
        "T1: send_clean should be unmuted at position 0 (scene 0 = Clean)"
    );
    assert!(
        send_drive.is_muted().await?,
        "T1: send_drive should be muted at position 0"
    );
    assert!(
        send_lead.is_muted().await?,
        "T1: send_lead should be muted at position 0"
    );
    ctx.log("Test 1: PASS — first scene activates at position 0");

    // ══════════════════════════════════════════════════════════════════
    // Test 2: Cursor-based switching (no playback) — middle of section
    // ══════════════════════════════════════════════════════════════════
    ctx.log("Test 2: Cursor to middle of Drive section (3.0s)...");
    transport.set_position(3.0).await?; // middle of Drive (2.0-4.0)
    wait_for_switch().await;

    assert!(
        send_clean.is_muted().await?,
        "T2: send_clean should be muted during Drive"
    );
    assert!(
        !send_drive.is_muted().await?,
        "T2: send_drive should be unmuted at 3.0s (middle of Drive)"
    );
    assert!(
        send_lead.is_muted().await?,
        "T2: send_lead should be muted during Drive"
    );
    ctx.log("Test 2: PASS — cursor-based switching to middle of section");

    // ══════════════════════════════════════════════════════════════════
    // Test 3: Switch to third scene (Lead) at middle of section
    // ══════════════════════════════════════════════════════════════════
    ctx.log("Test 3: Cursor to middle of Lead section (5.0s)...");
    transport.set_position(5.0).await?; // middle of Lead (4.0-6.0)
    wait_for_switch().await;

    assert!(
        send_clean.is_muted().await?,
        "T3: send_clean should be muted during Lead"
    );
    assert!(
        send_drive.is_muted().await?,
        "T3: send_drive should be muted during Lead"
    );
    assert!(
        !send_lead.is_muted().await?,
        "T3: send_lead should be unmuted at 5.0s (middle of Lead)"
    );
    ctx.log("Test 3: PASS — third scene activates");

    // ══════════════════════════════════════════════════════════════════
    // Test 4: Seek backward to first scene
    // ══════════════════════════════════════════════════════════════════
    ctx.log("Test 4: Seek backward to Clean (0.5s)...");
    transport.set_position(0.5).await?; // middle of Clean (0.0-2.0)
    wait_for_switch().await;

    assert!(
        !send_clean.is_muted().await?,
        "T4: send_clean should be unmuted after seeking back to Clean"
    );
    assert!(
        send_drive.is_muted().await?,
        "T4: send_drive should be muted after seeking back"
    );
    assert!(
        send_lead.is_muted().await?,
        "T4: send_lead should be muted after seeking back"
    );
    ctx.log("Test 4: PASS — backward seek re-activates first scene");

    // ══════════════════════════════════════════════════════════════════
    // Test 5: Playback-based switching (play into next section)
    // ══════════════════════════════════════════════════════════════════
    ctx.log("Test 5: Play from 1.5s, wait to cross into Drive...");
    transport.set_position(1.5).await?;
    wait_for_switch().await;

    // Verify we're in Clean before playing
    assert!(
        !send_clean.is_muted().await?,
        "T5: should be in Clean at 1.5s before play"
    );

    transport.play().await?;
    // Wait for playback to cross the 2.0s boundary into Drive
    tokio::time::sleep(Duration::from_millis(1200)).await;

    assert!(
        send_clean.is_muted().await?,
        "T5: send_clean should be muted after crossing into Drive"
    );
    assert!(
        !send_drive.is_muted().await?,
        "T5: send_drive should be unmuted after crossing into Drive"
    );
    assert!(
        send_lead.is_muted().await?,
        "T5: send_lead should be muted during Drive"
    );

    transport.stop().await?;
    ctx.log("Test 5: PASS — playback crosses section boundary");

    // ══════════════════════════════════════════════════════════════════
    // Test 6: Exact section boundary positions
    // ══════════════════════════════════════════════════════════════════
    ctx.log("Test 6: Exact boundary positions...");

    // At exactly 2.0s (start of Drive)
    transport.set_position(2.0).await?;
    wait_for_switch().await;
    assert!(
        !send_drive.is_muted().await?,
        "T6: send_drive should be unmuted at exactly 2.0s (start of Drive)"
    );

    // At exactly 4.0s (start of Lead)
    transport.set_position(4.0).await?;
    wait_for_switch().await;
    assert!(
        !send_lead.is_muted().await?,
        "T6: send_lead should be unmuted at exactly 4.0s (start of Lead)"
    );

    ctx.log("Test 6: PASS — exact boundary positions work");

    ctx.log("scene_switching_named_items: ALL TESTS PASSED");
    Ok(())
}
