//! End-to-end: build a project with takes that point at synthetic
//! source paths, hand a closure resolver to `materialize_audio`, and
//! verify the decoded PCM lands in `ProjectState.audio_sources`.

#![cfg(feature = "decode")]

use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{ProjectInfo, TrackRef, Tracks};
use daw_standalone::audio_engine::DecodedAudio;
use daw_standalone::audio_engine::materialize::{attach_audio_source, materialize_audio};
use daw_standalone::sync::Standalone;

fn seeded() -> (Standalone, String) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "test-proj".into(),
        name: "test".into(),
        path: String::new(),
    });
    (daw, guid)
}

/// 1-second 440Hz mono 16-bit PCM WAV so symphonia has something to
/// chew on.
fn synth_440hz_wav_1s() -> Vec<u8> {
    let sample_rate: u32 = 44100;
    let num_samples = sample_rate as usize;
    let mut data = Vec::with_capacity(44 + num_samples * 2);
    data.extend_from_slice(b"RIFF");
    let chunk_size: u32 = 36 + (num_samples as u32) * 2;
    data.extend_from_slice(&chunk_size.to_le_bytes());
    data.extend_from_slice(b"WAVE");
    data.extend_from_slice(b"fmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes()); // PCM
    data.extend_from_slice(&1u16.to_le_bytes()); // mono
    data.extend_from_slice(&sample_rate.to_le_bytes());
    data.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&((num_samples as u32) * 2).to_le_bytes());
    for i in 0..num_samples {
        let t = i as f64 / sample_rate as f64;
        let s = (t * 440.0 * 2.0 * std::f64::consts::PI).sin();
        let v = (s * i16::MAX as f64) as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    data
}

#[test]
fn attach_audio_source_directly_round_trips() {
    let (daw, guid) = seeded();
    let take_guid = "synth-take";
    let decoded = DecodedAudio::new(vec![0.1, -0.1, 0.2, -0.2], 1, 48_000);
    attach_audio_source(&daw, &guid, take_guid, decoded);

    let got = daw.audio_source(&guid, take_guid).expect("attached");
    assert_eq!(got.frame_count(), 4);
    assert_eq!(got.channels(), 1);
    assert_eq!(got.sample_rate(), 48_000);
    assert_eq!(daw.audio_source_count(&guid), 1);
}

#[test]
fn materialize_resolves_and_decodes_take_sources() {
    use daw_proto::Takes;

    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;

    // Create a real item + take; rewrite the take to point at an
    // audio source path so materialize has something to resolve.
    let track_guid = Tracks::add(&daw, ctx.clone(), "Synth", None).unwrap();
    let loc = Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track_guid), 0.0, 1.0)
        .expect("midi item");
    let item_guid = match &loc.item {
        daw_proto::ItemRef::Guid(g) => g.clone(),
        _ => panic!(),
    };
    let active = Takes::get_active_take(&daw, ctx, daw_proto::ItemRef::Guid(item_guid)).unwrap();

    daw.write_project(&guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.source_file_path = Some("tone.wav".into());
                    t.is_midi = false;
                    t.source_type = daw_proto::item::SourceType::Audio;
                }
            }
        }
    });

    let wav = synth_440hz_wav_1s();
    let report = materialize_audio(&daw, &guid, |path| {
        assert_eq!(path, "tone.wav");
        Ok(wav.clone())
    });
    assert_eq!(report.loaded, 1, "expected 1 loaded, got {report:?}");
    assert!(report.failed.is_empty(), "failed: {:?}", report.failed);

    let decoded = daw.audio_source(&guid, &active.guid).expect("attached");
    assert_eq!(decoded.sample_rate(), 44_100);
    assert!(decoded.frame_count() >= 44_000);
}

#[test]
fn materialize_reports_resolver_failure_without_panic() {
    let (daw, guid) = seeded();
    let ctx = ProjectContext::Current;

    let track_guid = Tracks::add(&daw, ctx.clone(), "T", None).unwrap();
    let loc =
        Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track_guid), 0.0, 1.0).unwrap();
    let item_guid = match &loc.item {
        daw_proto::ItemRef::Guid(g) => g.clone(),
        _ => panic!(),
    };
    let active =
        daw_proto::Takes::get_active_take(&daw, ctx, daw_proto::ItemRef::Guid(item_guid)).unwrap();
    daw.write_project(&guid, |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                if t.guid == active.guid {
                    t.source_file_path = Some("missing.wav".into());
                }
            }
        }
    });

    let report = materialize_audio(&daw, &guid, |_| Err("file not found".into()));
    assert_eq!(report.loaded, 0);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].0, active.guid);
    assert!(report.failed[0].1.contains("file not found"));
}
