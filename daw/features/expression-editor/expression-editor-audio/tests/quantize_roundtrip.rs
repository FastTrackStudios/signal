//! The quantize round trip on a synthetic kit, both write modes.
//!
//! Three mics, one performance, known off-grid hits. Quantize at 100 %
//! through the same engine path the panel drives — detect on the sum,
//! plan against the grid, write through the group rule — then read the
//! audio back through the accessor and detect again: every hit lands
//! within one sample of its division, on every mic, in SPLIT and in
//! WARP; a second pass finds nothing left to move.

#![cfg(feature = "daw")]

use daw::service::midi::Midi;
use daw::service::{ItemRef, ProjectContext, Takes, TrackRef, Tracks};
use daw::standalone::sync::Standalone;
use expression_editor_audio::apply_quantize::{apply_split, apply_warp};
use expression_editor_audio::detect::{DetectConfig, transients};
use expression_editor_audio::quantize::{QuantizeConfig, SplitConfig, plan};

const SR: u32 = 48_000;
const GRID: f64 = 0.25;
const LEN: f64 = 2.0;

/// The performance: hits meant for the grid, each late or early by a
/// known amount. Every mic hears the same hits at the same times —
/// that is what "mics on one source" means.
fn hit_times() -> Vec<f64> {
    vec![
        1.0 * GRID + 0.020,
        2.0 * GRID - 0.015,
        3.0 * GRID + 0.030,
        4.0 * GRID + 0.010,
        5.0 * GRID - 0.025,
        6.0 * GRID + 0.018,
    ]
}

/// A mono WAV of decaying clicks at `times`, with a per-mic tone so the
/// three files are distinct audio, as three mics would be.
fn kit_wav(times: &[f64], tone_hz: f64) -> Vec<u8> {
    let n = (SR as f64 * LEN) as usize;
    let mut samples = vec![0.0f64; n];
    for &at in times {
        let start = (at * SR as f64) as usize;
        for i in 0..((SR as usize) / 50).min(n.saturating_sub(start)) {
            let t = i as f64 / SR as f64;
            let env = (-t * 200.0).exp();
            samples[start + i] += env * (core::f64::consts::TAU * tone_hz * t).sin() * 0.8;
        }
    }
    let mut d = Vec::with_capacity(44 + n * 2);
    d.extend_from_slice(b"RIFF");
    d.extend_from_slice(&(36 + n as u32 * 2).to_le_bytes());
    d.extend_from_slice(b"WAVE");
    d.extend_from_slice(b"fmt ");
    d.extend_from_slice(&16u32.to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&SR.to_le_bytes());
    d.extend_from_slice(&(SR * 2).to_le_bytes());
    d.extend_from_slice(&2u16.to_le_bytes());
    d.extend_from_slice(&16u16.to_le_bytes());
    d.extend_from_slice(b"data");
    d.extend_from_slice(&(n as u32 * 2).to_le_bytes());
    for s in samples {
        d.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f64) as i16).to_le_bytes());
    }
    d
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ee-qroundtrip-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Three mics on the kit, one item each, all starting at zero.
fn kit(daw: &Standalone) -> (Vec<ItemRef>, TempDir) {
    let dir = TempDir::new();
    let ctx = ProjectContext::Current;
    let mut items = Vec::new();
    for (i, (name, tone)) in [("Kick In", 60.0), ("Snare Top", 220.0), ("OH", 900.0)]
        .iter()
        .enumerate()
    {
        let path = dir.0.join(format!("mic{i}.wav"));
        std::fs::write(&path, kit_wav(&hit_times(), *tone)).expect("write wav");
        let track = Tracks::add(daw, ctx.clone(), name, None).unwrap();
        let loc = Midi::create_midi_item(daw, ctx.clone(), TrackRef::Guid(track), 0.0, LEN)
            .expect("item");
        let item_guid = match &loc.item {
            ItemRef::Guid(g) => g.clone(),
            _ => panic!(),
        };
        let active =
            Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item_guid.clone())).unwrap();
        daw.write_project("p", |p| {
            for tl in p.takes.values_mut() {
                for t in tl.takes.iter_mut() {
                    if t.guid == active.guid {
                        t.source_file_path = Some(path.to_string_lossy().into_owned());
                        t.is_midi = false;
                        t.source_type = daw::service::item::SourceType::Audio;
                    }
                }
            }
        });
        items.push(ItemRef::Guid(item_guid));
    }
    // The renderer plays materialized sources; the files are on disk.
    let report = daw::standalone::audio_engine::materialize::materialize_audio(daw, "p", |p| {
        std::fs::read(p).map_err(|e| e.to_string())
    });
    assert_eq!(report.loaded, 3, "all mics materialized: {report:?}");
    (items, dir)
}

fn project() -> Standalone {
    let daw = Standalone::new();
    daw.seed_project(daw::service::ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    daw
}

/// The audio a mic's track plays now, rendered by the real engine —
/// fades, positions, offsets and stretch markers all honoured, so a
/// SPLIT's crossfades are crossfades rather than butt-join clicks that
/// would trip the detector.
///
/// Every other track is muted for the render and unmuted after.
fn playback(daw: &Standalone, first_item: &ItemRef) -> Vec<f64> {
    use daw::standalone::audio_engine::render::ProjectRenderer;
    let ctx = ProjectContext::Current;
    let info = daw.get_item(ctx.clone(), first_item.clone()).expect("item");
    let all = Tracks::all(daw, ctx.clone());
    for t in &all {
        let muted = t.guid != info.track_guid;
        Tracks::set_muted(daw, ctx.clone(), TrackRef::Guid(t.guid.clone()), muted).unwrap();
    }
    let frames = (LEN * SR as f64) as usize;
    let block = ProjectRenderer::new(daw, "p", SR).render_block(0, frames);
    for t in &all {
        Tracks::set_muted(daw, ctx.clone(), TrackRef::Guid(t.guid.clone()), false).unwrap();
    }
    (0..block.frames)
        .map(|i| block.samples[i * 2] as f64)
        .collect()
}

use daw::service::Items;

fn detected(samples: &[f64]) -> Vec<f64> {
    transients(samples, SR as f64, DetectConfig::default())
        .into_iter()
        .map(|t| t.at)
        .collect()
}

fn worst_grid_error(times: &[f64]) -> f64 {
    times
        .iter()
        .map(|t| {
            let division = (t / GRID).round() * GRID;
            (t - division).abs()
        })
        .fold(0.0, f64::max)
}

fn quantize_cfg() -> QuantizeConfig {
    QuantizeConfig {
        grid_secs: GRID,
        ..QuantizeConfig::default()
    }
}

/// What re-detection can honestly measure.
///
/// The *write* is sample-exact — the piece placements and the marker
/// map put each transient on its division to within one sample, and the
/// apply tests check that directly. Re-detecting on the rendered audio
/// adds measurement jitter of its own: the gate fires a couple of
/// samples into an attack, and a warped attack is interpolated, so the
/// trigger point moves by a few samples. 0.2 ms is far below anything
/// audible as a timing error (a flam starts nearer 10 ms).
const MEASURE_TOL: f64 = 0.0002;

// r[verify drums.verify.quantize-roundtrip]
#[test]
fn split_lands_every_hit_on_the_grid_on_every_mic() {
    let daw = project();
    let (items, _dir) = kit(&daw);
    let cfg = SplitConfig {
        leading_pad_secs: 0.005,
        crossfade_secs: 0.005,
    };

    // Detect on the trigger mic, plan at 100 %, write to the group.
    let before = playback(&daw, &items[0]);
    let hits = transients(&before, SR as f64, DetectConfig::default());
    assert_eq!(hits.len(), hit_times().len(), "detector found the hits");
    let p = plan(&hits, quantize_cfg());
    assert!(!p.moves.is_empty());
    let pieces = p.splits(LEN, cfg);
    apply_split(&daw, ProjectContext::Current, &items, &pieces, cfg).expect("split");

    for item in &items {
        let after = detected(&playback(&daw, item));
        assert_eq!(
            after.len(),
            hit_times().len(),
            "no hit lost; detected {after:?}, pieces {:?}",
            {
                let ctx = ProjectContext::Current;
                let info = daw.get_item(ctx.clone(), item.clone()).unwrap();
                let mut v: Vec<(f64, f64)> = daw
                    .get_items(ctx, TrackRef::Guid(info.track_guid))
                    .iter()
                    .map(|i| (i.position.as_seconds(), i.length.as_seconds()))
                    .collect();
                v.sort_by(|a, b| a.0.total_cmp(&b.0));
                v
            }
        );
        let worst = worst_grid_error(&after);
        assert!(worst <= MEASURE_TOL, "worst error {worst}s > {MEASURE_TOL}");
    }

    // A second pass finds nothing left to move beyond measurement
    // jitter.
    let again = transients(
        &playback(&daw, &items[0]),
        SR as f64,
        DetectConfig::default(),
    );
    let p2 = plan(&again, quantize_cfg());
    let biggest = p2.moves.iter().map(|m| m.shift().abs()).fold(0.0, f64::max);
    assert!(biggest <= MEASURE_TOL, "second pass still wants {biggest}s");
}

// r[verify drums.verify.quantize-roundtrip]
#[test]
fn warp_lands_every_hit_on_the_grid_on_every_mic() {
    let daw = project();
    let (items, _dir) = kit(&daw);

    let before = playback(&daw, &items[0]);
    let hits = transients(&before, SR as f64, DetectConfig::default());
    let p = plan(&hits, quantize_cfg());
    let frames = (LEN * SR as f64) as usize;
    let alignment = p.alignment(frames, SR as f64).expect("alignment");
    apply_warp(&daw, ProjectContext::Current, &items, &alignment).expect("warp");

    // The accessor reads through markers, so playback() hears the warp.
    for item in &items {
        let after = detected(&playback(&daw, item));
        assert_eq!(after.len(), hit_times().len(), "no hit lost");
        let worst = worst_grid_error(&after);
        assert!(worst <= MEASURE_TOL, "worst error {worst}s > {MEASURE_TOL}");
    }
}
