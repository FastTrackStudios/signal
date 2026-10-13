//! `AudioAccessors` against the standalone backend.
//!
//! The point of these is the *facade*: an accessor is how anything that
//! wants a take's samples asks for them, and until this impl existed
//! only REAPER could answer. A caller written against the trait — the
//! expression editor's audio session, for one — now works headlessly.

#![cfg(feature = "audio")]

use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{
    AudioAccessors, GetSamplesRequest, ItemRef, ProjectInfo, TakeRef, Takes, TrackRef, Tracks,
};
use daw_standalone::sync::Standalone;

const SR: u32 = 44_100;

/// A mono 16-bit WAV of a 440 Hz tone.
fn tone_wav(secs: f64) -> Vec<u8> {
    let n = (SR as f64 * secs) as usize;
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
    for i in 0..n {
        let t = i as f64 / SR as f64;
        let s = (t * 440.0 * std::f64::consts::TAU).sin();
        d.extend_from_slice(&((s * 0.6 * i16::MAX as f64) as i16).to_le_bytes());
    }
    d
}

/// A temp directory that cleans up after itself.
///
/// Hand-rolled rather than a `tempfile` dependency: one test file does
/// not justify a new workspace dep, and the accessor needs a real path
/// on disk because decoding a file is the whole point.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "daw-standalone-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A project with one audio item whose take points at a real file.
fn project_with_audio(secs: f64) -> (Standalone, ItemRef, TempDir) {
    let dir = TempDir::new("accessor");
    let path = dir.0.join("tone.wav");
    std::fs::write(&path, tone_wav(secs)).expect("write wav");

    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Vox", None).unwrap();
    let loc =
        Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track), 0.0, secs).expect("item");
    let item_guid = match &loc.item {
        ItemRef::Guid(g) => g.clone(),
        _ => panic!("expected a guid"),
    };
    let active = Takes::get_active_take(&daw, ctx, ItemRef::Guid(item_guid.clone())).expect("take");

    // Point the take at the file on disk; the accessor decodes it.
    daw.write_project(&guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.source_file_path = Some(path.to_string_lossy().into_owned());
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                }
            }
        }
    });

    (daw, ItemRef::Guid(item_guid), dir)
}

#[test]
fn a_takes_source_file_can_be_read_through_the_facade() {
    let (daw, item, _dir) = project_with_audio(0.5);
    let acc = daw
        .create_take_accessor(ProjectContext::Current, item, TakeRef::Active)
        .expect("accessor");

    let got = daw.get_samples(GetSamplesRequest {
        accessor_id: acc.clone(),
        sample_rate: SR as f64,
        num_channels: 1,
        start_time: 0.0,
        num_samples: 1000,
    });
    assert_eq!(got.num_samples, 1000);
    assert_eq!(got.samples.len(), 1000);
    assert!(
        got.samples.iter().any(|s| s.abs() > 0.1),
        "the tone actually decoded"
    );
    daw.destroy_accessor(&acc);
}

#[test]
fn a_zero_in_the_request_means_tell_me_what_you_have() {
    // How a caller discovers the source format before reading in
    // earnest — and the reason it must not have to name a rate, since
    // naming one would force a resample.
    let (daw, item, _dir) = project_with_audio(0.3);
    let acc = daw
        .create_take_accessor(ProjectContext::Current, item, TakeRef::Active)
        .expect("accessor");

    let probe = daw.get_samples(GetSamplesRequest {
        accessor_id: acc.clone(),
        sample_rate: 0.0,
        num_channels: 0,
        start_time: 0.0,
        num_samples: 1,
    });
    assert_eq!(probe.sample_rate, SR as f64);
    assert_eq!(probe.num_channels, 1);
    daw.destroy_accessor(&acc);
}

#[test]
fn reading_in_chunks_returns_the_same_audio_as_one_read() {
    let (daw, item, _dir) = project_with_audio(0.4);
    let acc = daw
        .create_take_accessor(ProjectContext::Current, item, TakeRef::Active)
        .expect("accessor");

    let whole = daw.get_samples(GetSamplesRequest {
        accessor_id: acc.clone(),
        sample_rate: SR as f64,
        num_channels: 1,
        start_time: 0.0,
        num_samples: 8000,
    });
    let mut pieces = Vec::new();
    for k in 0..8 {
        let chunk = daw.get_samples(GetSamplesRequest {
            accessor_id: acc.clone(),
            sample_rate: SR as f64,
            num_channels: 1,
            start_time: k as f64 * 1000.0 / SR as f64,
            num_samples: 1000,
        });
        pieces.extend(chunk.samples);
    }
    assert_eq!(pieces.len(), whole.samples.len());
    let worst = pieces
        .iter()
        .zip(&whole.samples)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(worst < 1e-12, "chunked read differs by {worst}");
    daw.destroy_accessor(&acc);
}

#[test]
fn reading_past_the_end_is_silence_rather_than_an_error() {
    // An item longer than its source is ordinary, and a caller reading
    // the item's full length must not be handed a failure for it.
    let (daw, item, _dir) = project_with_audio(0.1);
    let acc = daw
        .create_take_accessor(ProjectContext::Current, item, TakeRef::Active)
        .expect("accessor");

    let got = daw.get_samples(GetSamplesRequest {
        accessor_id: acc.clone(),
        sample_rate: SR as f64,
        num_channels: 1,
        start_time: 5.0,
        num_samples: 500,
    });
    assert_eq!(got.num_samples, 500);
    assert!(got.samples.iter().all(|s| *s == 0.0));
    daw.destroy_accessor(&acc);
}

#[test]
fn a_mono_source_read_as_stereo_is_centred_not_half_silent() {
    let (daw, item, _dir) = project_with_audio(0.2);
    let acc = daw
        .create_take_accessor(ProjectContext::Current, item, TakeRef::Active)
        .expect("accessor");

    let got = daw.get_samples(GetSamplesRequest {
        accessor_id: acc.clone(),
        sample_rate: SR as f64,
        num_channels: 2,
        start_time: 0.0,
        num_samples: 400,
    });
    assert_eq!(got.samples.len(), 800);
    // Both channels carry the same signal.
    for f in got.samples.chunks(2) {
        assert_eq!(f[0], f[1]);
    }
    assert!(got.samples.iter().any(|s| s.abs() > 0.1));
    daw.destroy_accessor(&acc);
}

#[test]
fn a_take_with_no_source_declines_rather_than_serving_silence() {
    // A MIDI take has no file. Declining tells the caller so; silence
    // would have it conclude the take is empty audio.
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Keys", None).unwrap();
    let loc =
        Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track), 0.0, 1.0).expect("item");

    assert!(
        daw.create_take_accessor(ctx, loc.item, TakeRef::Active)
            .is_none()
    );
}

#[test]
fn a_destroyed_accessor_stops_serving() {
    let (daw, item, _dir) = project_with_audio(0.2);
    let acc = daw
        .create_take_accessor(ProjectContext::Current, item, TakeRef::Active)
        .expect("accessor");
    daw.destroy_accessor(&acc);

    let got = daw.get_samples(GetSamplesRequest {
        accessor_id: acc,
        sample_rate: SR as f64,
        num_channels: 1,
        start_time: 0.0,
        num_samples: 100,
    });
    assert!(got.samples.is_empty(), "a freed handle serves nothing");
}

/// A track reads as its items play into it: in project time, each item at
/// its place through its volume — silence where none plays, nothing of a
/// muted one.
#[test]
fn a_track_reads_its_items_where_they_play_through_their_gain() {
    use daw_proto::Items as _;
    let (daw, item, _dir) = project_with_audio(0.5);
    daw.set_position(
        ProjectContext::Current,
        item.clone(),
        daw_proto::primitives::PositionInSeconds::from_seconds(1.0),
    )
    .expect("moved");
    daw_proto::Items::set_volume(&daw, ProjectContext::Current, item.clone(), 0.5)
        .expect("its gain");
    let take = daw
        .create_take_accessor(ProjectContext::Current, item.clone(), TakeRef::Active)
        .expect("the take");
    let track = daw
        .create_track_accessor(ProjectContext::Current, TrackRef::Index(0))
        .expect("the track");

    assert!(
        read(&daw, &track, 0.5, 200).iter().all(|s| *s == 0.0),
        "silence before the item"
    );
    let played = read(&daw, &track, 1.1, 200);
    let source = read(&daw, &take, 0.1, 200);
    assert!(source.iter().any(|s| s.abs() > 0.1), "the tone");
    for (p, s) in played.iter().zip(&source) {
        assert!((p - s * 0.5).abs() < 1e-6, "the take at half: {p} vs {s}");
    }

    daw_proto::Items::set_muted(&daw, ProjectContext::Current, item, true).expect("muted");
    assert!(
        daw.create_track_accessor(ProjectContext::Current, TrackRef::Index(0))
            .is_none(),
        "a track whose only item is muted has nothing to read: None, not silence"
    );
}

/// Read `n` mono samples at the source rate from `start`.
fn read(daw: &Standalone, acc: &str, start: f64, n: u32) -> Vec<f64> {
    daw.get_samples(GetSamplesRequest {
        accessor_id: acc.to_string(),
        sample_rate: SR as f64,
        num_channels: 1,
        start_time: start,
        num_samples: n,
    })
    .samples
}

fn set_take(daw: &Standalone, f: impl Fn(&mut daw_proto::item::Take)) {
    daw.write_project("p", |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                f(t);
            }
        }
    });
}

// r[verify drums.open.accessor-placement]
#[test]
fn the_accessor_reads_in_take_playback_time() {
    // A start offset of 0.1 s means take time 0 is source time 0.1 —
    // the same samples REAPER's accessor would hand back.
    let (daw, item, _dir) = project_with_audio(0.5);
    let plain = daw
        .create_take_accessor(ProjectContext::Current, item.clone(), TakeRef::Active)
        .expect("accessor");
    let from_source = read(&daw, &plain, 0.1, 500);
    daw.destroy_accessor(&plain);

    set_take(&daw, |t| {
        t.start_offset = daw_proto::Duration::from_seconds(0.1)
    });
    let shifted = daw
        .create_take_accessor(ProjectContext::Current, item.clone(), TakeRef::Active)
        .expect("accessor");
    let from_take = read(&daw, &shifted, 0.0, 500);
    daw.destroy_accessor(&shifted);
    let worst = from_source
        .iter()
        .zip(&from_take)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(worst < 1e-9, "start offset ignored (diff {worst})");

    // Play rate 2: take time 0.1 is source time 0.1 + 0.2 = 0.3.
    set_take(&daw, |t| t.play_rate = 2.0);
    let fast = daw
        .create_take_accessor(ProjectContext::Current, item.clone(), TakeRef::Active)
        .expect("accessor");
    let a = read(&daw, &fast, 0.1, 1)[0];
    daw.destroy_accessor(&fast);
    set_take(&daw, |t| {
        t.play_rate = 1.0;
        t.start_offset = daw_proto::Duration::from_seconds(0.0);
    });
    let plain = daw
        .create_take_accessor(ProjectContext::Current, item.clone(), TakeRef::Active)
        .expect("accessor");
    let b = read(&daw, &plain, 0.3, 1)[0];
    daw.destroy_accessor(&plain);
    assert!((a - b).abs() < 1e-9, "play rate ignored ({a} vs {b})");

    // A stretch marker map overrides offset+rate: take 0.2 s plays
    // source 0.4 s.
    use daw_proto::{StretchMarker, StretchMarkers, StretchTakeRef};
    StretchMarkers::set_stretch_markers(
        &daw,
        StretchTakeRef {
            project: ProjectContext::Current,
            item: item.clone(),
            take: TakeRef::Active,
        },
        vec![StretchMarker::new(0.0, 0.0), StretchMarker::new(0.2, 0.4)],
    )
    .expect("markers");
    let warped = daw
        .create_take_accessor(ProjectContext::Current, item.clone(), TakeRef::Active)
        .expect("accessor");
    let w = read(&daw, &warped, 0.2, 1)[0];
    daw.destroy_accessor(&warped);
    let plain = daw
        .create_take_accessor(ProjectContext::Current, item, TakeRef::Active)
        .expect("accessor");
    let p = read(&daw, &plain, 0.4, 1)[0];
    assert!((w - p).abs() < 1e-9, "markers ignored ({w} vs {p})");
}

// r[verify drums.open.peaks]
#[test]
fn take_peaks_cover_the_item_in_take_time() {
    use daw_proto::Peaks;
    let (daw, item, _dir) = project_with_audio(0.5);
    let peaks = daw.take_peaks(ProjectContext::Current, item.clone(), TakeRef::Active, 441);
    assert_eq!(peaks.num_channels, 1);
    assert_eq!(peaks.samples_per_peak, 441);
    // 0.5 s at 44.1k / 441 = 50 blocks of (min, max).
    assert_eq!(peaks.peaks.len(), 100);
    assert!(peaks.peaks.iter().any(|p| p.abs() > 0.3), "real peaks");
    assert!(peaks.peaks.chunks(2).all(|p| p[0] <= 0.0 && p[1] >= 0.0));

    // A take with no media: empty, not an error.
    set_take(&daw, |t| {
        t.source_file_path = Some("/nonexistent/x.wav".into())
    });
    daw.write_project("p", |p| {
        p.audio_sources.clear();
    });
    let none = daw.take_peaks(ProjectContext::Current, item, TakeRef::Active, 441);
    assert!(none.peaks.is_empty());
}
