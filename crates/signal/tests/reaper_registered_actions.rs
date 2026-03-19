//! REAPER integration test for action registration.
//!
//! Verifies that all expected FTS actions are registered with REAPER's action
//! system and discoverable via `is_registered` / `lookup_command_id`.
//!
//! Run with:
//!   cargo xtask reaper-test -- registered_actions

use reaper_test::reaper_test;

/// Representative actions from each action group that should be registered
/// when the reaper-extension loads. This is a smoke test — we check a
/// sample from each group rather than exhaustively listing every action.
const EXPECTED_ACTIONS: &[(&str, &str)] = &[
    // ── Transport ────────────────────────────────────────────────────────
    ("fts.transport.play", "Transport"),
    ("fts.transport.stop", "Transport"),
    ("fts.transport.play_pause", "Transport"),
    ("fts.transport.play_stop", "Transport"),
    ("fts.transport.record", "Transport"),
    ("fts.transport.toggle_repeat", "Transport"),
    // ── Markers & Regions ────────────────────────────────────────────────
    ("fts.markers_regions.insert_marker", "Markers/Regions"),
    ("fts.markers_regions.insert_region", "Markers/Regions"),
    // ── Reaper Extension (dev/logging) ───────────────────────────────────
    ("fts.reaper_extension.log_runtime", "Reaper Extension"),
    ("fts.reaper_extension.console_msg", "Reaper Extension"),
    // ── Dynamic Template ─────────────────────────────────────────────────
    ("fts.dynamic_template.organize_tracks", "Dynamic Template"),
    // ── Auto Color ───────────────────────────────────────────────────────
    ("fts.auto_color.apply_auto_color", "Auto Color"),
    // ── Visibility Manager ───────────────────────────────────────────────
    ("fts.visibility_manager.show_all", "Visibility Manager"),
    ("fts.visibility_manager.hide_all", "Visibility Manager"),
    // ── Session ──────────────────────────────────────────────────────────
    ("fts.session.build_setlist", "Session"),
    ("fts.session.next_song", "Session"),
    ("fts.session.previous_song", "Session"),
    // ── Signal (variation switching) ─────────────────────────────────────
    ("fts.signal.next_section", "Signal"),
    ("fts.signal.previous_section", "Signal"),
    ("fts.signal.switch_variation_1", "Signal"),
];

/// Verify that all expected FTS actions are registered with REAPER.
#[reaper_test]
async fn registered_actions_check(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();

    let mut missing = Vec::new();
    let mut found = 0u32;

    for &(action_id, group) in EXPECTED_ACTIONS {
        let registered = actions.is_registered(action_id).await?;
        if registered {
            found += 1;
            let cmd_id = actions.lookup_command_id(action_id).await?;
            println!("  OK  {action_id} (cmd_id={cmd_id:?}) [{group}]");
        } else {
            missing.push((action_id, group));
            println!("  MISSING  {action_id} [{group}]");
        }
    }

    println!("\n{found}/{} actions registered", EXPECTED_ACTIONS.len());

    assert!(
        missing.is_empty(),
        "Missing {} action(s):\n{}",
        missing.len(),
        missing
            .iter()
            .map(|(id, group)| format!("  - {id} [{group}]"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    Ok(())
}
