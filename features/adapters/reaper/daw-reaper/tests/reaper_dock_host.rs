//! Integration tests for the DockHost service.
//!
//! Verifies that a DAW client can register docks, mint stable handles
//! per id, and toggle visibility — through the same vox client surface
//! the rest of the daw API uses.
//!
//! These tests do NOT mount real Dioxus panels (that requires a host
//! extension cdylib registering them via `dock::register_panel_from_service`).
//! They exercise the RPC plumbing end-to-end so panel-mounting tests
//! built on top can rely on it.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- reaper_dock_host

use daw::test::reaper_test;
use daw_proto::dock_host::DockKind;

#[reaper_test(isolated)]
async fn register_dock_returns_stable_handle(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let h1 = ctx
        .dock_handle("fts.test.dock_a", "Dock A", DockKind::Tabbed)
        .await?;
    let h2 = ctx
        .dock_handle("fts.test.dock_a", "Dock A renamed", DockKind::Floating)
        .await?;
    assert_eq!(
        h1, h2,
        "re-registering the same id must yield the same handle"
    );
    Ok(())
}

#[reaper_test(isolated)]
async fn distinct_ids_yield_distinct_handles(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let a = ctx
        .dock_handle("fts.test.dock_distinct_a", "A", DockKind::Tabbed)
        .await?;
    let b = ctx
        .dock_handle("fts.test.dock_distinct_b", "B", DockKind::Tabbed)
        .await?;
    assert_ne!(a, b, "distinct ids must yield distinct handles");
    Ok(())
}

#[reaper_test(isolated)]
async fn unmounted_dock_reports_hidden(ctx: &ReaperTestContext) -> eyre::Result<()> {
    // Without a real `register_panel_from_service` call, the dock module
    // has no mounted panel for this id, so visibility is `false`.
    ctx.assert_panel_hidden("fts.test.dock_unmounted").await?;
    Ok(())
}

#[reaper_test(isolated)]
async fn show_hide_round_trip_through_rpc(ctx: &ReaperTestContext) -> eyre::Result<()> {
    // No real panel mounted, so show/hide are no-ops at the dock level
    // — but they MUST traverse the dispatcher cleanly without RPC errors.
    let dock = ctx.daw.dock_host();
    let h = ctx
        .dock_handle("fts.test.dock_round_trip", "Round Trip", DockKind::Tabbed)
        .await?;

    // These calls exist to verify the dispatcher is wired and the trait
    // surface routes correctly. State assertions after show/hide require
    // a mounted panel and live in a follow-up test.
    dock.show(h).await?;
    dock.hide(h).await?;
    let _ = dock.toggle(h).await?;
    Ok(())
}

#[reaper_test(isolated)]
async fn save_restore_layout_round_trip(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let dock = ctx.daw.dock_host();
    // save_layout returns an empty marker blob for the REAPER adapter
    // (state lives in REAPER ExtState). Just verify it doesn't error.
    let _blob = dock.save_layout().await?;
    let _ok = dock.restore_layout(Vec::new()).await?;
    Ok(())
}
