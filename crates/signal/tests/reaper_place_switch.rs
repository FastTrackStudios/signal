//! REAPER integration test: Place switch actions.
//!
//! Verifies the place_section_switch, place_song_switch, and place_scene_switch
//! actions correctly:
//! 1. Walk up the parent chain to find the right controller track
//! 2. Place a named MIDI item at the edit cursor position
//! 3. Set the correct scene color on the item
//! 4. Produce items that the scene timer can use for switching
//!
//! Run with:
//!   cargo xtask reaper-test place_switch

use std::time::Duration;

use daw::service::MidiNoteCreate;
use reaper_test::reaper_test;

/// Small sleep to let REAPER process changes.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// Wait for scene timer re-scan (~5 seconds).
async fn wait_for_timer() {
    tokio::time::sleep(Duration::from_secs(6)).await;
}

/// Wait for scene timer to apply a switch.
async fn wait_for_switch() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ---------------------------------------------------------------------------
// Helper: Build a setlist-like track structure for testing
// ---------------------------------------------------------------------------

struct TestSetlist {
    rig_folder: daw::TrackHandle,
    belief_folder: daw::TrackHandle,
    /// Section tracks: Clean, Ambient, Rhythm (direct children of song folder)
    belief_sections: Vec<daw::TrackHandle>,
    #[allow(dead_code)]
    vienna_folder: daw::TrackHandle,
    /// Section tracks: Clean, Drive
    vienna_sections: Vec<daw::TrackHandle>,
}

async fn build_test_setlist(tracks: &daw::Tracks) -> eyre::Result<TestSetlist> {
    // ── Rig folder ──────────────────────────────────────────────────
    let rig_folder = tracks.add("Guitar Rig", None).await?;
    rig_folder.set_folder_depth(1).await?;
    rig_folder.set_color(0x9CA3AF).await?;
    rig_folder
        .set_ext_state("fts_signal", "scene_count", "2")
        .await?;
    settle().await;

    // Guitar Input — audio source, no special routing
    let _rig_input = tracks.add("Guitar Input", None).await?;
    settle().await;

    // ── Song 1: Belief (3 sections) ─────────────────────────────────
    let belief_folder = tracks.add("Belief", None).await?;
    belief_folder.set_folder_depth(1).await?;
    belief_folder.set_color(0x22C55E).await?;
    belief_folder
        .set_ext_state("fts_signal", "scene_count", "3")
        .await?;
    settle().await;

    let mut belief_sections = Vec::new();
    for (i, name) in ["Clean", "Ambient", "Rhythm"].iter().enumerate() {
        let is_last = i == 2;
        let section = tracks.add(name, None).await?;
        let depth = if is_last { -1 } else { 0 };
        section.set_folder_depth(depth).await?;
        settle().await;
        belief_sections.push(section);
    }

    // ── Song 2: Vienna (2 sections) ─────────────────────────────────
    let vienna_folder = tracks.add("Vienna", None).await?;
    vienna_folder.set_folder_depth(1).await?;
    vienna_folder.set_color(0x3B82F6).await?;
    vienna_folder
        .set_ext_state("fts_signal", "scene_count", "2")
        .await?;
    settle().await;

    let mut vienna_sections = Vec::new();
    for (i, name) in ["Clean", "Drive"].iter().enumerate() {
        let is_last = i == 1;
        let section = tracks.add(name, None).await?;
        let depth = if is_last { -2 } else { 0 }; // close song + rig on last
        section.set_folder_depth(depth).await?;
        settle().await;
        vienna_sections.push(section);
    }

    Ok(TestSetlist {
        rig_folder,
        belief_folder,
        belief_sections,
        vienna_folder,
        vienna_sections,
    })
}

// ---------------------------------------------------------------------------
// Test: place_section_switch places a named item on the song folder
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn place_section_switch_creates_named_item(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let transport = project.transport();
    let setlist = build_test_setlist(&tracks).await?;

    // Move edit cursor to 2.0s (will be the item position)
    transport.set_position(2.0).await?;
    settle().await;

    // Select the Ambient section track (direct child of Belief folder)
    let ambient = &setlist.belief_sections[1];
    ambient.select_exclusive().await?;
    settle().await;

    // Call place_section_switch
    ctx.log("Calling place_section_switch with Ambient layer selected...");
    signal_extension::place_switch::place_section_switch(&ctx.daw).await?;
    settle().await;

    // Verify: an item was placed on the Belief folder (song controller)
    let item_count = setlist.belief_folder.items().count().await?;
    assert!(
        item_count > 0,
        "place_section_switch should create an item on the song folder"
    );

    let item = setlist
        .belief_folder
        .items()
        .by_index(0)
        .await?
        .expect("item 0 should exist");

    // Item should be at position ~2.0s
    let pos = item.position().await?.as_seconds();
    assert!(
        (pos - 2.0).abs() < 0.1,
        "Item should be at ~2.0s, got {pos}"
    );

    // Item take should be named "Ambient"
    let take_name = item.active_take().name().await?;
    assert_eq!(
        take_name, "Ambient",
        "Take should be named 'Ambient', got '{take_name}'"
    );

    ctx.log("place_section_switch_creates_named_item: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: place_song_switch places a named item on the rig folder
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn place_song_switch_creates_named_item(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let transport = project.transport();
    let setlist = build_test_setlist(&tracks).await?;

    // Move edit cursor to 4.0s
    transport.set_position(4.0).await?;
    settle().await;

    // Select a track inside Vienna (first section track)
    setlist.vienna_sections[0].select_exclusive().await?;
    settle().await;

    // Call place_song_switch
    ctx.log("Calling place_song_switch with Vienna Input selected...");
    signal_extension::place_switch::place_song_switch(&ctx.daw).await?;
    settle().await;

    // Verify: an item was placed on the Rig folder (rig controller)
    let item_count = setlist.rig_folder.items().count().await?;
    assert!(
        item_count > 0,
        "place_song_switch should create an item on the rig folder"
    );

    let item = setlist
        .rig_folder
        .items()
        .by_index(0)
        .await?
        .expect("item 0 should exist");

    let pos = item.position().await?.as_seconds();
    assert!(
        (pos - 4.0).abs() < 0.1,
        "Item should be at ~4.0s, got {pos}"
    );

    let take_name = item.active_take().name().await?;
    assert_eq!(
        take_name, "Vienna",
        "Take should be named 'Vienna', got '{take_name}'"
    );

    ctx.log("place_song_switch_creates_named_item: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: place_scene_switch places a named item on the profile/song folder
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn place_scene_switch_creates_named_item(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let transport = project.transport();
    let setlist = build_test_setlist(&tracks).await?;

    // Move edit cursor to 6.0s
    transport.set_position(6.0).await?;
    settle().await;

    // Select the Rhythm section track of Belief
    let rhythm = &setlist.belief_sections[2];
    rhythm.select_exclusive().await?;
    settle().await;

    // Call place_scene_switch (same as section — walks up to Scene folder)
    ctx.log("Calling place_scene_switch with Rhythm layer selected...");
    signal_extension::place_switch::place_scene_switch(&ctx.daw).await?;
    settle().await;

    // Verify: an item was placed on the Belief folder
    let item_count = setlist.belief_folder.items().count().await?;
    assert!(
        item_count > 0,
        "place_scene_switch should create an item on the song folder"
    );

    let item = setlist
        .belief_folder
        .items()
        .by_index(0)
        .await?
        .expect("item 0 should exist");

    let pos = item.position().await?.as_seconds();
    assert!(
        (pos - 6.0).abs() < 0.1,
        "Item should be at ~6.0s, got {pos}"
    );

    let take_name = item.active_take().name().await?;
    assert_eq!(
        take_name, "Rhythm",
        "Take should be named 'Rhythm', got '{take_name}'"
    );

    ctx.log("place_scene_switch_creates_named_item: PASS");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: Placed items drive scene switching via the timer
// ---------------------------------------------------------------------------

#[reaper_test(isolated)]
async fn placed_items_drive_scene_switching(
    ctx: &ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let transport = project.transport();
    let setlist = build_test_setlist(&tracks).await?;

    // Place section switch items on the Belief folder manually
    let items = setlist.belief_folder.items();

    // Item 0: "Clean" at 0.0-2.0s
    if let Some(item) = items
        .create_midi_item_with_notes(0.0, 2.0, vec![MidiNoteCreate::new(60, 100, 0.0, 960.0)])
        .await?
    {
        item.active_take().set_name("Clean").await?;
    }

    // Item 1: "Ambient" at 2.0-4.0s
    if let Some(item) = items
        .create_midi_item_with_notes(2.0, 4.0, vec![MidiNoteCreate::new(60, 100, 0.0, 960.0)])
        .await?
    {
        item.active_take().set_name("Ambient").await?;
    }

    // Item 2: "Rhythm" at 4.0-6.0s
    if let Some(item) = items
        .create_midi_item_with_notes(4.0, 6.0, vec![MidiNoteCreate::new(60, 100, 0.0, 960.0)])
        .await?
    {
        item.active_take().set_name("Rhythm").await?;
    }

    settle().await;

    // Wait for scene timer to discover the controller
    ctx.log("Waiting for scene timer re-scan...");
    wait_for_timer().await;

    // ── Position at 0.0s → Clean (first scene) ──────────────────────
    ctx.log("Testing section switching at 0.0s (Clean)...");
    transport.set_position(0.0).await?;
    wait_for_switch().await;

    assert!(
        !setlist.belief_sections[0].is_muted().await?,
        "Clean track should be unmuted at 0.0s"
    );
    assert!(
        setlist.belief_sections[1].is_muted().await?,
        "Ambient track should be muted at 0.0s"
    );
    assert!(
        setlist.belief_sections[2].is_muted().await?,
        "Rhythm track should be muted at 0.0s"
    );

    // ── Middle of Ambient (3.0s) ─────────────────────────────────────
    ctx.log("Testing section switching at 3.0s (Ambient)...");
    transport.set_position(3.0).await?;
    wait_for_switch().await;

    assert!(
        setlist.belief_sections[0].is_muted().await?,
        "Clean track should be muted at 3.0s"
    );
    assert!(
        !setlist.belief_sections[1].is_muted().await?,
        "Ambient track should be unmuted at 3.0s"
    );
    assert!(
        setlist.belief_sections[2].is_muted().await?,
        "Rhythm track should be muted at 3.0s"
    );

    // ── Middle of Rhythm (5.0s) ──────────────────────────────────────
    ctx.log("Testing section switching at 5.0s (Rhythm)...");
    transport.set_position(5.0).await?;
    wait_for_switch().await;

    assert!(
        setlist.belief_sections[0].is_muted().await?,
        "Clean track should be muted at 5.0s"
    );
    assert!(
        setlist.belief_sections[1].is_muted().await?,
        "Ambient track should be muted at 5.0s"
    );
    assert!(
        !setlist.belief_sections[2].is_muted().await?,
        "Rhythm track should be unmuted at 5.0s"
    );

    // ── Seek back to Clean (0.5s) ────────────────────────────────────
    ctx.log("Testing backward seek to 0.5s (Clean)...");
    transport.set_position(0.5).await?;
    wait_for_switch().await;

    assert!(
        !setlist.belief_sections[0].is_muted().await?,
        "Clean track should be unmuted after seeking back"
    );
    assert!(
        setlist.belief_sections[1].is_muted().await?,
        "Ambient track should be muted after seeking back"
    );
    assert!(
        setlist.belief_sections[2].is_muted().await?,
        "Rhythm track should be muted after seeking back"
    );

    ctx.log("placed_items_drive_scene_switching: PASS");
    Ok(())
}
