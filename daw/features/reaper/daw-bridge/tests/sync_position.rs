//! Multi-instance position sync integration test.
//!
//! Spawns 4 REAPER instances via `#[reaper_test(instances(...))]`, connects
//! them via direct TCP PeerMesh, then verifies that transport position stays
//! tightly synced across ALL instances through scenarios: play, sustained
//! playback, tempo change, seek while playing, stop, and restart.
//!
//! Run with:
//!   cargo xtask reaper-test -- position_sync

use std::time::Duration;

use daw::service::PlayState;
use daw::test::reaper_test;

/// Maximum allowed position drift between any two instances (seconds).
///
/// Positions are read concurrently (parallel RPC) to minimize measurement
/// jitter. With parallel reads, the dominant jitter source is REAPER's
/// main-thread scheduling (~30ms). 100ms tolerance catches real sync
/// failures while allowing for scheduling jitter across 4 instances.
const MAX_DRIFT_SECS: f64 = 0.1; // 100ms

/// Relaxed tolerance after tempo changes / seeks — transients need more time.
/// With 4 instances, the drift corrector may need longer to converge after
/// a disruptive change (tempo switch, seek). 200ms accounts for multi-hop
/// propagation and REAPER's ~30ms timer resolution across all instances.
const MAX_DRIFT_AFTER_CHANGE: f64 = 0.2; // 200ms

/// How often to sample positions during sustained playback checks.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(200);

#[reaper_test(instances("master", "follower1", "follower2", "follower3"))]
async fn position_sync(ctx: &daw::test::MultiDawTestContext) -> Result<()> {
    let master = ctx.by_label("master");
    let followers: Vec<(&str, &daw::test::DawInstance)> = vec![
        ("follower1", ctx.by_label("follower1")),
        ("follower2", ctx.by_label("follower2")),
        ("follower3", ctx.by_label("follower3")),
    ];

    // ── Setup: wait for extensions, connect all 4 peers via direct TCP ──
    // No mDNS in daw-bridge sync runtime yet; tests use connect_sync_peers_direct
    // which writes connect_peers ext_state to trigger TCP dials.
    ctx.connect_sync_peers_direct("FTS_SYNC_EXT").await?;

    // ── Scenario 1: Start playback, verify all followers follow ──
    println!("\n  === Scenario 1: Play and verify all instances sync ===");
    let p_master = master.daw.current_project().await?;
    p_master.transport().play().await?;
    println!("  [master] playback started");

    // Give drift corrector time to converge
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check all instances are playing
    let m_state = master
        .daw
        .current_project()
        .await?
        .transport()
        .get_state()
        .await?;
    println!("  [master] play_state={:?}", m_state.play_state);

    for &(label, inst) in &followers {
        let state = inst
            .daw
            .current_project()
            .await?
            .transport()
            .get_state()
            .await?;
        println!("  [{label}] play_state={:?}", state.play_state);
        assert!(
            matches!(state.play_state, PlayState::Playing),
            "{label} should be playing (got {:?})",
            state.play_state
        );
        println!("  [{label}] confirmed playing");
    }

    // ── Scenario 2: Sustained playback — sample positions over 5s ─
    println!("\n  === Scenario 2: Sustained playback position check (5s) ===");
    let mut max_drift: f64 = 0.0;
    for i in 0..25 {
        // First samples use relaxed tolerance while drift corrector
        // finishes converging across all 4 instances. With smoothed
        // proportional control (gain=1.5, max=0.15), convergence from
        // ~300ms takes ~3s — need ~15 relaxed samples at 200ms interval.
        let tol = if i < 15 {
            MAX_DRIFT_AFTER_CHANGE
        } else {
            MAX_DRIFT_SECS
        };
        let (pairs, drift) = ctx
            .assert_all_positions_close(tol, &format!("sustained playback sample {i}"))
            .await?;
        max_drift = max_drift.max(drift);
        for (a, b, pa, pb, d) in &pairs {
            println!("    sample {i:2}: {a}={pa:.4}s {b}={pb:.4}s drift={d:.4}s");
        }
        tokio::time::sleep(SAMPLE_INTERVAL).await;
    }
    println!("  Max drift during sustained playback: {max_drift:.4}s");

    // ── Scenario 3: Tempo change while playing ───────────────
    println!("\n  === Scenario 3: Tempo change 120→160 while playing ===");
    p_master.transport().set_tempo(160.0).await?;
    println!("  [master] tempo set to 160");

    // Verify master actually changed tempo
    tokio::time::sleep(Duration::from_millis(500)).await;
    let master_tempo = p_master.transport().get_tempo().await?;
    println!("  [master] tempo readback: {master_tempo:.1}");

    // Allow drift corrector to converge after tempo change
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check all followers have new tempo
    for &(label, inst) in &followers {
        let proj = inst.daw.current_project().await?;
        let tempo = proj.transport().get_tempo().await?;
        println!("  [{label}] tempo: {tempo:.1}");
        assert!(
            (tempo - 160.0).abs() < 1.0,
            "{label} tempo should be ~160, got {tempo}"
        );
        println!("  [{label}] tempo confirmed ~160 ({tempo:.1})");
    }

    // Log all positions after tempo change
    let m_state = p_master.transport().get_state().await?;
    println!(
        "  [master] state: tempo={:.1}, pos={:.3}s",
        m_state.tempo.bpm,
        m_state
            .playhead_position
            .time
            .as_ref()
            .map(|t| t.as_seconds())
            .unwrap_or(0.0)
    );
    for &(label, inst) in &followers {
        let state = inst
            .daw
            .current_project()
            .await?
            .transport()
            .get_state()
            .await?;
        println!(
            "  [{label}] state: tempo={:.1}, pos={:.3}s",
            state.tempo.bpm,
            state
                .playhead_position
                .time
                .as_ref()
                .map(|t| t.as_seconds())
                .unwrap_or(0.0)
        );
    }

    for i in 0..10 {
        let (pairs, _) = ctx
            .assert_all_positions_close(
                MAX_DRIFT_AFTER_CHANGE,
                &format!("post-tempo-change sample {i}"),
            )
            .await?;
        for (a, b, pa, pb, d) in &pairs {
            println!("    sample {i}: {a}={pa:.4}s {b}={pb:.4}s drift={d:.4}s");
        }
        tokio::time::sleep(SAMPLE_INTERVAL).await;
    }

    // ── Scenario 4: Seek while playing ───────────────────────
    println!("\n  === Scenario 4: Seek to 30s while playing ===");
    p_master.transport().set_position(30.0).await?;
    println!("  [master] seeked to 30s");

    tokio::time::sleep(Duration::from_secs(3)).await;

    let (pairs, _) = ctx
        .assert_all_positions_close(MAX_DRIFT_AFTER_CHANGE, "after seek to 30s")
        .await?;
    for (a, b, pa, pb, d) in &pairs {
        println!("  After seek: {a}={pa:.4}s {b}={pb:.4}s drift={d:.4}s");
    }

    // ── Scenario 5: Stop ─────────────────────────────────────
    println!("\n  === Scenario 5: Stop ===");
    p_master.transport().stop().await?;
    println!("  [master] stopped");

    // Poll all followers in parallel (up to 5s each)
    {
        let mut handles = Vec::new();
        for &(label, inst) in &followers {
            let daw = inst.daw.clone();
            let label = label.to_string();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if let Ok(proj) = daw.current_project().await
                        && let Ok(state) = proj.transport().get_state().await
                        && matches!(state.play_state, PlayState::Stopped)
                    {
                        println!("  [{label}] confirmed stopped");
                        return Ok::<_, eyre::Report>(());
                    }
                }
                eyre::bail!("{label} should be stopped within 5 seconds");
            }));
        }
        for h in handles {
            h.await??;
        }
    }

    // ── Scenario 6: Restart and verify convergence ───────────
    println!("\n  === Scenario 6: Restart from position 0 ===");
    p_master.transport().set_tempo(120.0).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    p_master.transport().set_position(0.0).await?;
    p_master.transport().play().await?;
    println!("  [master] restarted from 0 (tempo reset to 120)");

    // Poll all followers in parallel (up to 10s each)
    {
        let mut handles = Vec::new();
        for &(label, inst) in &followers {
            let daw = inst.daw.clone();
            let label = label.to_string();
            handles.push(tokio::spawn(async move {
                for _ in 0..20 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if let Ok(proj) = daw.current_project().await
                        && let Ok(state) = proj.transport().get_state().await
                        && matches!(state.play_state, PlayState::Playing)
                    {
                        println!("  [{label}] confirmed playing after restart");
                        return Ok::<_, eyre::Report>(());
                    }
                }
                eyre::bail!("{label} should be playing after restart within 10 seconds");
            }));
        }
        for h in handles {
            h.await??;
        }
    }

    // Re-seek master to 0 now that all followers are playing. The sequential
    // polling above means the master has been advancing while waiting for each
    // follower, so positions are wildly divergent. A fresh seek resets everyone.
    p_master.transport().set_position(0.0).await?;
    println!("  [master] re-seeked to 0 after all followers confirmed playing");

    // Convergence time after re-seek — with 4 instances, the seek propagates
    // through the mesh and drift correction (gain=1.5, max=0.15) takes ~3-4s
    // from 500ms drift. Allow 5s for reliable convergence.
    tokio::time::sleep(Duration::from_secs(5)).await;

    println!("\n  === Final: Post-restart position check (5s) ===");
    let mut max_drift_final: f64 = 0.0;
    for i in 0..15 {
        let tol = if i < 5 {
            MAX_DRIFT_AFTER_CHANGE
        } else {
            MAX_DRIFT_SECS
        };
        let (pairs, drift) = ctx
            .assert_all_positions_close(tol, &format!("post-restart sample {i}"))
            .await?;
        max_drift_final = max_drift_final.max(drift);
        for (a, b, pa, pb, d) in &pairs {
            println!("    sample {i:2}: {a}={pa:.4}s {b}={pb:.4}s drift={d:.4}s");
        }
        tokio::time::sleep(SAMPLE_INTERVAL).await;
    }
    println!("  Max drift post-restart: {max_drift_final:.4}s");

    // ── Cleanup ──────────────────────────────────────────────
    p_master.transport().stop().await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("\n  All position sync assertions passed (4 instances)!");
    println!("  Peak drift during sustained playback: {max_drift:.4}s");
    println!("  Peak drift after restart: {max_drift_final:.4}s");
    Ok(())
}
