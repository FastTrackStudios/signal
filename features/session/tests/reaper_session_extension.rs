//! REAPER integration test for the session-extension test host.
//!
//! Verifies that the session-extension REAPER plugin was loaded and wrote its
//! health beacon to `ExtState`.
//!
//! Run with:
//!   cargo xtask reaper-test -- `session_extension_health`

use std::time::Duration;

use daw::test::reaper_test;

/// Verify that session-extension connected and wrote its health beacon.
///
/// The extension writes `FTS_SESSION_EXT/status = "ready"` and
/// `FTS_SESSION_EXT/pid = "<pid>"` after successful initialization.
/// We poll for up to 10 seconds to give it time to start.
#[reaper_test]
async fn session_extension_health(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();

    // Poll — the extension may still be connecting
    let mut status = None;
    for i in 0..20 {
        status = ext.get("FTS_SESSION_EXT", "status").await?;
        if status.is_some() {
            break;
        }
        if i < 19 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    let status = status.ok_or_else(|| {
        eyre::eyre!("session-extension should have written FTS_SESSION_EXT/status")
    })?;
    if status != "ready" {
        return Err(eyre::eyre!("status should be 'ready', got '{}'", status));
    }

    let pid = ext
        .get("FTS_SESSION_EXT", "pid")
        .await?
        .ok_or_else(|| eyre::eyre!("session-extension should have written FTS_SESSION_EXT/pid"))?;
    let pid: u32 = pid
        .parse()
        .map_err(|e| eyre::eyre!("pid should be a valid u32: {}", e))?;
    if pid == 0 {
        return Err(eyre::eyre!("pid should be a real process id"));
    }

    println!("session-extension is healthy: status={status}, pid={pid}");

    // Clean up so subsequent runs start fresh
    ext.delete("FTS_SESSION_EXT", "status", false).await?;
    ext.delete("FTS_SESSION_EXT", "pid", false).await?;

    Ok(())
}
