//! Multi-instance transport sync integration test.
//!
//! Spawns 3 REAPER instances, connects their sync extensions via direct TCP
//! (bypassing mDNS which doesn't work on loopback), then verifies that
//! transport state (play/stop/tempo) propagates between all instances
//! through the sync engine's TCP peer mesh.
//!
//! Run with:
//!   cargo xtask reaper-test -- reaper_sync_multi_transport

use std::time::Duration;

use daw::test::{DawInstanceConfig, run_multi_reaper_test};
use eyre::Result;

/// Spawn 3 REAPER instances, connect peers directly, and test transport sync.
#[test]
#[ignore]
fn multi_instance_transport_sync() -> Result<()> {
    run_multi_reaper_test(
        "multi_instance_transport_sync",
        vec![
            DawInstanceConfig::new("inst1")
                .with_env("DISPLAY", "")
                .with_env("FTS_SYNC_NO_LINK", "1")
                .with_env("FTS_SYNC_ENABLED", "1")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-inst1.sock"),
            DawInstanceConfig::new("inst2")
                .with_env("DISPLAY", "")
                .with_env("FTS_SYNC_NO_LINK", "1")
                .with_env("FTS_SYNC_ENABLED", "1")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-inst2.sock"),
            DawInstanceConfig::new("inst3")
                .with_env("DISPLAY", "")
                .with_env("FTS_SYNC_NO_LINK", "1")
                .with_env("FTS_SYNC_ENABLED", "1")
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-inst3.sock"),
        ],
        |ctx| {
            Box::pin(async move {
                let inst1 = ctx.by_label("inst1");
                let inst2 = ctx.by_label("inst2");
                let inst3 = ctx.by_label("inst3");

                // ── Step 1: Wait for all extensions to report ready ─────
                for (label, inst) in [("inst1", inst1), ("inst2", inst2), ("inst3", inst3)] {
                    poll_ext_state(&inst.daw, "FTS_SYNC_EXT", "status", "ready", 30, 1000)
                        .await
                        .unwrap_or_else(|_| panic!("{label}: sync extension did not become ready"));
                    println!("  [{label}] sync extension ready");
                }

                // ── Step 2: Read mesh ports and peer IDs, then connect directly ──
                // mDNS doesn't work on loopback, so we orchestrate connections via ExtState.
                let mut peers = Vec::new();
                for (label, inst) in [("inst1", inst1), ("inst2", inst2), ("inst3", inst3)] {
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

                // Build connect_peers string: "peer_id@127.0.0.1:port,..."
                let connect_str: String = peers
                    .iter()
                    .map(|(_, peer_id, port)| format!("{peer_id}@127.0.0.1:{port}"))
                    .collect::<Vec<_>>()
                    .join(",");

                // Write connect_peers to each instance — each will skip itself by peer_id
                for (_label, inst) in [("inst1", inst1), ("inst2", inst2), ("inst3", inst3)] {
                    inst.daw
                        .ext_state()
                        .set("FTS_SYNC_EXT", "connect_peers", &connect_str, false)
                        .await?;
                }
                println!("  Triggered direct peer connections: {connect_str}");

                // Wait for all to see 2 peers (3 instances - 1 = 2 peers each)
                let expected_peers = "2";
                for (label, inst) in [("inst1", inst1), ("inst2", inst2), ("inst3", inst3)] {
                    poll_ext_state(
                        &inst.daw,
                        "FTS_SYNC_EXT",
                        "peer_count",
                        expected_peers,
                        30,
                        1000,
                    )
                    .await
                    .unwrap_or_else(|_| panic!("{label}: did not reach {expected_peers} peers"));
                    println!("  [{label}] connected to {expected_peers} peers");
                }

                // ── Step 3: Start playback on instance 1 ────────────────
                let p1 = inst1.daw.current_project().await?;
                p1.transport().play().await?;
                println!("  [inst1] playback started");

                // Verify inst2 and inst3 are also playing
                tokio::time::sleep(Duration::from_secs(2)).await;
                for (label, inst) in [("inst2", inst2), ("inst3", inst3)] {
                    let project = inst.daw.current_project().await?;
                    let state = project.transport().get_state().await?;
                    assert!(
                        matches!(state.play_state, daw::service::PlayState::Playing),
                        "{label} should be playing after inst1 started playback"
                    );
                    println!("  [{label}] confirmed playing");
                }

                // ── Step 4: Set tempo to 140 on instance 1 ─────────────
                p1.transport().set_tempo(140.0).await?;
                println!("  [inst1] tempo set to 140");

                tokio::time::sleep(Duration::from_secs(2)).await;
                for (label, inst) in [("inst2", inst2), ("inst3", inst3)] {
                    let project = inst.daw.current_project().await?;
                    let tempo = project.transport().get_tempo().await?;
                    let diff = (tempo - 140.0).abs();
                    assert!(diff < 1.0, "{label} tempo should be ~140, got {tempo}");
                    println!("  [{label}] tempo confirmed ~140 ({tempo:.1})");
                }

                // ── Step 5: Stop playback on instance 1 ────────────────
                p1.transport().stop().await?;
                println!("  [inst1] playback stopped");

                // Poll each instance for up to 5s — stop propagation through the
                // mesh + apply_transport + local poller can take a moment.
                for (label, inst) in [("inst1", inst1), ("inst2", inst2), ("inst3", inst3)] {
                    let mut stopped = false;
                    for _ in 0..20 {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        let project = inst.daw.current_project().await?;
                        let state = project.transport().get_state().await?;
                        if matches!(state.play_state, daw::service::PlayState::Stopped) {
                            stopped = true;
                            break;
                        }
                    }
                    assert!(stopped, "{label} should be stopped within 10s");
                    println!("  [{label}] confirmed stopped");
                }

                println!("  All transport sync assertions passed!");
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
