//! Integration tests for the WindowGeometryService.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- reaper_window_geometry

use daw::test::reaper_test;
use daw_proto::WindowTarget;

#[reaper_test(isolated)]
async fn get_rect_returns_main_window_size(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let result = ctx.daw.window_geometry().rect(WindowTarget::Main).await?;
    assert!(result.applied, "main window must be resolvable: {result:?}");
    let rect = result.rect.expect("applied result must include rect");
    assert!(rect.width > 0 && rect.height > 0, "non-zero rect: {rect:?}");
    Ok(())
}

#[reaper_test(isolated)]
async fn nudge_round_trips_main_window(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let geometry = ctx.daw.window_geometry();
    let before = geometry
        .rect(WindowTarget::Main)
        .await?
        .rect
        .expect("baseline rect");
    geometry.nudge(WindowTarget::Main, 30, 20).await?;
    geometry.nudge(WindowTarget::Main, -30, -20).await?;
    let after = geometry
        .rect(WindowTarget::Main)
        .await?
        .rect
        .expect("post-nudge rect");
    // Some window managers ignore SetWindowPos for maximized / tiled
    // windows. Either we get back to the baseline (allow ±4 px slack),
    // or the WM ignored both calls and we never moved at all. Both are
    // acceptable — the assertion is that nudge doesn't *escape* its
    // own delta.
    let dx = (after.x - before.x).abs();
    let dy = (after.y - before.y).abs();
    assert!(
        dx <= 4 && dy <= 4,
        "nudge round-trip must not drift past ±4 px (before={before:?} after={after:?})"
    );
    Ok(())
}

#[reaper_test(isolated)]
async fn focused_target_unresolved_falls_back(ctx: &ReaperTestContext) -> eyre::Result<()> {
    // The headless reaper-test rig may have no focused window; in that
    // case the service should return `applied=false` with a populated
    // error rather than failing the RPC. Either outcome is fine — what
    // we're locking in is "doesn't panic / hang the dispatcher".
    let result = ctx
        .daw
        .window_geometry()
        .rect(WindowTarget::Focused)
        .await?;
    if !result.applied {
        assert!(!result.error.is_empty(), "must surface a reason");
    }
    Ok(())
}
