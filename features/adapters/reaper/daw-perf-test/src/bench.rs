//! Benchmark suite comparing native reaper-rs, individual RPC, and batch RPC.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use daw::rpc::{BatchBuilder, Daw};
use daw::service::batch::*;
use daw::service::track::TracksOp;

use tracing::info;

// ============================================================================
// Helpers
// ============================================================================

fn fmt_dur(d: Duration) -> String {
    if d.as_millis() >= 1000 {
        format!("{:.2}s", d.as_secs_f64())
    } else if d.as_millis() > 0 {
        format!("{:.2}ms", d.as_secs_f64() * 1000.0)
    } else {
        format!("{}µs", d.as_micros())
    }
}

fn log_result(label: &str, n: u32, elapsed: Duration) {
    info!(
        "  {label:<30} {n:>5} ops  {total:>10}  ({per_op}/op)",
        total = fmt_dur(elapsed),
        per_op = fmt_dur(elapsed / n),
    );
}

fn log_comparison(
    native: Duration,
    in_process: Duration,
    batch_rpc: Duration,
    individual: Duration,
) {
    let inproc_vs_native = in_process.as_secs_f64() / native.as_secs_f64();
    let batch_vs_native = batch_rpc.as_secs_f64() / native.as_secs_f64();
    let ind_vs_native = individual.as_secs_f64() / native.as_secs_f64();
    let batch_vs_inproc = batch_rpc.as_secs_f64() / in_process.as_secs_f64();
    let ind_vs_batch = individual.as_secs_f64() / batch_rpc.as_secs_f64();
    info!("  Ratios (vs native):");
    info!("    In-process batch / Native:  {inproc_vs_native:.1}x");
    info!("    Batch RPC / Native:         {batch_vs_native:.1}x");
    info!("    Individual RPC / Native:    {ind_vs_native:.1}x");
    info!("  Overhead breakdown:");
    info!("    Batch RPC / In-process:     {batch_vs_inproc:.1}x (= socket + serialization cost)");
    info!("    Individual RPC / Batch RPC: {ind_vs_batch:.1}x (= per-op RPC overhead)");
}

/// Connect to daw-bridge's Unix socket and return a Daw handle.
async fn connect_to_daw_bridge() -> eyre::Result<Daw> {
    let socket_path = if let Ok(path) = std::env::var("FTS_SOCKET") {
        PathBuf::from(path)
    } else {
        let pid = std::process::id();
        PathBuf::from(format!("/tmp/fts-daw-{pid}.sock"))
    };

    if !socket_path.exists() {
        return Err(eyre::eyre!(
            "daw-bridge socket not found at {}. Is daw-bridge loaded?",
            socket_path.display()
        ));
    }

    let stream = tokio::net::UnixStream::connect(&socket_path).await?;
    let link = vox_stream::StreamLink::unix(stream);

    // vox 0.10 lane model: establish the connection, then open the DAW
    // service lane (carries `vox-service: perf-test` automatically),
    // yielding a ready-to-use `Caller`.
    let connection = vox::initiator_on(link)
        .establish_connection()
        .await
        .map_err(|e| eyre::eyre!("Failed to establish vox connection: {:?}", e))?;
    let client = connection
        .open_lane::<PerfLaneClient>()
        .await
        .map_err(|e| eyre::eyre!("Failed to open DAW lane: {:?}", e))?;
    // The lane's Caller + its spawned driver keep the session alive.
    drop(connection);

    Ok(Daw::new(client.caller))
}

/// Minimal `FromVoxLane` client capturing the lane's `Caller`
/// (vox 0.10 replacement for the removed `NoopClient`).
#[derive(Clone)]
struct PerfLaneClient {
    caller: vox::Caller,
}

impl vox::FromVoxLane for PerfLaneClient {
    const SERVICE_NAME: &'static str = "perf-test";

    fn from_vox_lane(caller: vox::Caller, _connection: Option<vox::ConnectionHandle>) -> Self {
        Self { caller }
    }
}

// ============================================================================
// Native reaper-rs benchmarks (runs closures on main thread)
// ============================================================================

async fn native_create_tracks(n: u32) -> Duration {
    let start = Instant::now();
    daw::reaper::main_thread::query(move || {
        let reaper = reaper_high::Reaper::get();
        let project = reaper.current_project();
        for i in 0..n {
            let name = format!("NativeTrack-{i}");
            if let Ok(track) = project.insert_track_at(project.track_count()) {
                track.set_name(name.as_str());
            }
        }
    })
    .await;
    start.elapsed()
}

async fn native_remove_all_tracks() {
    daw::reaper::main_thread::query(move || {
        let reaper = reaper_high::Reaper::get();
        let project = reaper.current_project();
        while project.track_count() > 0 {
            if let Some(track) = project.track_by_index(project.track_count() - 1) {
                project.remove_track(&track);
            } else {
                break;
            }
        }
    })
    .await;
}

async fn native_mutate_tracks(n: u32) -> Duration {
    let start = Instant::now();
    daw::reaper::main_thread::query(move || {
        let reaper = reaper_high::Reaper::get();
        let project = reaper.current_project();
        let count = project.track_count().min(n);
        for i in 0..count {
            if let Some(track) = project.track_by_index(i) {
                let name = format!("NativeMutated-{i}");
                track.set_name(name.as_str());
                if let Ok(vol) = reaper_medium::ReaperVolumeValue::new(0.5 + (i as f64) * 0.001) {
                    let _ = track.set_volume_smart(vol, Default::default());
                }
                use reaper_high::GroupingBehavior;
                use reaper_medium::GangBehavior;
                if i % 2 == 0 {
                    track.mute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping);
                } else {
                    track.unmute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping);
                }
            }
        }
    })
    .await;
    start.elapsed()
}

async fn native_create_and_mutate_tracks(n: u32) -> Duration {
    let start = Instant::now();
    daw::reaper::main_thread::query(move || {
        let reaper = reaper_high::Reaper::get();
        let project = reaper.current_project();
        for i in 0..n {
            let name = format!("NativeCM-{i}");
            if let Ok(track) = project.insert_track_at(project.track_count()) {
                track.set_name(name.as_str());
                if let Ok(vol) = reaper_medium::ReaperVolumeValue::new(0.7) {
                    let _ = track.set_volume_smart(vol, Default::default());
                }
                use reaper_high::GroupingBehavior;
                use reaper_medium::GangBehavior;
                if i % 2 == 0 {
                    track.mute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping);
                } else {
                    track.unmute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping);
                }
            }
        }
    })
    .await;
    start.elapsed()
}

async fn native_add_markers(n: u32) -> Duration {
    let start = Instant::now();
    daw::reaper::main_thread::query(move || {
        let reaper = reaper_high::Reaper::get();
        let medium = reaper.medium_reaper();
        for i in 0..n {
            if let Ok(pos) = reaper_medium::PositionInSeconds::new(i as f64 * 0.5) {
                let name = format!("NativeMarker-{i}");
                let _ = medium.add_project_marker_2(
                    reaper_medium::ProjectContext::CurrentProject,
                    reaper_medium::MarkerOrRegionPosition::Marker(pos),
                    name.as_str(),
                    None,
                    None,
                );
            }
        }
    })
    .await;
    start.elapsed()
}

async fn native_remove_all_markers() {
    daw::reaper::main_thread::query(move || {
        use daw::reaper::safe_wrappers::markers as markers_sw;
        let reaper = reaper_high::Reaper::get();
        let low = reaper.medium_reaper().low();
        let rctx = reaper_medium::ProjectContext::CurrentProject;
        let mut ids = Vec::new();
        let mut idx = 0;
        while let Some(result) = markers_sw::enum_project_markers(low, idx) {
            idx += 1;
            if !result.is_region {
                ids.push(result.marker_idx);
            }
        }
        for id in ids.into_iter().rev() {
            markers_sw::delete_project_marker(low, rctx, id, false);
        }
    })
    .await;
}

// ============================================================================
// In-process batch benchmarks (direct BatchExecution, no socket/serialization)
// ============================================================================

async fn inproc_create_tracks(executor: &daw_reaper::Reaper, n: u32) -> Duration {
    let mut b = BatchBuilder::new().with_undo("In-proc create tracks");
    let proj = b.current_project();
    for i in 0..n {
        b.add_track(&proj, format!("InprocTrack-{i}"), None);
    }
    let start = Instant::now();
    BatchExecution::execute(executor, b.build());
    start.elapsed()
}

async fn inproc_mutate_tracks(
    executor: &daw_reaper::Reaper,
    daw: &Daw,
    n: u32,
) -> eyre::Result<Duration> {
    let project = daw.current_project().await?;
    let all = project.tracks().all().await?;
    let count = (all.len() as u32).min(n);

    let mut b = BatchBuilder::new().with_undo("In-proc mutate tracks");
    let proj = b.current_project();
    for i in 0..count {
        let tref = daw::service::TrackRef::Guid(all[i as usize].guid.clone());
        b.rename_track(&proj, tref.clone(), format!("InprocMutated-{i}"));
        b.set_track_volume(&proj, tref.clone(), 0.5 + (i as f64) * 0.001);
        b.push_unchecked(BatchOp::Track(TracksOp::SetMuted {
            project: ProjectArg::FromStep(proj.index()),
            track: TrackArg::Literal(tref),
            muted: i % 2 == 0,
        }));
    }

    let start = Instant::now();
    BatchExecution::execute(executor, b.build());
    Ok(start.elapsed())
}

async fn inproc_create_and_mutate(executor: &daw_reaper::Reaper, n: u32) -> Duration {
    let mut b = BatchBuilder::new().with_undo("In-proc create+mutate");
    let proj = b.current_project();

    let mut handles = Vec::with_capacity(n as usize);
    for i in 0..n {
        let h = b.add_track(&proj, format!("InprocCM-{i}"), None);
        handles.push(h);
    }
    for (i, handle) in handles.iter().enumerate() {
        b.push_unchecked(BatchOp::Track(TracksOp::SetVolume {
            project: ProjectArg::FromStep(proj.index()),
            track: TrackArg::FromStep(handle.index()),
            volume: 0.7,
        }));
        b.push_unchecked(BatchOp::Track(TracksOp::SetMuted {
            project: ProjectArg::FromStep(proj.index()),
            track: TrackArg::FromStep(handle.index()),
            muted: i % 2 == 0,
        }));
    }

    let start = Instant::now();
    BatchExecution::execute(executor, b.build());
    start.elapsed()
}

async fn inproc_add_markers(executor: &daw_reaper::Reaper, n: u32) -> Duration {
    let mut b = BatchBuilder::new().with_undo("In-proc add markers");
    let proj = b.current_project();
    for i in 0..n {
        b.add_marker(&proj, i as f64 * 0.5, format!("InprocMarker-{i}"));
    }
    let start = Instant::now();
    BatchExecution::execute(executor, b.build());
    start.elapsed()
}

// ============================================================================
// Individual RPC benchmarks
// ============================================================================

async fn rpc_create_tracks(daw: &Daw, n: u32) -> eyre::Result<Duration> {
    let project = daw.current_project().await?;
    let tracks = project.tracks();
    let start = Instant::now();
    for i in 0..n {
        tracks.add(&format!("RpcTrack-{i}"), None).await?;
    }
    Ok(start.elapsed())
}

async fn rpc_remove_all_tracks(daw: &Daw) -> eyre::Result<()> {
    let project = daw.current_project().await?;
    project.tracks().remove_all().await?;
    Ok(())
}

async fn rpc_mutate_tracks(daw: &Daw, n: u32) -> eyre::Result<Duration> {
    let project = daw.current_project().await?;
    let tracks_svc = project.tracks();
    let all = tracks_svc.all().await?;
    let count = (all.len() as u32).min(n);
    let start = Instant::now();
    for i in 0..count {
        let track = &all[i as usize];
        let handle = tracks_svc
            .by_guid(&track.guid)
            .await?
            .expect("track should exist");
        handle.rename(&format!("RpcMutated-{i}")).await?;
        handle.set_volume(0.5 + (i as f64) * 0.001).await?;
        if i % 2 == 0 {
            handle.mute().await?;
        } else {
            handle.unmute().await?;
        }
    }
    Ok(start.elapsed())
}

// ============================================================================
// Batch RPC benchmarks
// ============================================================================

async fn batch_create_tracks(daw: &Daw, n: u32) -> eyre::Result<Duration> {
    let mut b = BatchBuilder::new().with_undo("Batch create tracks");
    let proj = b.current_project();
    for i in 0..n {
        b.add_track(&proj, format!("BatchTrack-{i}"), None);
    }
    let start = Instant::now();
    let response = daw.execute_batch(b.build()).await?;
    let elapsed = start.elapsed();

    let errors: Vec<_> = response
        .results
        .iter()
        .filter(|r| matches!(r.outcome, StepOutcome::Error(_)))
        .collect();
    if !errors.is_empty() {
        return Err(eyre::eyre!(
            "Batch create tracks had {} errors",
            errors.len()
        ));
    }
    Ok(elapsed)
}

async fn batch_mutate_tracks(daw: &Daw, n: u32) -> eyre::Result<Duration> {
    let project = daw.current_project().await?;
    let all = project.tracks().all().await?;
    let count = (all.len() as u32).min(n);

    let mut b = BatchBuilder::new().with_undo("Batch mutate tracks");
    let proj = b.current_project();
    for i in 0..count {
        let tref = daw::service::TrackRef::Guid(all[i as usize].guid.clone());
        b.rename_track(&proj, tref.clone(), format!("BatchMutated-{i}"));
        b.set_track_volume(&proj, tref.clone(), 0.5 + (i as f64) * 0.001);
        b.push_unchecked(BatchOp::Track(TracksOp::SetMuted {
            project: ProjectArg::FromStep(proj.index()),
            track: TrackArg::Literal(tref),
            muted: i % 2 == 0,
        }));
    }

    let start = Instant::now();
    let response = daw.execute_batch(b.build()).await?;
    let elapsed = start.elapsed();

    let errors: Vec<_> = response
        .results
        .iter()
        .filter(|r| matches!(r.outcome, StepOutcome::Error(_)))
        .collect();
    if !errors.is_empty() {
        return Err(eyre::eyre!(
            "Batch mutate tracks had {} errors",
            errors.len()
        ));
    }
    Ok(elapsed)
}

async fn rpc_add_markers(daw: &Daw, n: u32) -> eyre::Result<Duration> {
    let project = daw.current_project().await?;
    let markers = project.markers();
    let start = Instant::now();
    for i in 0..n {
        markers
            .add(i as f64 * 0.5, &format!("RpcMarker-{i}"))
            .await?;
    }
    Ok(start.elapsed())
}

async fn rpc_remove_all_markers(daw: &Daw) -> eyre::Result<()> {
    let project = daw.current_project().await?;
    let markers = project.markers();
    let all = markers.all().await?;
    for m in all.iter().rev() {
        if let Some(id) = m.id {
            markers.remove(id).await?;
        }
    }
    Ok(())
}

async fn batch_add_markers(daw: &Daw, n: u32) -> eyre::Result<Duration> {
    let mut b = BatchBuilder::new().with_undo("Batch add markers");
    let proj = b.current_project();
    for i in 0..n {
        b.add_marker(&proj, i as f64 * 0.5, format!("BatchMarker-{i}"));
    }
    let start = Instant::now();
    let response = daw.execute_batch(b.build()).await?;
    let elapsed = start.elapsed();

    let errors: Vec<_> = response
        .results
        .iter()
        .filter(|r| matches!(r.outcome, StepOutcome::Error(_)))
        .collect();
    if !errors.is_empty() {
        return Err(eyre::eyre!("Batch add markers had {} errors", errors.len()));
    }
    Ok(elapsed)
}

async fn batch_create_and_mutate(daw: &Daw, n: u32) -> eyre::Result<Duration> {
    let mut b = BatchBuilder::new().with_undo("Batch create+mutate");
    let proj = b.current_project();

    // Create N tracks and immediately mutate each via FromStep reference
    let mut handles = Vec::with_capacity(n as usize);
    for i in 0..n {
        let h = b.add_track(&proj, format!("BatchCM-{i}"), None);
        handles.push(h);
    }
    for (i, handle) in handles.iter().enumerate() {
        b.push_unchecked(BatchOp::Track(TracksOp::SetVolume {
            project: ProjectArg::FromStep(proj.index()),
            track: TrackArg::FromStep(handle.index()),
            volume: 0.7,
        }));
        b.push_unchecked(BatchOp::Track(TracksOp::SetMuted {
            project: ProjectArg::FromStep(proj.index()),
            track: TrackArg::FromStep(handle.index()),
            muted: i % 2 == 0,
        }));
    }

    let total_ops = n * 3; // create + volume + mute
    let start = Instant::now();
    let response = daw.execute_batch(b.build()).await?;
    let elapsed = start.elapsed();

    let errors: Vec<_> = response
        .results
        .iter()
        .filter(|r| matches!(r.outcome, StepOutcome::Error(_)))
        .collect();
    if !errors.is_empty() {
        for e in &errors[..errors.len().min(3)] {
            info!("  error at step {}: {:?}", e.step, e.outcome);
        }
        return Err(eyre::eyre!(
            "Batch create+mutate had {} errors",
            errors.len()
        ));
    }

    info!(
        "  (create+mutate batch: {} ops in 1 RPC, {}/op)",
        total_ops,
        fmt_dur(elapsed / total_ops)
    );
    Ok(elapsed)
}

// ============================================================================
// Main benchmark suite
// ============================================================================

pub async fn run_all() -> eyre::Result<()> {
    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║      DAW Performance Benchmark Suite (4-way comparison)     ║");
    info!("║  Native | In-Process Batch | Batch RPC | Individual RPC    ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("");

    // Connect to daw-bridge over Unix socket (for RPC benchmarks)
    info!("Connecting to daw-bridge socket...");
    let daw = connect_to_daw_bridge().await?;
    info!("Connected.");

    // In-process executor: the Reaper backend itself (bypasses socket entirely)
    let executor = daw_reaper::Reaper;
    info!("In-process batch executor ready.");
    info!("");

    // Clean slate
    rpc_remove_all_tracks(&daw).await?;

    // ── Benchmark 1: Create tracks ──────────────────────────────────────
    for &n in &[100u32, 500] {
        info!("── Create {n} tracks ──────────────────────────────");

        let native_elapsed = native_create_tracks(n).await;
        log_result("Native reaper-rs", n, native_elapsed);
        native_remove_all_tracks().await;

        let inproc_elapsed = inproc_create_tracks(&executor, n).await;
        log_result("In-process batch", n, inproc_elapsed);
        native_remove_all_tracks().await;

        let batch_elapsed = batch_create_tracks(&daw, n).await?;
        log_result("Batch RPC", n, batch_elapsed);
        rpc_remove_all_tracks(&daw).await?;

        let rpc_elapsed = rpc_create_tracks(&daw, n).await?;
        log_result("Individual RPC", n, rpc_elapsed);
        rpc_remove_all_tracks(&daw).await?;

        log_comparison(native_elapsed, inproc_elapsed, batch_elapsed, rpc_elapsed);
        info!("");
    }

    // ── Benchmark 2: Mutate existing tracks (rename + volume + mute) ────
    for &n in &[100u32, 200] {
        info!("── Mutate {n} tracks (3 ops each) ────────────────");

        // Setup N tracks via native
        native_create_tracks(n).await;

        let native_elapsed = native_mutate_tracks(n).await;
        log_result("Native reaper-rs", n * 3, native_elapsed);

        let inproc_elapsed = inproc_mutate_tracks(&executor, &daw, n).await?;
        log_result("In-process batch", n * 3, inproc_elapsed);

        let batch_elapsed = batch_mutate_tracks(&daw, n).await?;
        log_result("Batch RPC", n * 3, batch_elapsed);

        let rpc_elapsed = rpc_mutate_tracks(&daw, n).await?;
        log_result("Individual RPC", n * 3, rpc_elapsed);

        log_comparison(native_elapsed, inproc_elapsed, batch_elapsed, rpc_elapsed);
        info!("");

        native_remove_all_tracks().await;
    }

    // ── Benchmark 3: Create + mutate in single batch (FromStep chains) ──
    for &n in &[100u32, 500] {
        info!("── Create + mutate {n} tracks (3 ops each) ──────────");

        let native_elapsed = native_create_and_mutate_tracks(n).await;
        log_result("Native reaper-rs", n * 3, native_elapsed);
        native_remove_all_tracks().await;

        let inproc_elapsed = inproc_create_and_mutate(&executor, n).await;
        log_result("In-process batch", n * 3, inproc_elapsed);
        native_remove_all_tracks().await;

        let batch_elapsed = batch_create_and_mutate(&daw, n).await?;
        log_result("Batch RPC", n * 3, batch_elapsed);
        rpc_remove_all_tracks(&daw).await?;

        // Individual RPC: sequential create + mutate
        let seq_start = Instant::now();
        {
            let project = daw.current_project().await?;
            let tracks = project.tracks();
            for i in 0..n {
                let handle = tracks.add(&format!("SeqCM-{i}"), None).await?;
                handle.set_volume(0.7).await?;
                if i % 2 == 0 {
                    handle.mute().await?;
                }
            }
        }
        let seq_elapsed = seq_start.elapsed();
        log_result("Individual RPC", n * 3, seq_elapsed);
        rpc_remove_all_tracks(&daw).await?;

        log_comparison(native_elapsed, inproc_elapsed, batch_elapsed, seq_elapsed);
        info!("");
    }

    // ── Benchmark 4: Add markers in bulk ────────────────────────────────
    for &n in &[200u32, 500] {
        info!("── Add {n} markers ──────────────────────────────────");

        let native_elapsed = native_add_markers(n).await;
        log_result("Native reaper-rs", n, native_elapsed);
        native_remove_all_markers().await;

        let inproc_elapsed = inproc_add_markers(&executor, n).await;
        log_result("In-process batch", n, inproc_elapsed);
        native_remove_all_markers().await;

        let rpc_elapsed = rpc_add_markers(&daw, n).await?;
        log_result("Individual RPC", n, rpc_elapsed);
        rpc_remove_all_markers(&daw).await?;

        let batch_elapsed = batch_add_markers(&daw, n).await?;
        log_result("Batch RPC", n, batch_elapsed);

        log_comparison(native_elapsed, inproc_elapsed, batch_elapsed, rpc_elapsed);
        info!("");

        rpc_remove_all_markers(&daw).await?;
    }

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║          Benchmarks complete — see /tmp/daw-perf-test.log   ║");
    info!("╚══════════════════════════════════════════════════════════════╝");

    Ok(())
}
