//! Timer health check — verifies REAPER's main-thread timer callback
//! keeps firing for a sustained period.
//!
//! This test does NOT run in the normal integration suite. Run it
//! explicitly to diagnose headless timer issues:
//!
//!   cargo xtask reaper-test -- timer_responsive_for_60s

use daw::test::reaper_test;

/// Ping REAPER's transport every second for 55 seconds.
/// If the timer callback stops firing, the RPC calls will hang and
/// the test will time out — proving the timer is broken.
///
/// Uses 55s to leave headroom for test setup/cleanup within the
/// 60s per-test timeout.
#[reaper_test(isolated)]
async fn timer_responsive_for_60s(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let transport = ctx.project().transport();

    for tick in 1..=55 {
        let state = transport.get_play_state().await?;
        ctx.log(&format!("[tick {tick}/55] play_state={state:?}"));
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    ctx.log("Timer stayed responsive for 55 seconds");
    Ok(())
}
