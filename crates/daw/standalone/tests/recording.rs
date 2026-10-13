//! Recording on the standalone engine: an armed track's input captured
//! while the transport records, and each pass put on the track as a new
//! take on a fixed lane when it stops (REAPER 7's model).
//!
//! No hardware: the input channel plays a generated buffer in its place
//! (`Standalone::set_test_input`), and the record span is rendered offline
//! through a `ProjectRenderer` — the real transport, the real capture, the
//! real writer and the real `finish_recording`.

#![cfg(all(feature = "decode", not(target_arch = "wasm32")))]

use daw_proto::track::RecordInput;
use daw_proto::transport::service::Transport;
use daw_proto::{ItemRef, Items, ProjectContext, ProjectInfo, Takes, TrackRef, Tracks};
use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::recording::{RecordedTake, TakeLanePolicy};
use daw_standalone::sync::Standalone;

const RATE: u32 = 48_000;
const BLOCK: usize = 512;

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "daw-recording-{}-{}",
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

/// A sine at `freq` Hz, −18 dBFS, `seconds` long.
fn sine(freq: f64, seconds: f64) -> Vec<f32> {
    let amp = 10f64.powf(-18.0 / 20.0);
    (0..(seconds * f64::from(RATE)) as usize)
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            ((t * freq * std::f64::consts::TAU).sin() * amp) as f32
        })
        .collect()
}

/// A project saved at `<dir>/song.RPP` with one track, `Guitar`, armed
/// to input channel 0.
fn armed_project(dir: &std::path::Path) -> (Standalone, String, String) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "rec".into(),
        name: "Recording".into(),
        path: dir.join("song.RPP").to_string_lossy().into_owned(),
    });
    let ctx = ProjectContext::Project(guid.clone());
    let track = Tracks::add(&daw, ctx.clone(), "Guitar", None).expect("a track");
    let at = TrackRef::Guid(track.clone());
    Tracks::set_record_input(
        &daw,
        ctx.clone(),
        at.clone(),
        RecordInput::Audio { channel: 0 },
    )
    .expect("its input");
    Tracks::set_armed(&daw, ctx, at, true).expect("armed");
    (daw, guid, track)
}

/// Record `seconds` from `from` (seconds): record, render the span block by
/// block, stop. What the pass placed.
fn record(daw: &Standalone, guid: &str, from: f64, seconds: f64) -> Vec<RecordedTake> {
    let ctx = ProjectContext::Project(guid.to_owned());
    Transport::set_position(daw, ctx.clone(), from).expect("located");
    Transport::record(daw, ctx.clone()).expect("recording");
    assert!(daw.is_capturing(guid), "a pass is open");
    let renderer = ProjectRenderer::new(daw, guid, RATE);
    let start = (from * f64::from(RATE)).round() as u64;
    let total = (seconds * f64::from(RATE)).round() as usize;
    let mut done = 0;
    while done < total {
        let n = BLOCK.min(total - done);
        renderer.render_block(start + done as u64, n);
        done += n;
    }
    Transport::stop_recording(daw, ctx).expect("stopped");
    assert!(!daw.is_capturing(guid), "the pass is closed");
    daw.last_recording(guid)
}

/// A mono float WAV's rate and samples, as the recorder writes them.
fn read_wav(path: &std::path::Path) -> (u32, Vec<f32>) {
    let bytes = std::fs::read(path).expect("the take's file");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 3, "float");
    assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1, "mono");
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let data = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
    let samples = bytes[44..44 + data]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    (rate, samples)
}

fn track(daw: &Standalone, guid: &str, track: &str) -> daw_proto::Track {
    Tracks::get(
        daw,
        ProjectContext::Project(guid.to_owned()),
        TrackRef::Guid(track.to_owned()),
    )
    .expect("the track")
}

/// Arm, play a 1 kHz tone into the input, record 2 s, stop: one new item at
/// the record start, on lane `1` (lanes switched on), playing a WAV in the
/// project's `Media/` whose samples are the tone's. Record again over the
/// same range: a second take, on a new lane `2`, which now plays.
#[test]
fn two_passes_over_one_range_land_on_lanes_one_and_two() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let dir = TempDir::new();
    let (daw, guid, guitar) = armed_project(&dir.0);
    let ctx = ProjectContext::Project(guid.clone());
    assert_eq!(daw.record_dir(&guid), dir.0.join("Media"));
    let tone = sine(1_000.0, 3.0);
    daw.set_test_input(0, tone.clone());

    let first = record(&daw, &guid, 1.0, 2.0);
    let [take] = first.as_slice() else {
        panic!("one take: {first:?}");
    };
    assert_eq!(take.track_guid, guitar);
    assert!((take.position - 1.0).abs() < 1e-9, "at the record start");
    assert!((take.length - 2.0).abs() < 1e-9, "as long as the pass");
    assert_eq!((take.lane, take.lane_name.as_str()), (0, "1"));
    assert_eq!(take.dropped_frames, 0);
    assert_eq!(take.path, dir.0.join("Media").join("Guitar-01.wav"));
    let (rate, samples) = read_wav(&take.path);
    assert_eq!(rate, RATE);
    assert_eq!(samples.len(), 2 * RATE as usize);
    assert_eq!(
        samples.as_slice(),
        &tone[..2 * RATE as usize],
        "the tone, sample for sample"
    );

    let t = track(&daw, &guid, &guitar);
    assert_eq!(t.lane_count, 1, "lanes switched on");
    assert_eq!(t.lane_play_mask, 1, "the take plays");
    let items = Items::get_items(&daw, ctx.clone(), TrackRef::Guid(guitar.clone()));
    let [item] = items.as_slice() else {
        panic!("one item: {items:?}");
    };
    assert_eq!(item.guid, take.item_guid);
    assert_eq!(item.fixed_lane, Some(0));
    let active = Takes::get_active_take(&daw, ctx.clone(), ItemRef::Guid(item.guid.clone()))
        .expect("its take");
    assert_eq!(
        active.source_file_path.as_deref(),
        Some(take.path.to_string_lossy().as_ref())
    );

    // It plays: the tone comes back out of the master.
    let back =
        ProjectRenderer::new(&daw, &guid, RATE).render_block((1.5 * f64::from(RATE)) as u64, BLOCK);
    let peak = back.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.1, "the take is heard: peak {peak}");

    let second = record(&daw, &guid, 1.0, 2.0);
    let [again] = second.as_slice() else {
        panic!("one take: {second:?}");
    };
    assert_eq!((again.lane, again.lane_name.as_str()), (1, "2"));
    assert_eq!(again.path, dir.0.join("Media").join("Guitar-02.wav"));
    assert!((again.position - 1.0).abs() < 1e-9);
    let t = track(&daw, &guid, &guitar);
    assert_eq!(t.lane_count, 2);
    assert_eq!(t.lane_play_mask, 1 << 1, "the newest take plays");
    assert_eq!(
        Items::get_items(&daw, ctx, TrackRef::Guid(guitar))
            .iter()
            .filter_map(|i| i.fixed_lane)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

/// Lowest free (the default): a pass over a range lane `1` is empty for
/// goes on lane `1`, beside the earlier take — no new lane. Always new:
/// it gets a lane of its own.
#[test]
fn a_pass_goes_on_the_lowest_lane_free_over_its_range_unless_asked_otherwise() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let dir = TempDir::new();
    let (daw, guid, guitar) = armed_project(&dir.0);
    daw.set_test_input(0, sine(220.0, 2.0));
    record(&daw, &guid, 0.0, 1.0);
    record(&daw, &guid, 0.0, 1.0);

    let later = record(&daw, &guid, 4.0, 1.0);
    assert_eq!(later.first().map(|t| t.lane), Some(0), "beside take 1");
    assert_eq!(track(&daw, &guid, &guitar).lane_count, 2);

    daw.set_take_lane_policy(TakeLanePolicy::AlwaysNew);
    let own = record(&daw, &guid, 8.0, 1.0);
    assert_eq!(
        own.first().map(|t| (t.lane, t.lane_name.clone())),
        Some((2, "3".to_owned()))
    );
}

/// Only the renderer feeding the pass records: a bounce rendered from
/// elsewhere on the timeline meanwhile is not the input.
#[test]
fn a_second_renderer_does_not_feed_the_pass() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let dir = TempDir::new();
    let (daw, guid, _) = armed_project(&dir.0);
    let tone = sine(1_000.0, 1.0);
    daw.set_test_input(0, tone.clone());
    let ctx = ProjectContext::Project(guid.clone());
    Transport::record(&daw, ctx.clone()).expect("recording");
    let feeding = ProjectRenderer::new(&daw, &guid, RATE);
    let bounce = ProjectRenderer::new(&daw, &guid, RATE);
    let mut at = 0_u64;
    while at < u64::from(RATE) {
        feeding.render_block(at, BLOCK);
        bounce.render_block(at + 10 * u64::from(RATE), BLOCK);
        at += BLOCK as u64;
    }
    Transport::stop(&daw, ctx).expect("stopped");
    let takes = daw.last_recording(&guid);
    let [take] = takes.as_slice() else {
        panic!("one take: {takes:?}");
    };
    let (_, samples) = read_wav(&take.path);
    assert_eq!(samples.len(), at as usize);
    assert_eq!(&samples[..tone.len()], tone.as_slice());
}

/// Nothing armed: the transport records, and stopping places nothing.
#[test]
fn nothing_armed_records_nothing() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _inside = runtime.enter();
    let dir = TempDir::new();
    let (daw, guid, guitar) = armed_project(&dir.0);
    let ctx = ProjectContext::Project(guid.clone());
    Tracks::set_armed(&daw, ctx.clone(), TrackRef::Guid(guitar.clone()), false).expect("disarmed");
    Transport::record(&daw, ctx.clone()).expect("recording");
    assert!(!daw.is_capturing(&guid));
    ProjectRenderer::new(&daw, &guid, RATE).render_block(0, BLOCK);
    Transport::stop_recording(&daw, ctx.clone()).expect("stopped");
    assert!(daw.last_recording(&guid).is_empty());
    assert!(Items::get_items(&daw, ctx, TrackRef::Guid(guitar)).is_empty());
}
