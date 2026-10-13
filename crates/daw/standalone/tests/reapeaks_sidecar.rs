//! Source-level peak-cache persistence (the `reapeaks` feature): take
//! peaks for on-disk PCM media fold from a REAPER-compatible cache, a
//! `.reapeaks` REAPER itself left beside the media is honoured, the
//! first scan writes a `.sessionpeaks` in `Peaks/`, and a stale cache
//! (wrong stamp / wrong length) is recomputed.

#![cfg(all(feature = "reapeaks", feature = "decode", not(target_arch = "wasm32")))]

use std::path::{Path, PathBuf};

use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{ItemRef, Peaks, ProjectInfo, TakeRef, Takes, TrackRef, Tracks};
use daw_standalone::audio_engine::materialize::materialize_audio_streaming;
use daw_standalone::reapeaks::{ReaPeaks, stamp};
use daw_standalone::sync::Standalone;

const RATE: u32 = 48_000;
const FRAMES: usize = 48_000; // 1 s mono
const FINE: usize = 160; // finest mipmap ratio

/// 1 s mono 16-bit WAV: a 440 Hz sine at 0.9 full scale, so every
/// 480-sample peak window has max ≈ 0.9 / min ≈ −0.9.
fn write_sine_wav(path: &Path) {
    let mut data = Vec::with_capacity(44 + FRAMES * 2);
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&((36 + FRAMES * 2) as u32).to_le_bytes());
    data.extend_from_slice(b"WAVE");
    data.extend_from_slice(b"fmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes()); // PCM
    data.extend_from_slice(&1u16.to_le_bytes()); // mono
    data.extend_from_slice(&RATE.to_le_bytes());
    data.extend_from_slice(&(RATE * 2).to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&((FRAMES * 2) as u32).to_le_bytes());
    for i in 0..FRAMES {
        let t = i as f64 / RATE as f64;
        let s = 0.9 * (t * 440.0 * 2.0 * std::f64::consts::PI).sin();
        data.extend_from_slice(&((s * i16::MAX as f64) as i16).to_le_bytes());
    }
    std::fs::write(path, data).unwrap();
}

/// REAPER's own placement, beside the media — what this test seeds, to
/// prove a cache REAPER wrote is read rather than rebuilt.
fn sidecar_path(media: &Path) -> PathBuf {
    PathBuf::from(format!("{}.reapeaks", media.display()))
}

/// Where a scan writes: `Peaks/<name>.sessionpeaks` beside the media.
fn cache_path(media: &Path) -> PathBuf {
    dawfile_reaper::sessionpeaks::cache_path(media).unwrap()
}

/// The size-and-mtime stamp REAPER records, which is what the sidecar
/// is validated against.
fn media_stamp(media: &Path) -> u64 {
    let meta = std::fs::metadata(media).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    stamp(meta.len(), mtime)
}

/// A Standalone with one audio take materialized (streamed, mmap'd)
/// from `media`. Returns the daw and the item ref.
fn daw_with_media(name: &str, media: &Path) -> (Standalone, String, ItemRef) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: format!("reapeaks-{name}"),
        name: name.into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track_guid = Tracks::add(&daw, ctx.clone(), "Audio", None).unwrap();
    let loc =
        Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track_guid), 0.0, 1.0).unwrap();
    let item = loc.item.clone();
    let active = Takes::get_active_take(&daw, ctx, item.clone()).unwrap();
    let media_str = media.display().to_string();
    daw.write_project(&guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.source_file_path = Some(media_str.clone());
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                }
            }
        }
    });
    let media_path = media.to_path_buf();
    let report = materialize_audio_streaming(
        &daw,
        &guid,
        |p| std::fs::read(p).map_err(|e| e.to_string()),
        move |_| Some(media_path.clone()),
    );
    assert_eq!(report.loaded, 1, "materialize failed: {report:?}");
    (daw, guid, item)
}

fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fts-reapeaks-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A valid sidecar whose peaks are a constant `level` — deliberately
/// unlike the real audio, so serving it is distinguishable from a scan.
fn constant_sidecar(level: f32, stamp: u64) -> ReaPeaks {
    let mut pk = ReaPeaks::compute(1, RATE, FRAMES, |_, _| level);
    pk.source_stamp = stamp;
    pk
}

#[test]
fn first_scan_writes_a_sidecar_matching_the_pcm() {
    let dir = fresh_dir("compute");
    let media = dir.join("tone.wav");
    write_sine_wav(&media);
    let (daw, _guid, item) = daw_with_media("compute", &media);

    // 480 samples/block = 3 aligned fine windows: the sidecar fold and a
    // direct PCM scan see the same absolute windows, so values agree to
    // i16 quantization.
    let data = daw.take_peaks(ProjectContext::Current, item, TakeRef::Active, 480);
    assert_eq!(data.num_channels, 1);
    assert_eq!(data.peaks.len() / 2, 100, "1 s / 10 ms blocks");

    // Expected min/max per 480-frame window straight from the signal.
    for (b, pair) in data.peaks.chunks(2).enumerate() {
        let (mut mn, mut mx) = (f64::MAX, f64::MIN);
        for i in b * 480..(b + 1) * 480 {
            let t = i as f64 / RATE as f64;
            let s = 0.9 * (t * 440.0 * 2.0 * std::f64::consts::PI).sin();
            let s = ((s * i16::MAX as f64) as i16) as f64 / 32767.0;
            mn = mn.min(s);
            mx = mx.max(s);
        }
        assert!(
            (pair[0] - mn.min(0.0)).abs() < 2e-3 && (pair[1] - mx.max(0.0)).abs() < 2e-3,
            "block {b}: got ({}, {}), expected ({mn}, {mx})",
            pair[0],
            pair[1]
        );
    }

    // The scan persisted a valid REAPER-format cache in `Peaks/`.
    let side = cache_path(&media);
    let pk = ReaPeaks::read(&side).expect("cache written and parses");
    assert_eq!(pk.channels, 1);
    assert_eq!(pk.samplerate, RATE);
    assert_eq!(pk.source_stamp, media_stamp(&media));
    assert_eq!(pk.levels[0].samples_per_peak as usize, FINE);
    assert_eq!(pk.levels[0].count, FRAMES.div_ceil(FINE));
}

#[test]
fn valid_sidecar_serves_peaks_without_scanning_pcm() {
    let dir = fresh_dir("serve");
    let media = dir.join("tone.wav");
    write_sine_wav(&media);
    // Pre-seed a VALID sidecar carrying constant 0.25 peaks — nothing
    // like the 0.9 sine on disk. If peaks come back 0.25, they were
    // folded from the sidecar, not scanned from PCM.
    constant_sidecar(0.25, media_stamp(&media))
        .write(sidecar_path(&media))
        .unwrap();

    let (daw, _guid, item) = daw_with_media("serve", &media);
    let data = daw.take_peaks(ProjectContext::Current, item, TakeRef::Active, 480);
    let max = data.peaks.chunks(2).map(|p| p[1]).fold(0.0f64, f64::max);
    assert!(
        (max - 0.25).abs() < 2e-3,
        "expected sidecar-fed peaks (~0.25), got max {max} — PCM was scanned"
    );
}

#[test]
fn stale_sidecar_is_recomputed() {
    // Wrong stamp.
    let dir = fresh_dir("stale-mtime");
    let media = dir.join("tone.wav");
    write_sine_wav(&media);
    constant_sidecar(0.25, media_stamp(&media) + 999)
        .write(sidecar_path(&media))
        .unwrap();

    let (daw, _guid, item) = daw_with_media("stale-mtime", &media);
    let data = daw.take_peaks(ProjectContext::Current, item, TakeRef::Active, 480);
    let max = data.peaks.chunks(2).map(|p| p[1]).fold(0.0f64, f64::max);
    assert!(max > 0.8, "stale sidecar must be ignored; got max {max}");
    // ... and rewritten with the real stamp + real audio.
    let pk = ReaPeaks::read(cache_path(&media)).unwrap();
    assert_eq!(pk.source_stamp, media_stamp(&media));
    let (pmax, _) = pk.levels[0].pair(1, 0, 40);
    assert!(pmax > 0.8, "recomputed sidecar carries real peaks: {pmax}");

    // Wrong length (right stamp): also recomputed.
    let dir = fresh_dir("stale-len");
    let media = dir.join("tone.wav");
    write_sine_wav(&media);
    let mut short = ReaPeaks::compute(1, RATE, FRAMES / 2, |_, _| 0.25);
    short.source_stamp = media_stamp(&media);
    short.write(sidecar_path(&media)).unwrap();

    let (daw, _guid, item) = daw_with_media("stale-len", &media);
    let data = daw.take_peaks(ProjectContext::Current, item, TakeRef::Active, 480);
    let max = data.peaks.chunks(2).map(|p| p[1]).fold(0.0f64, f64::max);
    assert!(max > 0.8, "wrong-length sidecar must be ignored; got {max}");
    let pk = ReaPeaks::read(cache_path(&media)).unwrap();
    assert_eq!(pk.levels[0].count, FRAMES.div_ceil(FINE));
}
