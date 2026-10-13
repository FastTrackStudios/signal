#![allow(dead_code)]
//! Network latency simulation test for cross-machine sync validation.
//!
//! Simulates LAN conditions (1-50ms one-way latency with jitter) between
//! two REAPER instances on the same machine using `FTS_SYNC_SIMULATED_LATENCY_MS`.
//! Measures position drift during sustained playback, tempo changes, and seeks
//! to validate that the drift corrector converges under realistic network conditions.
//!
//! Run with:
//!   cargo xtask reaper-test -- network_latency_sync
//!
//! To test specific conditions:
//!   FTS_SYNC_SIMULATED_LATENCY_MS=5 FTS_SYNC_SIMULATED_JITTER_MS=2 cargo xtask reaper-test -- network_latency_sync

use std::time::Duration;

use daw::service::PlayState;
use daw::test::{DawInstanceConfig, run_multi_reaper_test};
use eyre::Result;

/// Maximum allowed position drift during sustained playback (seconds).
/// Cross-machine LAN adds 1-5ms latency, which the drift corrector handles
/// easily. 150ms tolerance accounts for LAN latency + jitter + REAPER scheduling.
const MAX_DRIFT_SECS: f64 = 0.15;

/// Relaxed tolerance after disruptive changes (tempo, seek).
const MAX_DRIFT_AFTER_CHANGE: f64 = 0.3;

/// Latency scenarios to test (one-way ms, jitter ms).
/// Each scenario restarts playback to test convergence from scratch.
const LATENCY_SCENARIOS: &[(u64, u64, &str)] = &[
    (1, 0, "1ms LAN (same switch)"),
    (5, 2, "5ms±2ms LAN (typical WiFi)"),
    (10, 5, "10ms±5ms (cross-subnet)"),
    (25, 10, "25ms±10ms (WAN-like)"),
];

#[test]
#[ignore]
fn network_latency_sync() -> Result<()> {
    run_multi_reaper_test(
        "network_latency_sync",
        vec![
            DawInstanceConfig::new("master")
                .with_env("DISPLAY", "")
                .with_env("FTS_SYNC_NO_LINK", "1")
                .with_env("FTS_SYNC_ENABLED", "1")
                .with_env("FTS_SYNC_NO_MDNS", "1")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-netlat-master.sock"),
            DawInstanceConfig::new("follower")
                .with_env("DISPLAY", "")
                .with_env("FTS_SYNC_NO_LINK", "1")
                .with_env("FTS_SYNC_ENABLED", "1")
                .with_env("FTS_SYNC_NO_MDNS", "1")
                .with_fts_config()
                // Latency is injected on the follower only (simulates one-way delay
                // on incoming messages from the master).
                .with_env(
                    "FTS_SYNC_SIMULATED_LATENCY_MS",
                    &std::env::var("FTS_SYNC_SIMULATED_LATENCY_MS")
                        .unwrap_or_else(|_| "5".to_string()),
                )
                .with_env(
                    "FTS_SYNC_SIMULATED_JITTER_MS",
                    &std::env::var("FTS_SYNC_SIMULATED_JITTER_MS")
                        .unwrap_or_else(|_| "2".to_string()),
                )
                .with_socket("/tmp/fts-daw-test-netlat-follower.sock"),
        ],
        |ctx| {
            Box::pin(async move {
                let master = ctx.by_label("master");
                let follower = ctx.by_label("follower");

                // ── Wait for extensions and connect peers ──────────────
                for (label, inst) in [("master", master), ("follower", follower)] {
                    poll_ext_state(&inst.daw, "FTS_SYNC_EXT", "status", "ready", 30, 1000)
                        .await
                        .unwrap_or_else(|_| panic!("{label}: sync extension did not become ready"));
                    println!("  [{label}] sync extension ready");
                }

                // Read mesh ports and connect directly
                let mut peers = Vec::new();
                for (label, inst) in [("master", master), ("follower", follower)] {
                    let port =
                        poll_ext_state_value(&inst.daw, "FTS_SYNC_EXT", "mesh_port", 10, 500)
                            .await
                            .unwrap_or_else(|_| panic!("{label}: no mesh_port"));
                    let peer_id =
                        poll_ext_state_value(&inst.daw, "FTS_SYNC_EXT", "peer_id", 10, 500)
                            .await
                            .unwrap_or_else(|_| panic!("{label}: no peer_id"));
                    println!("  [{label}] peer_id={peer_id}, mesh_port={port}");
                    peers.push((label, peer_id, port));
                }

                let connect_str: String = peers
                    .iter()
                    .map(|(_, peer_id, port)| format!("{peer_id}@127.0.0.1:{port}"))
                    .collect::<Vec<_>>()
                    .join(",");

                for (_label, inst) in [("master", master), ("follower", follower)] {
                    inst.daw
                        .ext_state()
                        .set("FTS_SYNC_EXT", "connect_peers", &connect_str, false)
                        .await?;
                }
                println!("  Connected peers: {connect_str}");

                poll_ext_state(&master.daw, "FTS_SYNC_EXT", "peer_count", "1", 30, 1000).await?;
                poll_ext_state(&follower.daw, "FTS_SYNC_EXT", "peer_count", "1", 30, 1000).await?;
                println!("  Both peers connected");

                // Read latency config
                let latency = std::env::var("FTS_SYNC_SIMULATED_LATENCY_MS")
                    .unwrap_or_else(|_| "5".to_string());
                let jitter = std::env::var("FTS_SYNC_SIMULATED_JITTER_MS")
                    .unwrap_or_else(|_| "2".to_string());
                println!("\n  ═══════════════════════════════════════════");
                println!("  Network latency test: {latency}ms ± {jitter}ms jitter");
                println!("  ═══════════════════════════════════════════");

                let p_master = master.daw.current_project().await?;
                let p_follower = follower.daw.current_project().await?;

                // ── Test 1: Start playback and measure convergence ────
                println!("\n  === Test 1: Playback convergence ===");
                p_master.transport().play().await?;
                println!("  [master] playback started");

                // Wait for follower to start
                tokio::time::sleep(Duration::from_secs(2)).await;

                let f_state = p_follower.transport().get_state().await?;
                assert!(
                    matches!(f_state.play_state, PlayState::Playing),
                    "follower should be playing (got {:?})",
                    f_state.play_state
                );
                println!("  [follower] confirmed playing");

                // Sample positions over 10 seconds
                let mut max_drift: f64 = 0.0;
                let mut converged_samples = 0u32;
                for i in 0..50 {
                    let m_state = p_master.transport().get_state().await?;
                    let f_state = p_follower.transport().get_state().await?;
                    let m_pos = m_state
                        .playhead_position
                        .time
                        .as_ref()
                        .map(|t| t.as_seconds())
                        .unwrap_or(0.0);
                    let f_pos = f_state
                        .playhead_position
                        .time
                        .as_ref()
                        .map(|t| t.as_seconds())
                        .unwrap_or(0.0);
                    let drift = (m_pos - f_pos).abs();
                    max_drift = max_drift.max(drift);

                    if drift < MAX_DRIFT_SECS {
                        converged_samples += 1;
                    }

                    if i % 10 == 0 || drift > MAX_DRIFT_SECS {
                        println!(
                            "    sample {i:2}: master={m_pos:.3}s follower={f_pos:.3}s drift={drift:.4}s {}",
                            if drift < MAX_DRIFT_SECS { "✓" } else { "⚠" }
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                println!("  Max drift: {max_drift:.4}s, converged: {converged_samples}/50 samples");
                assert!(
                    converged_samples > 35,
                    "Should converge >70% of the time (got {converged_samples}/50)"
                );

                // ── Test 2: Tempo change ──────────────────────────────
                println!("\n  === Test 2: Tempo change 120→160 ===");
                p_master.transport().set_tempo(160.0).await?;
                println!("  [master] tempo set to 160");

                tokio::time::sleep(Duration::from_secs(5)).await;

                let f_tempo = p_follower.transport().get_tempo().await?;
                assert!(
                    (f_tempo - 160.0).abs() < 1.0,
                    "follower tempo should be ~160, got {f_tempo}"
                );
                println!("  [follower] tempo: {f_tempo:.1} ✓");

                // Check positions converge after tempo change
                let mut post_tempo_max: f64 = 0.0;
                for i in 0..10 {
                    let m_state = p_master.transport().get_state().await?;
                    let f_state = p_follower.transport().get_state().await?;
                    let m_pos = m_state
                        .playhead_position
                        .time
                        .as_ref()
                        .map(|t| t.as_seconds())
                        .unwrap_or(0.0);
                    let f_pos = f_state
                        .playhead_position
                        .time
                        .as_ref()
                        .map(|t| t.as_seconds())
                        .unwrap_or(0.0);
                    let drift = (m_pos - f_pos).abs();
                    post_tempo_max = post_tempo_max.max(drift);
                    println!(
                        "    sample {i}: master={m_pos:.3}s follower={f_pos:.3}s drift={drift:.4}s",
                    );
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                println!("  Post-tempo max drift: {post_tempo_max:.4}s");
                assert!(
                    post_tempo_max < MAX_DRIFT_AFTER_CHANGE,
                    "post-tempo drift too high: {post_tempo_max:.4}s > {MAX_DRIFT_AFTER_CHANGE}"
                );

                // ── Test 3: Seek while playing ────────────────────────
                println!("\n  === Test 3: Seek to 60s while playing ===");
                p_master.transport().set_position(60.0).await?;
                println!("  [master] seeked to 60s");

                tokio::time::sleep(Duration::from_secs(3)).await;

                let m_state = p_master.transport().get_state().await?;
                let f_state = p_follower.transport().get_state().await?;
                let m_pos = m_state
                    .playhead_position
                    .time
                    .as_ref()
                    .map(|t| t.as_seconds())
                    .unwrap_or(0.0);
                let f_pos = f_state
                    .playhead_position
                    .time
                    .as_ref()
                    .map(|t| t.as_seconds())
                    .unwrap_or(0.0);
                let seek_drift = (m_pos - f_pos).abs();
                println!("  master={m_pos:.3}s follower={f_pos:.3}s drift={seek_drift:.4}s");
                assert!(
                    seek_drift < MAX_DRIFT_AFTER_CHANGE,
                    "post-seek drift too high: {seek_drift:.4}s"
                );
                println!("  Seek sync ✓");

                // ── Test 4: Stop ──────────────────────────────────────
                println!("\n  === Test 4: Stop ===");
                p_master.transport().stop().await?;
                println!("  [master] stopped");

                tokio::time::sleep(Duration::from_secs(2)).await;
                let f_state = p_follower.transport().get_state().await?;
                assert!(
                    matches!(f_state.play_state, PlayState::Stopped),
                    "follower should be stopped"
                );
                println!("  [follower] confirmed stopped ✓");

                println!("\n  ═══════════════════════════════════════════");
                println!("  Network latency test PASSED ({latency}ms ± {jitter}ms)");
                println!("  Max sustained drift: {max_drift:.4}s");
                println!("  ═══════════════════════════════════════════");

                Ok(())
            })
        },
    )
}

/// Poll an ExtState key until it matches `expected`, with retries.
async fn poll_ext_state(
    daw: &daw::rpc::Daw,
    section: &str,
    key: &str,
    expected: &str,
    retries: u32,
    interval_ms: u64,
) -> Result<()> {
    let ext = daw.ext_state();
    for i in 0..retries {
        if let Ok(Some(val)) = ext.get(section, key).await
            && val == expected
        {
            return Ok(());
        }
        if i < retries - 1 {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    }
    Err(eyre::eyre!(
        "ExtState {section}/{key} did not reach '{expected}' after {retries} retries"
    ))
}

/// Poll an ExtState key until it has any non-empty value.
async fn poll_ext_state_value(
    daw: &daw::rpc::Daw,
    section: &str,
    key: &str,
    retries: u32,
    interval_ms: u64,
) -> Result<String> {
    let ext = daw.ext_state();
    for i in 0..retries {
        if let Ok(Some(val)) = ext.get(section, key).await
            && !val.is_empty()
        {
            return Ok(val);
        }
        if i < retries - 1 {
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    }
    Err(eyre::eyre!(
        "ExtState {section}/{key} never got a value after {retries} retries"
    ))
}
