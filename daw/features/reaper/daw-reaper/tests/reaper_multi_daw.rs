//! Multi-instance REAPER integration tests.
//!
//! Validates that two independent REAPER instances can be spawned, connected
//! to, and operated in parallel using the daw-bridge extension infrastructure.
//!
//! Uses `fts-daw-test` (role = "testing") and `fts-daw-secondary` (role = "secondary").
//! Both rigs must be installed via `cargo xtask setup-rigs` before running.
//!
//! Run with:
//!
//!   cargo test -p daw-reaper --test reaper_multi_daw -- --ignored --nocapture
//!
//! On Linux without a display, run via xtask to get Xvfb:
//!
//!   cargo xtask reaper-test

use daw::test::{DawInstanceConfig, run_multi_reaper_test};
use eyre::Result;

// ---------------------------------------------------------------------------
// Two-instance: independent connections and project isolation
// ---------------------------------------------------------------------------

/// Spawn two REAPER instances using different rigs, connect to both, and
/// verify they are independent (different sockets, isolated project state).
///
/// Skipped when `FTS_SOCKET` is set (i.e. running via `cargo xtask reaper-test`)
/// because the xtask already occupies the `fts-daw-test` rig — spawning another
/// instance on the same rig would delete the existing socket and break subsequent tests.
#[test]
#[ignore]
fn two_daw_independent_instances() -> Result<()> {
    if std::env::var("FTS_SOCKET").is_ok() {
        println!("Skipping: FTS_SOCKET is set (xtask already owns fts-daw-test rig)");
        return Ok(());
    }
    run_multi_reaper_test(
        "two_daw_independent_instances",
        vec![
            DawInstanceConfig::for_rig("primary", "fts-daw-test"),
            DawInstanceConfig::for_rig("secondary", "fts-daw-secondary"),
        ],
        |ctx| {
            Box::pin(async move {
                let primary = ctx.by_label("primary");
                let secondary = ctx.by_label("secondary");

                // Verify the two instances use distinct sockets
                assert_ne!(
                    primary.socket_path, secondary.socket_path,
                    "instances must use distinct sockets"
                );

                println!(
                    "  primary   PID={} socket={}",
                    primary.pid,
                    primary.socket_path.display()
                );
                println!(
                    "  secondary PID={} socket={}",
                    secondary.pid,
                    secondary.socket_path.display()
                );

                // Both instances must respond to health checks
                assert!(
                    primary.daw.healthcheck().await,
                    "primary failed healthcheck"
                );
                assert!(
                    secondary.daw.healthcheck().await,
                    "secondary failed healthcheck"
                );

                // Create a track in primary — must not appear in secondary
                let p_project = primary.daw.current_project().await?;
                let s_project = secondary.daw.current_project().await?;

                let p_before = p_project.tracks().all().await?.len();
                let s_before = s_project.tracks().all().await?.len();

                p_project
                    .tracks()
                    .add("MultiDawIsolationCheck", None)
                    .await?;

                let p_after = p_project.tracks().all().await?.len();
                let s_after = s_project.tracks().all().await?.len();

                assert_eq!(p_after, p_before + 1, "primary should have one new track");
                assert_eq!(s_after, s_before, "secondary must not see primary's tracks");

                println!("  isolation verified: primary +1 track, secondary unchanged");
                Ok(())
            })
        },
    )
}
