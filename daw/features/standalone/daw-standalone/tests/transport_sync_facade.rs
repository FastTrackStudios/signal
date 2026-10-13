//! The `TransportSync` facade end to end: a standalone backend served
//! over the in-process link, followed by a client that only speaks
//! `daw_control` — its clock pinged, its positions streamed, and a
//! second standalone engine kept on it by a `Follower`.

#![cfg(feature = "bootstrap")]
#![allow(clippy::cast_precision_loss, clippy::float_cmp)]

use std::sync::Arc;
use std::time::Duration;

use daw_proto::{ProjectInfo, StampedPosition};
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;
use daw_standalone::transport_engine::TransportShared;
use daw_standalone::transport_sync::now_micros;
use daw_transport_sync::{ClockEstimator, Correction, Follower, TransportBackend};

const SR: f64 = 48_000.0;

fn seeded(guid: &str) -> Standalone {
    let s = Standalone::new();
    s.seed_project(ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    });
    s
}

/// The next position off `stream` that satisfies `want`, within 3 s.
async fn next_where(
    stream: &mut daw_control::EventStream<StampedPosition>,
    want: impl Fn(&StampedPosition) -> bool,
) -> StampedPosition {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let p = stream
                .recv()
                .await
                .expect("stream open")
                .expect("stream not ended")
                .get()
                .clone();
            if want(&p) {
                return p;
            }
        }
    })
    .await
    .expect("position within 3 s")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_server_clock_is_this_process_clock() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded("p")).await?;
    let sync = bundle.daw.transport_sync();
    let mut clock = ClockEstimator::default();
    for _ in 0..32 {
        let (t1, server, t4) = sync.ping(now_micros).await?;
        // One process, one clock: the server read it between our two
        // reads.
        assert!(t1 <= server && server <= t4, "{t1} ≤ {server} ≤ {t4}");
        clock.record(t1, server, server, t4);
    }
    let offset = clock.offset_micros().unwrap();
    let rtt = clock.round_trip_micros().unwrap();
    assert!(
        offset.abs() <= rtt / 2.0 + 1.0,
        "offset {offset} µs, rtt {rtt} µs"
    );
    assert!(offset.abs() < 1_000.0, "offset {offset} µs");
    Ok(())
}

/// Play and stop reach the stream at once, and between changes each
/// position is where the last one projects to at its time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn positions_flip_on_play_and_advance_with_their_clock() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded("p")).await?;
    let project = bundle.daw.current_project().await?;
    let sync = project.transport_sync();

    // The one-shot read starts the project's transport (its soft clock).
    let mut first = None;
    for _ in 0..50 {
        first = sync.snapshot().await?;
        if first.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let first = first.expect("a snapshot once the transport runs");
    assert_eq!(first.project_guid, "p");
    assert!(!first.is_playing);

    let mut positions = sync.positions();
    let stopped = next_where(&mut positions, |_| true).await;
    assert!(!stopped.is_playing);
    assert_eq!(stopped.project_guid, "p");

    project.transport().play().await?;
    let started = next_where(&mut positions, |p| p.is_playing).await;
    let mut run = vec![started];
    while run.len() < 40 {
        run.push(next_where(&mut positions, |_| true).await);
    }
    project.transport().stop().await?;
    let halted = next_where(&mut positions, |p| !p.is_playing).await;
    assert!(halted.host_micros > run.last().unwrap().host_micros);

    let mut gaps = Vec::new();
    for w in run.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        assert!(b.is_playing, "still playing");
        assert!(b.sequence > a.sequence && b.host_micros > a.host_micros);
        // Projecting one to the next's time lands on it: the soft clock
        // rounds each tick to whole frames, half a sample at most.
        let off = (a.position().at(b.host_micros) - b.playhead_seconds).abs() * SR;
        assert!(off < 2.0, "projection off by {off} samples");
        gaps.push(b.host_micros - a.host_micros);
    }
    // A keepalive at least every ~20 ms of the stream's own clock (the
    // soft clock ticks every 10 ms).
    gaps.sort_by(f64::total_cmp);
    let median = gaps[gaps.len() / 2];
    assert!(median <= 33_000.0, "median gap {median} µs");
    Ok(())
}

/// A stand-in audio device: opens a `frames`-long buffer on `shared`
/// every period in real time, stamped exactly `period` apart from
/// `t0` — the steady device clock a `BufferClock` would recover.
fn drive(shared: Arc<TransportShared>, t0: f64, frames: u32) -> tokio::task::JoinHandle<()> {
    let period = f64::from(frames) / SR * 1e6;
    let start = tokio::time::Instant::now();
    let start_micros = now_micros();
    tokio::spawn(async move {
        for k in 0_u32.. {
            let stamp = f64::from(k).mul_add(period, t0);
            let due = start + Duration::from_micros((stamp - start_micros).max(0.0) as u64);
            tokio::time::sleep_until(due).await;
            shared.begin_block(frames, stamp, period);
        }
    })
}

/// A second standalone engine, stopped, is brought onto a playing one
/// through the facade alone (clock pings + the positions stream) and
/// plays sample-aligned with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_follower_locks_onto_a_leader_through_the_facade() -> eyre::Result<()> {
    // The leader, served; its transport driven by a stand-in device.
    let served = seeded("leader");
    let bundle = build_in_process_daw(served.clone()).await?;
    let lead_engine = served.transport_engine_for("leader");
    lead_engine.disable_soft_clock();

    // The follower: another engine in this process, not served.
    let local = seeded("follower");
    let backend = local.sync_backend("follower").expect("project open");
    local.transport_engine_for("follower").disable_soft_clock();

    // Let a soft-clock tick already in flight finish before the devices
    // take over (one driver per transport).
    tokio::time::sleep(Duration::from_millis(30)).await;
    let t0 = now_micros() + 5_000.0;
    let lead_device = drive(lead_engine.shared.clone(), t0, 256);
    // Its device starts buffers 3.1 ms off the leader's.
    let follow_device = drive(backend.shared().clone(), t0 + 3_100.0, 256);

    let project = bundle.daw.project("leader").await?;
    project.transport().set_position(10.0).await?;
    project.transport().play().await?;

    let leader = project.transport_sync().leader(now_micros);
    let mut follower = Follower::default();
    let mut locates = 0;
    for _ in 0..150 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if let Some(Correction::Locate { .. }) = leader.tick(&mut follower, &backend) {
            locates += 1;
        }
    }
    lead_device.abort();
    follow_device.abort();

    assert!(
        leader.clock_samples() >= 10,
        "pinged {}",
        leader.clock_samples()
    );
    let offset = leader.offset_micros().unwrap();
    assert!(offset.abs() < 1_000.0, "offset {offset} µs in one process");
    assert!(locates >= 1, "started by a locate");

    // Where the leader really is (its engine, not the stream), against
    // where the follower is, at the follower's last buffer.
    let lead = lead_engine.shared.sync_snapshot().unwrap().position();
    let mine = backend.snapshot().unwrap();
    assert!(mine.is_playing);
    let gap = (mine.playhead_seconds - lead.at(mine.host_micros)).abs() * SR;
    assert!(
        gap < 4.0,
        "{gap} samples apart (offset estimate {offset} µs, {locates} locates)"
    );
    // The stream's view agrees with the engine's.
    let streamed = leader.position().unwrap();
    let seen = (streamed.at(lead.host_micros) - lead.playhead_seconds).abs() * SR;
    assert!(seen < 0.5, "stream vs engine: {seen} samples");
    Ok(())
}
