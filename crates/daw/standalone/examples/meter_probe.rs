//! Probe the `Peaks::meters` `#[subscribe]` stream end-to-end,
//! in-process: seed a project, install a meter bank, write levels into
//! it the way the audio mixer does (atomics, once per "block"), and
//! verify a `daw_control` client receives ~30 Hz [`MeterFrame`]s for
//! the right project with the written levels.
//!
//! ```bash
//! cargo run -p daw-standalone --features bootstrap,audio --example meter_probe
//! ```
//!
//! This exercises everything the mixer UI depends on except the cpal
//! callback itself (whose bank writes are the long-standing
//! `flush_meters` path): bank → meter pump → `PubSub` hub →
//! `peak::StreamService` → vox link → `Project::meter_events()` with
//! client-side project filtering.

use daw_proto::ProjectInfo;
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::metering::{HOLD_DECAY, Meters};
use daw_standalone::sync::Standalone;

fn main() -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    rt.block_on(run())
}

async fn run() -> eyre::Result<()> {
    // ── Backend: one project, a live meter bank of 3 tracks ──────────
    let standalone = Standalone::new();
    let guid = standalone.seed_project(ProjectInfo {
        guid: "meter-probe".into(),
        name: "Meter Probe".into(),
        path: String::new(),
    });
    let meters = Meters::new(3);
    standalone.set_meters(meters.clone()); // spawns the ~30 Hz pump

    // Simulated audio callback: write a distinct, moving level per
    // track every 10 ms (≈ one 480-sample block at 48 kHz).
    let bank = meters.clone();
    std::thread::spawn(move || {
        let mut n = 0u32;
        loop {
            let sweep = 0.5 + 0.4 * ((n as f32) * 0.05).sin();
            for i in 0..bank.len() {
                if let Some(cell) = bank.cell(i) {
                    let level = sweep * (i as f32 + 1.0) / 3.0;
                    cell.write(level, level * 0.5, HOLD_DECAY);
                }
            }
            n += 1;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // ── Client: in-process daw over a vox memory link ─────────────────
    let bundle = build_in_process_daw(standalone.clone()).await?;
    let project = bundle.daw.project(&guid).await?;
    let mut stream = project.meter_events();

    println!("subscribed to Peaks::meters for project {guid:?}; waiting for frames…");
    let mut frames = 0usize;
    let started = std::time::Instant::now();
    while frames < 10 {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), stream.recv())
            .await
            .map_err(|_| eyre::eyre!("no meter frame within 5s"))??
            .ok_or_else(|| eyre::eyre!("meter stream closed"))?;
        let f = frame.get();
        assert_eq!(f.project_guid, guid, "client-side project filter");
        assert_eq!(f.tracks.len(), 3, "one TrackLevels per bank cell");
        // Track 2 always writes 3x track 0's level — verify ordering.
        let (t0, t2) = (&f.tracks[0], &f.tracks[2]);
        assert!(
            t2.peak_left >= t0.peak_left,
            "track order preserved: {} < {}",
            t2.peak_left,
            t0.peak_left
        );
        assert!(t0.hold_left >= t0.peak_left, "hold never below peak");
        println!(
            "frame {frames}: L peaks = [{:.3} {:.3} {:.3}] holds = [{:.3} {:.3} {:.3}]",
            f.tracks[0].peak_left,
            f.tracks[1].peak_left,
            f.tracks[2].peak_left,
            f.tracks[0].hold_left,
            f.tracks[1].hold_left,
            f.tracks[2].hold_left,
        );
        frames += 1;
    }
    let hz = frames as f64 / started.elapsed().as_secs_f64();
    println!("received {frames} frames at ≈{hz:.1} Hz — meter stream OK");
    assert!(hz > 5.0, "pump should tick ~30 Hz, got {hz:.1}");

    // Levels must actually MOVE across frames (the sweep) — grab two
    // more frames a beat apart and compare.
    let a = next_peak(&mut stream).await?;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let b = next_peak(&mut stream).await?;
    assert!(
        (a - b).abs() > 1e-4,
        "levels move during playback: {a} vs {b}"
    );
    println!("levels move over time ({a:.3} → {b:.3}) — OK");
    Ok(())
}

async fn next_peak(
    stream: &mut daw_control::EventStream<daw_proto::MeterFrame>,
) -> eyre::Result<f32> {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), stream.recv())
        .await
        .map_err(|_| eyre::eyre!("no meter frame within 5s"))??
        .ok_or_else(|| eyre::eyre!("meter stream closed"))?;
    Ok(frame.get().tracks[0].peak_left)
}
