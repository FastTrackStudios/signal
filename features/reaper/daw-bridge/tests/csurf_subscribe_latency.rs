//! Measures end-to-end latency from a local mutation to the matching
//! stream event arriving over vox, with REAPER's IReaperControlSurface
//! in `full` mode driving the hub.
//!
//! Path under test:
//!
//!   set_volume() RPC → REAPER applies value → csurf callback fires
//!     → hub publish → TracksStream forwarder → vox Tx
//!     → test process Rx → assert latency
//!
//! With `FTS_CSURF_MODE=full`, the csurf callback fires synchronously
//! within REAPER's main-thread tick (sub-ms), so the dominant cost is
//! the vox round-trip. Compare against the 30Hz poller floor (~33ms)
//! to confirm the push path is genuinely faster.
//!
//! Run with:
//!   cargo xtask reaper-test -- csurf_subscribe_latency

use std::time::{Duration, Instant};

use daw::test::{DawInstanceConfig, run_multi_reaper_test};
use daw_proto::track::TrackEvent;
use eyre::Result;

/// Upper bound for end-to-end "mutate → event observed" latency.
///
/// The csurf callback itself runs synchronously inside `set_volume`,
/// so the dominant cost is the RPC round-trip into REAPER's main
/// thread (architect dispatcher tick, ~30ms) plus the vox forward
/// back to the test process. Budget is set above one dispatcher
/// tick to absorb that floor while still catching regressions
/// where the push path silently falls back to the 30Hz poller.
const FULL_MODE_BUDGET: Duration = Duration::from_millis(80);

#[test]
#[ignore]
fn csurf_subscribe_latency() -> Result<()> {
    run_multi_reaper_test(
        "csurf_subscribe_latency",
        vec![
            DawInstanceConfig::new("primary")
                .with_env("DISPLAY", "")
                .with_env("FTS_CSURF_MODE", "full")
                // Bump REAPER's misc timer from ~30Hz to 240Hz so the
                // architect dispatcher tick wait drops from 0..33ms to
                // 0..~4ms. Caller can override via $FTS_MISC_TIMER_HZ.
                .with_env(
                    "FTS_MISC_TIMER_HZ",
                    &std::env::var("FTS_MISC_TIMER_HZ").unwrap_or_else(|_| "240".to_string()),
                )
                .with_fts_config()
                .with_socket("/tmp/fts-daw-test-csurf-latency.sock"),
        ],
        |ctx| {
            Box::pin(async move {
                let inst = ctx.by_label("primary");
                let project = inst.daw.current_project().await?;
                let tracks = project.tracks();

                // Stand up a track to mutate.
                let probe = tracks.add("csurf_probe", None).await?;

                // Subscribe and drain anything queued (the Added event
                // from `add` above, etc.) so the latency measurement
                // starts from a clean slate.
                let mut rx = tracks.subscribe().await?;
                drain_for(&mut rx, Duration::from_millis(200)).await;

                // Warm-up: first sample after subscribe pays the
                // tokio task spawn + first vox encode cost (we saw
                // ~190ms on a cold path locally). Drop it from the
                // measured set, but still drain it so we don't match
                // against it during the timed samples.
                let target = 0.15;
                let t_warm = Instant::now();
                probe.set_volume(target).await?;
                let warm = wait_for_volume(&mut rx, probe.guid(), target, t_warm).await?;
                println!("  warmup: {:?}", warm);

                // Sample several mutations; report mean / p99-ish.
                let samples = 10;
                let mut latencies = Vec::with_capacity(samples);
                for i in 0..samples {
                    let target_vol = 0.2 + (i as f64) * 0.05;
                    let t0 = Instant::now();
                    probe.set_volume(target_vol).await?;
                    let elapsed = wait_for_volume(&mut rx, probe.guid(), target_vol, t0).await?;
                    latencies.push(elapsed);
                    println!("  sample {i}: {:?}", elapsed);
                }

                latencies.sort();
                let p50 = latencies[latencies.len() / 2];
                let p99 = latencies[latencies.len() - 1];
                let mean = latencies.iter().sum::<Duration>() / (latencies.len() as u32);
                println!(
                    "\n  csurf full-mode latency — mean: {mean:?}, p50: {p50:?}, p99: {p99:?}"
                );

                assert!(
                    p99 < FULL_MODE_BUDGET,
                    "p99 latency {p99:?} exceeded full-mode budget {FULL_MODE_BUDGET:?} \
                     (poller floor is ~33ms; full mode should beat that)"
                );

                Ok(())
            })
        },
    )
}

async fn drain_for(
    rx: &mut daw::rpc::EventStream<daw_proto::track::TrackStreamEvent>,
    window: Duration,
) {
    let deadline = Instant::now() + window;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if tokio::time::timeout(remaining, rx.recv()).await.is_err() {
            return;
        }
    }
}

async fn wait_for_volume(
    rx: &mut daw::rpc::EventStream<daw_proto::track::TrackStreamEvent>,
    guid: &str,
    expected: f64,
    t0: Instant,
) -> Result<Duration> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            eyre::bail!("timed out waiting for VolumeChanged({guid}, {expected})");
        }
        let envelope = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| eyre::eyre!("recv timeout"))??
            .ok_or_else(|| eyre::eyre!("subscriber closed before event"))?;
        let envelope = envelope.get();
        if let TrackEvent::VolumeChanged { guid: g, volume } = &envelope.event
            && g == guid
            && (volume - expected).abs() < 1e-4
        {
            return Ok(t0.elapsed());
        }
    }
}
