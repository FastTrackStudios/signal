//! Full workstation DOM benchmark. Run through `just ee-stress` for safe song copies.
#[path = "stress_support/input.rs"]
mod input;

use dioxus_test::{DocumentTester, by_testid};
use expression_editor_standalone::{
    Args, Runner,
    workstation::{WorkstationApp, bootstrap_daw_blocking, stage_workstation},
};
use serde_json::json;
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn main() -> eyre::Result<()> {
    // `RUST_LOG` decides what is shown; without a subscriber the spans
    // this crate emits — including what opening the kit cost — go
    // nowhere. Off unless asked for, so a benchmark run stays quiet.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let args = Args::from_env().map_err(|error| eyre::eyre!("{error}"))?;
    let frames: usize = std::env::var("FTS_STRESS_FRAMES")
        .unwrap_or_else(|_| "120".into())
        .parse()?;
    eyre::ensure!(frames >= 2, "FTS_STRESS_FRAMES must be at least 2");
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("target/ui-stress"));
    std::fs::create_dir_all(&out)?;
    eprintln!("Loading drum project…");
    let runner = Runner::open(&args.source, &args.target, args.viewport(), args.mode)?;
    let standalone = runner
        .daw
        .clone()
        .ok_or_else(|| eyre::eyre!("stress needs a project"))?;
    bootstrap_daw_blocking(&standalone)?;
    stage_workstation(
        runner.loaded.into_editor(),
        runner.host,
        (args.width as f64, args.height as f64),
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(benchmark(&args, frames, &out))
}

async fn benchmark(args: &Args, frames: usize, out: &Path) -> eyre::Result<()> {
    let tester = DocumentTester::from_element(WorkstationApp)
        .with_window_size(args.width, args.height)
        .build();
    eprintln!("Waiting for workstation tracks and waveform previews…");
    let started = Instant::now();
    loop {
        tester.drain();
        tester.relayout();
        if tester
            .query(by_testid("workstation-ready"))
            .immediately()
            .is_ok()
        {
            break;
        }
        eyre::ensure!(
            started.elapsed() < Duration::from_secs(600),
            "waveform readiness timeout"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let project_shape = tester
        .query(by_testid("workstation-ready"))
        .immediately()
        .unwrap()
        .inner_html();
    eprintln!("Ready: {project_shape}");
    tester
        .query(by_testid("stack-cell"))
        .immediately()
        .unwrap()
        .focus();
    let points = input::points(&tester);
    // Validate each gesture before timing: silently ignored input is not a benchmark.
    for phase in &input::PHASES[..7] {
        let before = input::observe(&tester, phase);
        input::frame(&tester, &points, phase, 0);
        tester.drain();
        tester.relayout();
        if before == input::observe(&tester, phase) {
            tester.render_png(out.join("failed-gesture.png"));
            eyre::bail!("{phase} did not change its pane; input points: {points:?}");
        }
    }
    // An empty variable is "no phase", not a phase named "" — `just
    // ee-bench` passes the flag through unconditionally.
    let selected_phase = std::env::var("FTS_STRESS_PHASE")
        .ok()
        .filter(|phase| !phase.is_empty());
    if let Some(ref phase) = selected_phase {
        eyre::ensure!(
            input::PHASES.contains(&phase.as_str()),
            "unknown phase: {phase}"
        );
    }
    let mut journal = std::fs::File::create(out.join("samples.jsonl"))?;
    let mut samples = Vec::new();
    // How many frames of each phase actually moved something.
    //
    // The preflight above proves a gesture works ONCE, from the state
    // the load left behind. It cannot prove the gesture keeps working:
    // a camera that reaches the end of the song, or a lane stack that
    // fits its pane, stops moving, and every frame after that is a few
    // hundred microseconds of nothing. Those frames make a median look
    // superb while measuring an idle window — the exact result the guide
    // says is a test failure, not progress. So the effectiveness check
    // runs per frame, outside all three timed stages, and the report
    // carries the count.
    let mut effective: std::collections::HashMap<&str, usize> = Default::default();
    for phase in input::PHASES.iter().filter(|phase| {
        selected_phase
            .as_deref()
            .is_none_or(|selected| selected == **phase)
    }) {
        eprintln!("DOM stress: {phase} ({frames} frames)");
        let mut previous = input::observe(&tester, phase);
        for frame in 0..frames {
            let start = Instant::now();
            input::frame(&tester, &points, phase, frame);
            let event_ms = start.elapsed().as_secs_f64() * 1000.0;
            let update = Instant::now();
            tester.drain();
            let update_ms = update.elapsed().as_secs_f64() * 1000.0;
            let layout = Instant::now();
            tester.relayout();
            let layout_ms = layout.elapsed().as_secs_f64() * 1000.0;
            let total_ms = start.elapsed().as_secs_f64() * 1000.0;
            samples.push(json!({
                "phase": phase, "frame": frame,
                "event_ms": event_ms, "update_ms": update_ms,
                "layout_ms": layout_ms, "total_ms": total_ms,
            }));
            // Preserve completed frames even if a later renderer mutation panics.
            serde_json::to_writer(&mut journal, samples.last().unwrap())?;
            writeln!(journal)?;
            let now = input::observe(&tester, phase);
            if now != previous {
                *effective.entry(phase).or_default() += 1;
                previous = now;
            }
            tokio::task::yield_now().await;
        }
    }
    // A phase that moved nothing is only a *failure* when its timings
    // would also read as a success. A clamped zoom still re-renders and
    // re-resolves the whole pane — that costs what it costs, and the
    // number is honest even though the DOM lands where it started. An
    // idle pane that never moved and never cost anything is the one that
    // turns a blank view into a green report.
    let budget = 1000.0 / 120.0;
    let idle: Vec<String> = input::PHASES
        .iter()
        .copied()
        .filter(|phase| {
            let mut timings: Vec<f64> = samples
                .iter()
                .filter(|s| s["phase"] == *phase)
                .filter_map(|s| s["total_ms"].as_f64())
                .collect();
            if timings.is_empty() || effective.get(phase).copied().unwrap_or(0) > 0 {
                return false;
            }
            timings.sort_by(f64::total_cmp);
            timings[timings.len() / 2] < budget
        })
        .map(|phase| phase.to_owned())
        .collect();
    // PNG compression and file IO are deliberately outside the timed workload.
    tester.render_png(out.join("workstation.png"));
    // How big the document actually is, measured after the run and
    // outside every timed stage. The layout stage is the one that scales
    // with it, so a change that claims to have cut the DOM down should
    // be able to show this number falling.
    let dom_nodes: serde_json::Map<String, serde_json::Value> = [
        "workstation-arrange",
        "workstation-timeline",
        "workstation-mixer",
        "stack-cell",
    ]
    .into_iter()
    .filter_map(|id| {
        let el = tester.query(by_testid(id)).immediately().ok()?;
        Some((id.to_owned(), json!(el.outer_html().matches('<').count())))
    })
    .collect();
    let report = json!({
        "schema": 1,
        "measurement": "headless DOM event + update + layout; excludes paint and presentation",
        "target_hz": 120,
        "budget_ms": 1000.0 / 120.0,
        "frames_per_phase": frames,
        "selected_phase": selected_phase,
        "dom_nodes": dom_nodes,
        "effective_frames": effective,
        "project_shape": project_shape,
        "viewport": [args.width, args.height],
        "project": format!("{:?}", args.source),
        "profile": std::env::var("FTS_STRESS_PROFILE").unwrap_or_else(|_| "unknown".into()),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "loadavg": std::fs::read_to_string("/proc/loadavg").ok(),
        "cpu": std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
            s.lines().find(|l| l.starts_with("model name")).map(str::to_owned)
        }),
        "samples": samples,
    });
    std::fs::write(
        out.join("samples.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", out.display());
    eyre::ensure!(
        idle.is_empty(),
        "these phases never moved their pane and cost less than the frame budget, \
         so their timings measure an idle window rather than the workload: {}",
        idle.join(", ")
    );
    Ok(())
}
