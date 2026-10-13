//! Measure a prepared session's playback: how long each render block
//! takes against its real-time budget, and where the generated click
//! actually sounds against the tempo grid. For looking at, not CI:
//! `FTS_CHART_PROJECT=song.rpp FTS_CHART=song.kf`.

use std::time::Instant;

use daw::service::{ProjectContext, TempoMap, TrackRef, Tracks};
use daw::standalone::audio_engine::render::ProjectRenderer;

const RATE: u32 = 48_000;
const BLOCK: usize = 512;

#[test]
fn a_prepared_session_renders_in_time_and_clicks_on_the_grid() {
    let (Some(project_file), Some(chart)) = (
        std::env::var_os("FTS_CHART_PROJECT"),
        std::env::var_os("FTS_CHART"),
    ) else {
        return;
    };
    let opened = session_daw::open::open_silent(std::path::Path::new(&project_file)).expect("open");
    session_daw::prepare::Prepare {
        organize: true,
        chart: Some(chart.into()),
        guide: true,
    }
    .run(&opened)
    .expect("prepare");
    let _rt = session_daw::open::runtime().expect("runtime").enter();
    let daw = opened.daw.clone();
    let project = ProjectContext::Project(opened.project_guid.clone());
    let budget_ms = BLOCK as f64 * 1000.0 / f64::from(RATE);

    // 1. Render cost, the whole mix, first 30 s.
    let renderer = ProjectRenderer::new(&daw, &opened.project_guid, RATE);
    let blocks = 30 * RATE as usize / BLOCK;
    let mut times: Vec<f64> = Vec::with_capacity(blocks);
    for b in 0..blocks {
        let t0 = Instant::now();
        let _ = renderer.render_block((b * BLOCK) as u64, BLOCK);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let first = times[0];
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let over = times.iter().filter(|t| **t > budget_ms).count();
    let slow: Vec<(usize, f64)> = times
        .iter()
        .enumerate()
        .filter(|(_, t)| **t > budget_ms)
        .map(|(i, t)| (i, *t))
        .take(10)
        .collect();
    eprintln!(
        "render: budget {budget_ms:.2} ms | first block {first:.1} ms | mean {:.3} | p99 {:.3} | max {:.1} | over budget {over}/{blocks}",
        times.iter().sum::<f64>() / times.len() as f64,
        sorted[sorted.len() * 99 / 100],
        sorted[sorted.len() - 1],
    );
    eprintln!("  slow blocks (index, ms): {slow:?}");

    // 2. The click alone: mute every other track and find its onsets.
    let tracks = Tracks::all(&daw, project.clone());
    let click = tracks
        .iter()
        .find(|t| t.name == "Click" && t.folder_depth <= 0 && !t.muted)
        .expect("generated click")
        .guid
        .clone();
    // Only other tracks with media: the buses the click routes through
    // must stay on.
    for t in &tracks {
        let has_media =
            !daw::service::Items::get_items(&daw, project.clone(), TrackRef::Guid(t.guid.clone()))
                .is_empty();
        if t.guid != click && has_media {
            let _ = Tracks::set_muted(&daw, project.clone(), TrackRef::Guid(t.guid.clone()), true);
        }
    }
    let route: Vec<String> =
        daw::service::Routing::sends(&daw, project.clone(), TrackRef::Guid(click.clone()))
            .iter()
            .map(|r| {
                format!(
                    "{:?}",
                    r.dest_track_guid
                        .as_ref()
                        .and_then(|g| tracks.iter().find(|t| &t.guid == g))
                        .map(|t| t.name.clone())
                )
            })
            .collect();
    eprintln!("click sends: {route:?}");
    let renderer = ProjectRenderer::new(&daw, &opened.project_guid, RATE);
    let mut env: Vec<f32> = Vec::new();
    for b in 0..(12 * RATE as usize / BLOCK) {
        let out = renderer.render_block((b * BLOCK) as u64, BLOCK);
        for f in 0..out.frames {
            let (l, r) = (out.samples[f * 2], out.samples[f * 2 + 1]);
            env.push(l.abs().max(r.abs()));
        }
    }
    let peak = env.iter().copied().fold(0.0f32, f32::max);
    let threshold = peak * 0.25;
    let mut onsets: Vec<f64> = Vec::new();
    let mut last = -1.0f64;
    for (i, v) in env.iter().enumerate() {
        let t = i as f64 / f64::from(RATE);
        if *v > threshold && t - last > 0.15 {
            onsets.push(t);
            last = t;
        }
    }
    let bpm = TempoMap::get_tempo_at(&daw, project, 0.0);
    let step = 60.0 / bpm / 2.0;
    eprintln!(
        "click: {bpm} bpm, eighth = {step:.4} s, peak {peak:.3}, {} onsets in 12 s",
        onsets.len()
    );
    for (i, t) in onsets.iter().take(24).enumerate() {
        let nearest = (t / step).round() * step;
        eprintln!(
            "  #{i:<2} {t:>8.4} s  grid {nearest:>8.4}  off {:>+7.1} ms",
            (t - nearest) * 1000.0
        );
    }
}
