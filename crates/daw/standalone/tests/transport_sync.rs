//! The standalone engine as a `daw-transport-sync` backend: true
//! varispeed, the per-buffer snapshot, and scheduled locates — driven
//! through the same `begin_block` → `render_plan` path the audio
//! callbacks use, with no device.

#![cfg(feature = "decode")]
#![allow(clippy::float_cmp, clippy::cast_precision_loss)]

use std::sync::Arc;

use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, ProjectInfo, TrackRef, Tracks};
use daw_standalone::audio_engine::DecodedAudio;
use daw_standalone::audio_engine::materialize::attach_audio_source;
use daw_standalone::audio_engine::render::{ProjectRenderer, StereoBuffer};
use daw_standalone::sync::Standalone;
use daw_standalone::transport_engine::{PlayStateRepr, ScheduledLocate, TransportShared};
use daw_standalone::transport_sync::SyncBackend;
use daw_transport_sync::{BufferClock, Correction, Follower, TransportBackend};

const SR: u32 = 48_000;
/// Source length: 10 s.
const SOURCE_FRAMES: usize = 10 * SR as usize;

/// The ramp source's value at (fractional) source frame `x`.
fn ramp(x: f64) -> f64 {
    x / SOURCE_FRAMES as f64
}

/// A project with one track holding one 10 s item at 0 s whose source is
/// a ramp (frame j = j / SOURCE_FRAMES) — linear, so the renderer's
/// linear interpolation reproduces it exactly at any fractional read
/// position.
fn ramp_project() -> (Standalone, ProjectRenderer) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let track = Tracks::add(&daw, ProjectContext::Current, "T", None).unwrap();
    let ctx = ProjectContext::Project(guid.clone());
    let loc = Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track), 0.0, 10.0).unwrap();
    let ItemRef::Guid(item) = loc.item else {
        panic!("item guid")
    };
    let active = daw_proto::Takes::get_active_take(&daw, ctx, ItemRef::Guid(item)).unwrap();
    daw.write_project(&guid, |p| {
        for tl in p.takes.values_mut() {
            for t in &mut tl.takes {
                if t.guid == active.guid {
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                    t.source_file_path = None;
                }
            }
        }
    });
    let samples: Vec<f32> = (0..SOURCE_FRAMES).map(|j| ramp(j as f64) as f32).collect();
    attach_audio_source(&daw, &guid, &active.guid, DecodedAudio::new(samples, 1, SR));
    let renderer = ProjectRenderer::new(&daw, &guid, SR);
    (daw, renderer)
}

fn left(buf: &StereoBuffer) -> Vec<f64> {
    (0..buf.frames)
        .map(|i| f64::from(buf.samples[i * 2]))
        .collect()
}

/// The pan law's gain on the left channel: what a 1x render of the ramp
/// at 1 s reads back over the source.
fn pan_gain(r: &ProjectRenderer) -> f64 {
    let at = f64::from(SR);
    left(&r.render_block(at as u64, 1))[0] / ramp(at)
}

fn playing_transport(at_samples: f64, rate: f64) -> Arc<TransportShared> {
    let shared = Arc::new(TransportShared::new(SR, 120.0));
    shared.set_playhead_samples_f64(at_samples);
    shared.set_playrate(rate);
    shared.set_play_state(PlayStateRepr::Playing);
    shared
}

/// µs one buffer of `frames` lasts at the nominal rate.
fn period(frames: u32) -> f64 {
    f64::from(frames) / f64::from(SR) * 1e6
}

#[test]
fn varispeed_renders_the_source_resampled() {
    let (_daw, r) = ramp_project();
    let g = pan_gain(&r);
    let (start, n, rate) = (f64::from(SR), 4_800_u32, 1.01);
    let shared = playing_transport(start, rate);

    let plan = shared.begin_block(n, 0.0, period(n));
    let out = left(&r.render_plan(&plan, false));

    // The playhead moved on by exactly what was rendered.
    let expected_end = start + f64::from(n) * rate;
    assert!((shared.playhead_samples_f64() - expected_end).abs() < 1e-6);
    // Output frame i is the source at start + i * rate.
    for (i, v) in out.iter().enumerate() {
        let want = g * ramp((i as f64).mul_add(rate, start));
        assert!((v - want).abs() < 1e-6, "frame {i}: {v} vs {want}");
    }
    // …and not the 1x read the old engine made (the ramp has moved
    // 48 source frames further by the block's end).
    let last = out.len() - 1;
    assert!((out[last] - g * ramp(start + last as f64)).abs() > 1e-5);
}

#[test]
fn block_boundaries_at_a_non_integer_rate_are_continuous() {
    let (_daw, r) = ramp_project();
    for rate in [1.000_08, 1.01, 0.993] {
        let start = 1.5 * f64::from(SR);
        let shared = playing_transport(start, rate);
        let mut blocks = Vec::new();
        for b in 0..10 {
            let plan = shared.begin_block(512, f64::from(b) * period(512), period(512));
            blocks.extend(left(&r.render_plan(&plan, false)));
        }
        // One long render from the same start: blocks must add up to it,
        // no sample dropped or repeated at any boundary.
        let whole = left(&r.render_block_varispeed(start, 5_120, rate));
        let worst = blocks
            .iter()
            .zip(&whole)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(worst < 1e-6, "rate {rate}: worst {worst}");
        assert!((shared.playhead_samples_f64() - 5_120.0f64.mul_add(rate, start)).abs() < 1e-6);
    }
}

#[test]
fn rate_one_is_the_plain_render() {
    let (_daw, r) = ramp_project();
    let shared = playing_transport(12_345.0, 1.0);
    let plan = shared.begin_block(256, 0.0, period(256));
    let via_plan = r.render_plan(&plan, false);
    let plain = r.render_block(12_345, 256);
    assert_eq!(via_plan.samples, plain.samples, "bit-identical at 1x");
    assert_eq!(shared.playhead_samples().0, 12_345 + 256);
}

#[test]
fn a_scheduled_locate_lands_on_its_exact_sample() {
    let (_daw, r) = ramp_project();
    let g = pan_gain(&r);
    let shared = Arc::new(TransportShared::new(SR, 120.0));
    let backend = SyncBackend::new(shared.clone(), [0; 16]);
    let (n, k) = (512_u32, 100_usize);
    let stamp = 50_000.0;
    // Frame 100 of the buffer that starts at `stamp`.
    let at = stamp + k as f64 / f64::from(SR) * 1e6;
    backend.locate_at(at, 2.0, true, 1.0);

    // The buffer before does not land it.
    let early = shared.begin_block(n, stamp - period(n), period(n));
    assert_eq!(early.segments().len(), 1);
    assert!(!early.any_playing());

    let plan = shared.begin_block(n, stamp, period(n));
    let segs = plan.segments();
    assert_eq!(segs.len(), 2);
    assert_eq!(
        (segs[0].offset, segs[0].frames, segs[0].playing),
        (0, k, false)
    );
    assert_eq!(
        (segs[1].offset, segs[1].frames, segs[1].playing),
        (k, n as usize - k, true)
    );
    // The playhead is the target exactly at the locate's frame.
    assert!((segs[1].start_samples - 2.0 * f64::from(SR)).abs() < 1e-6);
    assert!(
        (shared.playhead_samples_f64() - (2.0 * f64::from(SR) + (n as usize - k) as f64)).abs()
            < 1e-6
    );
    assert_eq!(shared.play_state(), PlayStateRepr::Playing);

    // Silence before the landing frame, the target's audio from it.
    let out = left(&r.render_plan(&plan, false));
    assert!(out[..k].iter().all(|v| *v == 0.0));
    for (i, v) in out.iter().enumerate().skip(k) {
        let want = g * ramp(2.0 * f64::from(SR) + (i - k) as f64);
        assert!((v - want).abs() < 1e-6, "frame {i}: {v} vs {want}");
    }

    // Landed once: the next buffer just plays on.
    let next = shared.begin_block(n, stamp + period(n), period(n));
    assert_eq!(next.segments().len(), 1);
}

#[test]
fn a_locate_while_playing_keeps_the_old_audio_up_to_its_frame() {
    let (_daw, r) = ramp_project();
    let (n, k) = (512_u32, 200_usize);
    let old_start = f64::from(SR);
    let shared = playing_transport(old_start, 1.0);
    let stamp = 0.0;
    shared.schedule_locate(ScheduledLocate {
        at_micros: stamp + k as f64 / f64::from(SR) * 1e6,
        position_seconds: 5.0,
        playing: true,
        rate: 1.01,
    });
    let plan = shared.begin_block(n, stamp, period(n));
    let out = left(&r.render_plan(&plan, false));
    let before = left(&r.render_block_varispeed(old_start, k, 1.0));
    let after = left(&r.render_block_varispeed(5.0 * f64::from(SR), n as usize - k, 1.01));
    assert_eq!(&out[..k], &before[..]);
    assert_eq!(&out[k..], &after[..]);
    assert_eq!(shared.playrate(), 1.01);
}

#[test]
fn a_late_locate_lands_where_the_playhead_would_be_by_now() {
    let shared = Arc::new(TransportShared::new(SR, 120.0));
    let backend = SyncBackend::new(shared.clone(), [0; 16]);
    let stamp = 1_000_000.0;
    // Due 50 frames before this buffer began.
    backend.locate_at(stamp - 50.0 / f64::from(SR) * 1e6, 3.0, true, 1.0);
    let plan = shared.begin_block(256, stamp, period(256));
    assert_eq!(plan.segments().len(), 1);
    let first = plan.first();
    assert!(first.playing);
    assert!((first.start_samples - (3.0 * f64::from(SR) + 50.0)).abs() < 1e-6);
}

#[test]
fn stop_lands_at_the_next_buffer_and_rests_at_its_position() {
    let shared = playing_transport(10_000.0, 1.0);
    let backend = SyncBackend::new(shared.clone(), [0; 16]);
    backend.stop(4.0);
    let plan = shared.begin_block(256, 0.0, period(256));
    assert!(!plan.any_playing());
    assert_eq!(shared.play_state(), PlayStateRepr::Stopped);
    assert_eq!(shared.playhead_samples_f64(), 4.0 * f64::from(SR));
    let snap = backend.snapshot().unwrap();
    assert!(!snap.is_playing);
    assert_eq!(snap.playhead_seconds, 4.0);
}

/// A deterministic jitter source.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[test]
fn snapshots_are_smooth_and_consistent() {
    let shared = playing_transport(0.0, 1.000_08);
    let backend = SyncBackend::new(shared.clone(), [7; 16]);
    assert!(
        backend.snapshot().is_none(),
        "nothing before the first buffer"
    );
    let mut stamps = BufferClock::default();
    let mut jitter = Lcg(11);
    let n = 256_u32;
    let mut snaps = Vec::new();
    let mut raw_late = Vec::new();
    for b in 0..4_000 {
        // Buffers start exactly on the device clock; callbacks run up to
        // 0.8 ms late.
        let truth = f64::from(b) * period(n);
        let entry = 0.0008e6f64.mul_add(jitter.next(), truth);
        raw_late.push(entry - truth);
        let stamp = stamps.tick(entry, n, f64::from(SR));
        shared.begin_block(n, stamp, stamps.period_micros());
        snaps.push((truth, backend.snapshot().unwrap()));
    }
    // Settled (after the loop's ~1 s time constant): each stamp a
    // buffer's period after the last to within a fraction of the 800 µs
    // callback jitter, and a steady distance from the true start (the
    // mean callback lateness — the same for every buffer, so harmless)
    // that wanders far less than the raw callback times do.
    let settled = &snaps[1_000..];
    let rms = |v: &mut dyn Iterator<Item = f64>| {
        let v: Vec<f64> = v.collect();
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    };
    let raw = rms(&mut raw_late[1_000..].iter().copied());
    let filtered = rms(&mut settled.iter().map(|(t, s)| s.host_micros - t));
    assert!(
        filtered < raw / 3.0,
        "stamps wander {filtered:.1} µs rms; callbacks {raw:.1}"
    );
    for w in settled.windows(2) {
        let ((_, a), (_, b)) = (w[0], w[1]);
        assert_eq!(b.sequence, a.sequence + 1);
        assert_eq!(b.project_id, [7; 16]);
        let step = b.host_micros - a.host_micros;
        assert!((step - period(n)).abs() < 40.0, "step {step}");
        // The playhead moved on by exactly the buffer at its rate…
        let moved = (b.playhead_seconds - a.playhead_seconds) * f64::from(SR);
        assert!(
            (moved - f64::from(n) * 1.000_08).abs() < 1e-6,
            "moved {moved}"
        );
        // …and projecting one snapshot to the next one's time lands on
        // it to within the stamps' step error (< 40 µs ≈ 2 samples).
        let projected = a.position().at(b.host_micros);
        let gap = (projected - b.playhead_seconds).abs() * f64::from(SR);
        assert!(gap < 2.0, "projection off by {gap} samples");
        assert!(b.is_playing);
        assert_eq!(b.buffer_len, n);
        assert_eq!(b.playrate, 1.000_08);
    }
}

/// Two engines on one clock: the follower, stopped, is brought in by a
/// `Follower` and plays sample-aligned with the leader.
#[test]
fn a_follower_is_located_onto_a_playing_leader() {
    let leader = playing_transport(10.0 * f64::from(SR), 1.0);
    let follower_shared = Arc::new(TransportShared::new(SR, 120.0));
    let follower = SyncBackend::new(follower_shared.clone(), [0; 16]);
    let mut follow = Follower::default();
    let n = 256_u32;
    let mut locates = 0;
    for b in 0..400 {
        let t = f64::from(b) * period(n);
        leader.begin_block(n, t, period(n));
        // The follower's device starts its buffers 3.1 ms off the leader's.
        follower_shared.begin_block(n, t + 3_100.0, period(n));
        if b % 8 == 0 {
            let lead = leader.sync_snapshot().unwrap().position();
            if let Correction::Locate { .. } = follow.tick(&follower, &lead, 0.0, t + 3_200.0) {
                locates += 1;
            }
        }
    }
    assert_eq!(locates, 1, "one locate brings it in; nothing else needed");
    let lead = leader.sync_snapshot().unwrap().position();
    let mine = follower.snapshot().unwrap();
    assert!(mine.is_playing);
    let gap = (mine.playhead_seconds - lead.at(mine.host_micros)).abs() * f64::from(SR);
    assert!(gap < 0.01, "{gap} samples apart");
}

#[test]
fn a_position_is_stamped_in_the_sync_clock() {
    // The snapshot's time is `transport_sync::now_micros`'s domain.
    let before = daw_standalone::transport_sync::now_micros();
    let shared = playing_transport(0.0, 1.0);
    shared.begin_block(64, daw_standalone::transport_sync::now_micros(), period(64));
    let snap = shared.sync_snapshot().unwrap();
    assert!(snap.host_micros >= before);
    assert!(snap.host_micros <= daw_standalone::transport_sync::now_micros());
}

#[tokio::test]
async fn standalone_hands_out_a_backend_per_project() {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "{67E55044-10B1-426F-9247-BB680E5FE0C8}".into(),
        name: "p".into(),
        path: String::new(),
    });
    assert!(daw.sync_backend("no-such-project").is_none());
    let backend = daw.sync_backend(&guid).unwrap();
    backend.set_rate(1.002);
    assert_eq!(daw.transport_engine_for(&guid).shared.playrate(), 1.002);
    assert_eq!(backend.project_id()[0], 0x67);
    // The soft clock drives it with no device: snapshots arrive.
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    assert!(backend.snapshot().is_some());
}
