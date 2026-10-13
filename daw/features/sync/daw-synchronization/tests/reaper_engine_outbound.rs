//! Integration test: a local DAW mutation propagates through the
//! `SynchronizationEngine`'s outbound stream as a `SyncEvent`.
//!
//! This is the Step-6 acceptance gate from `docs/synchronization-engine-lift.md`.
//! Drives a real REAPER (via the `reaper-test` harness), starts an engine,
//! adds a marker, and asserts the outbound broadcast emits a
//! `SyncDomain::Marker(Added)` with the correct origin peer.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- engine_outbound

use std::time::Duration;

use daw::test::reaper_test;
use daw_synchronization::{SyncConfig, SyncDomain, SyncSession, SynchronizationEngine};

const PEER_ID: &str = "test-peer";

#[reaper_test(isolated)]
async fn marker_add_emits_sync_event(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project_guid = ctx.project().guid().to_string();

    let session = SyncSession {
        session_id: "test-session".into(),
        peer_id: PEER_ID.into(),
        display_name: "Test Peer".into(),
    };
    let engine = SynchronizationEngine::new(ctx.daw.clone(), session, SyncConfig::all());

    // Subscribe BEFORE starting so we don't race the snapshot push.
    let mut outbound = engine.subscribe();
    engine.start().await?;

    // Mutate the test's isolated project.
    ctx.project().markers().add(1.0, "ti-marker").await?;

    // Pull events until we see the Marker(Added) for our project, or time out.
    // Other domains (e.g. transport snapshot on first subscribe) may arrive first.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            eyre::bail!("timed out waiting for SyncDomain::Marker(Added)");
        }
        let event = tokio::time::timeout(remaining, outbound.recv())
            .await
            .map_err(|_| eyre::eyre!("timed out waiting for SyncEvent"))??;

        if event.origin_peer != PEER_ID {
            continue;
        }
        if event.project_guid != project_guid {
            continue;
        }
        if let SyncDomain::Marker(daw::service::MarkerEvent::Added(marker)) = &event.domain {
            assert_eq!(marker.name, "ti-marker", "marker name should round-trip");
            let pos_secs = marker
                .position
                .time
                .as_ref()
                .map(|t| t.as_seconds())
                .unwrap_or(f64::NAN);
            assert!(
                (pos_secs - 1.0).abs() < 1e-6,
                "marker position should round-trip (got {pos_secs})"
            );
            println!(
                "engine emitted SyncEvent seq={} for marker '{}' at {pos_secs}s",
                event.sequence, marker.name
            );
            return Ok(());
        }
    }
}

#[reaper_test(isolated)]
async fn track_add_emits_sync_event(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project_guid = ctx.project().guid().to_string();

    let session = SyncSession {
        session_id: "test-session".into(),
        peer_id: PEER_ID.into(),
        display_name: "Test Peer".into(),
    };
    let engine = SynchronizationEngine::new(ctx.daw.clone(), session, SyncConfig::all());

    let mut outbound = engine.subscribe();
    engine.start().await?;

    let _track = ctx.project().tracks().add("SyncTestTrack", None).await?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            eyre::bail!("timed out waiting for SyncDomain::Track(Added)");
        }
        let event = tokio::time::timeout(remaining, outbound.recv())
            .await
            .map_err(|_| eyre::eyre!("timed out waiting for SyncEvent"))??;

        if event.origin_peer != PEER_ID || event.project_guid != project_guid {
            continue;
        }
        if let SyncDomain::Track(daw::service::TrackEvent::Added(track)) = &event.domain {
            assert_eq!(track.name, "SyncTestTrack");
            println!(
                "engine emitted SyncEvent seq={} for track '{}' guid={}",
                event.sequence, track.name, track.guid
            );
            return Ok(());
        }
    }
}
