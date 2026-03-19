//! REAPER integration test for the signal-extension SHM guest.
//!
//! Verifies that the signal-extension binary was loaded by daw-bridge,
//! connected over SHM, and wrote its health beacon to ExtState.
//!
//! Run with:
//!   cargo xtask reaper-test -- signal_extension_health

use std::time::Duration;

use reaper_test::reaper_test;

/// Verify that signal-extension connected and wrote its health beacon.
///
/// The extension writes `FTS_SIGNAL_EXT/status = "ready"` and
/// `FTS_SIGNAL_EXT/pid = "<pid>"` on successful SHM connect.
/// We poll for up to 10 seconds to give it time to start.
#[reaper_test]
async fn signal_extension_health(ctx: &reaper_test::ReaperTestContext) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();

    // Poll — the extension may still be connecting
    let mut status = None;
    for i in 0..20 {
        status = ext.get("FTS_SIGNAL_EXT", "status").await?;
        if status.is_some() {
            break;
        }
        if i < 19 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    let status = status.expect("signal-extension should have written FTS_SIGNAL_EXT/status");
    assert_eq!(status, "ready", "status should be 'ready', got '{status}'");

    let pid = ext
        .get("FTS_SIGNAL_EXT", "pid")
        .await?
        .expect("signal-extension should have written FTS_SIGNAL_EXT/pid");
    let pid: u32 = pid.parse().expect("pid should be a valid u32");
    assert!(pid > 0, "pid should be a real process id");

    println!("signal-extension is healthy: status={status}, pid={pid}");

    // Clean up so subsequent runs start fresh
    ext.delete("FTS_SIGNAL_EXT", "status", false).await?;
    ext.delete("FTS_SIGNAL_EXT", "pid", false).await?;

    Ok(())
}
