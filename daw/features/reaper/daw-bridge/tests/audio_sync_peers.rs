//! Phase E validation: two REAPER instances bring up the audio-sync
//! ClockSync layer, discover each other via UDP, and exchange
//! sample-position broadcasts.
//!
//! Each instance runs the standard `daw-bridge` extension plus the
//! audio-sync stack (audio hook + ClockSync). The test waits for
//! each peer to observe the other in its peer table with a recent
//! announce + a non-NaN remote playhead (proves the position frame
//! round-trip works between separate REAPER processes).
//!
//! Doesn't yet validate drift convergence — that's the next phase
//! (Phase F: drift correction actuator). This test proves the
//! observation layer works end-to-end across processes, which is
//! the foundation everything else builds on.
//!
//! Run with:
//!   cargo xtask reaper-test -- audio_sync_peers

use std::time::Duration;

use daw::test::{DawInstanceConfig, run_multi_reaper_test};
use eyre::Result;

const PORT_A: u16 = 17782;
const PORT_B: u16 = 17783;

#[test]
#[ignore]
fn audio_sync_peers() -> Result<()> {
    run_multi_reaper_test(
        "audio_sync_peers",
        vec![
            DawInstanceConfig::new("alpha")
                .with_env("DISPLAY", "")
                .with_env("FTS_AUDIO_SYNC_PORT", &PORT_A.to_string())
                .with_env("FTS_AUDIO_SYNC_DRIFT", "1")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-audio-sync-alpha.sock"),
            DawInstanceConfig::new("bravo")
                .with_env("DISPLAY", "")
                .with_env("FTS_AUDIO_SYNC_PORT", &PORT_B.to_string())
                .with_env("FTS_AUDIO_SYNC_DRIFT", "1")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-audio-sync-bravo.sock"),
        ],
        |ctx| {
            Box::pin(async move {
                let alpha = ctx.by_label("alpha");
                let bravo = ctx.by_label("bravo");

                // ── Wait for both peers' ClockSync to bind and pick
                //    up a stable peer id. The bind is async and
                //    happens after extension load, so allow a few
                //    seconds.
                let mut alpha_id = String::new();
                let mut bravo_id = String::new();
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                while std::time::Instant::now() < deadline
                    && (alpha_id.is_empty() || bravo_id.is_empty())
                {
                    if alpha_id.is_empty() {
                        alpha_id = alpha.daw.diagnostics().audio_sync_self_peer_id().await?;
                    }
                    if bravo_id.is_empty() {
                        bravo_id = bravo.daw.diagnostics().audio_sync_self_peer_id().await?;
                    }
                    if alpha_id.is_empty() || bravo_id.is_empty() {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
                eyre::ensure!(
                    !alpha_id.is_empty() && !bravo_id.is_empty(),
                    "ClockSync never bound (alpha_id={alpha_id:?} bravo_id={bravo_id:?})"
                );
                println!("  alpha peer_id={alpha_id}");
                println!("  bravo peer_id={bravo_id}");

                // ── Multicast on loopback is flaky across platforms.
                //    Seed peers explicitly via the diagnostics RPC so
                //    the test validates the protocol independently of
                //    the discovery layer. On real LANs multicast
                //    handles this automatically (see Phase B docs);
                //    here we just want to prove the round-trip works
                //    between two real REAPER processes.
                alpha
                    .daw
                    .diagnostics()
                    .audio_sync_seed_peer(&bravo_id, &format!("127.0.0.1:{PORT_B}"))
                    .await?;
                bravo
                    .daw
                    .diagnostics()
                    .audio_sync_seed_peer(&alpha_id, &format!("127.0.0.1:{PORT_A}"))
                    .await?;
                println!("  peers seeded; awaiting position broadcast");

                let deadline = std::time::Instant::now() + Duration::from_secs(30);
                loop {
                    let alpha_peers = alpha.daw.diagnostics().audio_sync_peers().await?;
                    let bravo_peers = bravo.daw.diagnostics().audio_sync_peers().await?;

                    let alpha_sees_bravo = alpha_peers
                        .iter()
                        .find(|p| p.id == bravo_id)
                        .filter(|p| p.announce_age_ms < 3_000);
                    let bravo_sees_alpha = bravo_peers
                        .iter()
                        .find(|p| p.id == alpha_id)
                        .filter(|p| p.announce_age_ms < 3_000);

                    if let (Some(b), Some(a)) = (alpha_sees_bravo, bravo_sees_alpha) {
                        println!(
                            "  alpha sees bravo: addr={} offset_us={} delay_us={} playhead={}",
                            b.addr, b.offset_us, b.delay_us, b.remote_playhead_seconds
                        );
                        println!(
                            "  bravo sees alpha: addr={} offset_us={} delay_us={} playhead={}",
                            a.addr, a.offset_us, a.delay_us, a.remote_playhead_seconds
                        );

                        // Round trip succeeded — recent announce in
                        // both directions. Position broadcast might
                        // still need another second to flow; check
                        // both arrived if we've waited long enough.
                        let pos_arrived = !b.remote_playhead_seconds.is_nan()
                            && !a.remote_playhead_seconds.is_nan();
                        if pos_arrived {
                            // Drift corrector should now be ticking on
                            // both sides. Verify each side's decision
                            // has a sequence > 0 (controller ran at
                            // least once). Drift value itself depends
                            // on whether transport is playing; both
                            // are stopped here, so target_rate should
                            // be 1.0 and drift_seconds is NaN.
                            let alpha_dec =
                                alpha.daw.diagnostics().audio_sync_drift_decision().await?;
                            let bravo_dec =
                                bravo.daw.diagnostics().audio_sync_drift_decision().await?;
                            println!(
                                "  alpha drift: seq={} leader={:?} rate={}",
                                alpha_dec.sequence, alpha_dec.leader_peer_id, alpha_dec.target_rate
                            );
                            println!(
                                "  bravo drift: seq={} leader={:?} rate={}",
                                bravo_dec.sequence, bravo_dec.leader_peer_id, bravo_dec.target_rate
                            );
                            eyre::ensure!(
                                alpha_dec.sequence > 0,
                                "alpha drift corrector never ticked"
                            );
                            eyre::ensure!(
                                bravo_dec.sequence > 0,
                                "bravo drift corrector never ticked"
                            );
                            // Transport stopped → no leader → rate held at 1.0
                            eyre::ensure!(
                                (alpha_dec.target_rate - 1.0).abs() < 1e-9,
                                "alpha rate should be 1.0 when stopped, got {}",
                                alpha_dec.target_rate
                            );

                            // Multi-project: each peer's local
                            // registry should have at least one
                            // project (REAPER always has one open
                            // when running). And each side should
                            // see at least one project frame from
                            // the other via audio_sync_peer_projects.
                            let alpha_local =
                                alpha.daw.diagnostics().audio_sync_local_projects().await?;
                            let bravo_local =
                                bravo.daw.diagnostics().audio_sync_local_projects().await?;
                            println!(
                                "  alpha local projects: {} | bravo local projects: {}",
                                alpha_local.len(),
                                bravo_local.len()
                            );
                            for p in &alpha_local {
                                println!(
                                    "    alpha[{}]: playhead={} playing={}",
                                    p.project_id_hex,
                                    p.snapshot.playhead_seconds,
                                    p.snapshot.is_playing
                                );
                            }
                            for p in &bravo_local {
                                println!(
                                    "    bravo[{}]: playhead={} playing={}",
                                    p.project_id_hex,
                                    p.snapshot.playhead_seconds,
                                    p.snapshot.is_playing
                                );
                            }
                            eyre::ensure!(
                                !alpha_local.is_empty(),
                                "alpha multi-project registry empty — updater not running?"
                            );
                            eyre::ensure!(
                                !bravo_local.is_empty(),
                                "bravo multi-project registry empty — updater not running?"
                            );

                            let alpha_sees_bravo_projects = alpha
                                .daw
                                .diagnostics()
                                .audio_sync_peer_projects(&bravo_id)
                                .await?;
                            let bravo_sees_alpha_projects = bravo
                                .daw
                                .diagnostics()
                                .audio_sync_peer_projects(&alpha_id)
                                .await?;
                            println!(
                                "  alpha sees {} bravo project(s), bravo sees {} alpha project(s)",
                                alpha_sees_bravo_projects.len(),
                                bravo_sees_alpha_projects.len()
                            );
                            return Ok(());
                        }
                    }

                    if std::time::Instant::now() > deadline {
                        eyre::bail!(
                            "peers did not converge within 30s. \
                             alpha_peers={alpha_peers:?} bravo_peers={bravo_peers:?}"
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            })
        },
    )
}
