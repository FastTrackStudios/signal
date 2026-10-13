//! The whole path against the standalone backend, with no DAW running.
//!
//! Everything else in this crate tests the session against a fake host.
//! This one uses a real one, reached through the same `daw` facade the
//! REAPER build uses — which is the claim the facade exists to make and
//! the only way to check it: one `AudioSession`, two hosts, no
//! host-specific code anywhere in the editor.

#![cfg(feature = "daw")]

use daw::service::StretchMarkers;
use daw::service::midi::Midi;
use daw::service::{ItemRef, ProjectContext, TakeRef, Takes, TrackRef, Tracks};
use daw::standalone::sync::Standalone;
use expression_editor_audio::{AudioSession, AudioTakeLocation, TakeConfig, WriteOutcome};
use expression_editor_core::doc::NoteId;
use expression_editor_core::{Mode, Viewport};

const SR: u32 = 44_100;

fn midi_to_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

/// A mono 16-bit WAV of a sung note, with harmonics so YIN has a
/// period to find.
fn sung_wav(midi: f64, secs: f64) -> Vec<u8> {
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
    let mut phase = 0.0f64;
    for i in 0..n {
        let t = i as f64 / SR as f64;
        phase += core::f64::consts::TAU * midi_to_hz(midi) / SR as f64;
        let s = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * (phase * 3.0).sin();
        let env = (t / 0.02).min(1.0) * ((secs - t) / 0.05).clamp(0.0, 1.0);
        d.extend_from_slice(&((s * env * 0.25 * i16::MAX as f64) as i16).to_le_bytes());
    }
    d
}

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ee-standalone-{}-{}",
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

/// A standalone project holding one sung audio item.
fn project(midi: f64, secs: f64) -> (Standalone, ItemRef, f64, TempDir) {
    let dir = TempDir::new();
    let path = dir.0.join("vox.wav");
    std::fs::write(&path, sung_wav(midi, secs)).expect("write wav");

    let daw = Standalone::new();
    let guid = daw.seed_project(daw::service::ProjectInfo {
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

    daw.write_project(&guid, |p| {
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

    (daw, ItemRef::Guid(item_guid), secs, dir)
}

fn open(daw: &Standalone, item: ItemRef, secs: f64, volume: f64) -> Option<AudioSession> {
    AudioSession::load(
        daw,
        AudioTakeLocation {
            project: ProjectContext::Current,
            item,
            take: TakeRef::Active,
        },
        secs,
        volume,
        Viewport::new(900.0, 500.0),
        TakeConfig::default(),
    )
}

#[test]
fn a_standalone_take_loads_into_the_audio_editor() {
    // The claim: the editor's session, unchanged, over a host that is
    // not REAPER.
    let (daw, item, secs, _dir) = project(60.0, 1.0);
    let s = open(&daw, item, secs, 1.0).expect("the take loaded");

    assert_eq!(s.editor.mode, Mode::PitchedAudio);
    assert!(!s.editor.doc.notes.is_empty(), "the vocal was analysed");
    assert_eq!(
        s.editor.doc.note(NoteId(1)).unwrap().row,
        60,
        "and came back at the pitch that was written to disk"
    );
    assert!(!s.editor.doc.peaks.is_empty(), "with its waveform");
    assert!((s.sample_rate() - SR as f64).abs() < 1e-9);
}

#[test]
fn the_pitch_is_recovered_across_the_range_through_the_backend() {
    for midi in [50.0, 60.0, 72.0] {
        let (daw, item, secs, _dir) = project(midi, 0.8);
        let s = open(&daw, item, secs, 1.0).expect("loaded");
        assert_eq!(
            s.editor.doc.note(NoteId(1)).unwrap().row,
            midi as i32,
            "midi {midi} through the standalone accessor"
        );
    }
}

#[test]
fn the_items_gain_reaches_the_analysis_through_the_facade() {
    let (daw, item, secs, _dir) = project(60.0, 0.8);
    let full = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    let quiet = open(&daw, item, secs, 0.25).expect("loaded");

    let peak = |s: &AudioSession| s.source().iter().fold(0.0_f64, |a, b| a.max(b.abs()));
    assert!((peak(&quiet) / peak(&full) - 0.25).abs() < 1e-9);
}

#[test]
fn a_midi_take_is_declined_rather_than_opened_as_silence() {
    // The item exists but has no audio source. An editor opened on it
    // would show an empty roll and no reason why.
    let daw = Standalone::new();
    daw.seed_project(daw::service::ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Keys", None).unwrap();
    let loc =
        Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track), 0.0, 1.0).expect("item");

    assert!(open(&daw, loc.item, 1.0, 1.0).is_none());
}

#[test]
fn editing_and_re_analysing_behave_the_same_as_against_any_host() {
    let (daw, item, secs, _dir) = project(60.0, 0.8);
    let mut s = open(&daw, item, secs, 1.0).expect("loaded");

    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 64;
    assert!(s.is_dirty());
    s.commit();
    assert!((s.analysis().pitch.blobs[0].center_midi - 64.0).abs() < 0.35);

    s.reanalyze(TakeConfig::default());
    assert!(!s.is_dirty());
    assert_eq!(s.editor.doc.note(NoteId(1)).unwrap().row, 60);
}

#[cfg(feature = "render")]
#[test]
fn a_standalone_take_renders_at_its_edited_pitch() {
    // The full loop with a real backend at the front: read a file
    // through the facade, analyse, transpose, render, and measure the
    // audio that comes out.
    let (daw, item, secs, _dir) = project(60.0, 1.0);
    let mut s = open(&daw, item, secs, 1.0).expect("loaded");

    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 63;
    let out = s.render();
    assert_eq!(out.len(), s.source().len());

    let again = expression_editor_audio::analyze_take(&out, SR as f64, TakeConfig::default());
    assert!(!again.doc.notes.is_empty(), "the render still sings");
    assert_eq!(
        again.doc.note(NoteId(1)).unwrap().row,
        63,
        "and at the pitch it was moved to"
    );
}

#[cfg(feature = "render")]
#[test]
fn writing_back_renders_beside_the_original_and_never_over_it() {
    // The property that makes this safe to point at a vocal: the
    // recording is the only copy of a performance, and WORLD is lossy,
    // so overwriting would make every edit permanent *and* degrade what
    // later edits read from.
    let (daw, item, secs, dir) = project(60.0, 0.8);
    let original = dir.0.join("vox.wav");
    let before = std::fs::read(&original).expect("source exists");

    let mut s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 64;
    let WriteOutcome::Rendered { path: written, .. } = s.write_back(&daw).expect("wrote") else {
        panic!("a pitch edit must render");
    };

    assert_ne!(written, original.to_string_lossy(), "a new file");
    assert!(std::path::Path::new(&written).exists());
    assert_eq!(
        std::fs::read(&original).unwrap(),
        before,
        "the recording is untouched"
    );

    // The take now points at the render...
    let takes = Takes::get_takes(&daw, ProjectContext::Current, item.clone());
    let active = takes.iter().find(|t| t.is_active).unwrap();
    assert_eq!(active.source_file_path.as_deref(), Some(written.as_str()));

    // ...and opening the take again reads the edited pitch back.
    let again = open(&daw, item, secs, 1.0).expect("reloaded");
    assert_eq!(
        again.editor.doc.note(NoteId(1)).unwrap().row,
        64,
        "the edit survived the round trip through the host"
    );
}

#[cfg(feature = "render")]
#[test]
fn a_second_write_makes_the_next_numbered_render() {
    let (daw, item, secs, _dir) = project(60.0, 0.6);
    let mut s = open(&daw, item.clone(), secs, 1.0).expect("loaded");

    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 62;
    let WriteOutcome::Rendered { path: first, .. } = s.write_back(&daw).expect("wrote") else {
        panic!("expected a render");
    };
    assert!(!s.is_dirty(), "the document now describes what is on disk");

    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 65;
    let WriteOutcome::Rendered { path: second, .. } = s.write_back(&daw).expect("wrote again")
    else {
        panic!("expected a render");
    };

    assert_ne!(
        first, second,
        "a second edit is a new version, not a clobber"
    );
    assert!(
        std::path::Path::new(&first).exists(),
        "and the first survives"
    );
    assert!(first.ends_with("-fts-001.wav"), "got {first}");
    assert!(second.ends_with("-fts-002.wav"), "got {second}");
}

#[cfg(feature = "render")]
#[test]
fn writing_back_a_take_with_no_source_says_so() {
    let (daw, item, secs, _dir) = project(60.0, 0.5);
    let mut s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    // An edit, so the write is not short-circuited as unchanged.
    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 62;

    // Drop the source out from under the session.
    daw.write_project("p", |p| {
        for tl in p.takes.values_mut() {
            for t in tl.takes.iter_mut() {
                t.source_file_path = None;
            }
        }
    });

    assert_eq!(
        s.write_back(&daw),
        Err(expression_editor_audio::WriteError::NoSource)
    );
}

#[cfg(feature = "render")]
#[test]
fn a_timing_only_edit_never_touches_the_audio() {
    // The whole point of routing timing through the host: moving a note
    // must not cost a generation of resynthesis, and must leave the
    // recording exactly where it was.
    let (daw, item, secs, dir) = project(60.0, 1.0);
    let original = dir.0.join("vox.wav");
    let before = std::fs::read(&original).expect("source");

    let mut s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    let n = s.editor.doc.note_mut(NoteId(1)).unwrap();
    n.start += 10.0;
    n.end += 10.0;

    let out = s.write_back(&daw).expect("wrote");
    match out {
        WriteOutcome::Retimed { markers } => assert!(markers >= 2, "got {markers}"),
        other => panic!("timing alone must not render, got {other:?}"),
    }

    assert_eq!(
        std::fs::read(&original).unwrap(),
        before,
        "the recording is untouched"
    );
    // No new render appeared beside it.
    let renders: Vec<_> = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("-fts-"))
        .collect();
    assert!(renders.is_empty(), "no audio was written");

    // And the host has the warp.
    let markers = daw.get_stretch_markers(ProjectContext::Current, item, TakeRef::Active);
    assert!(!markers.is_empty());
    for pair in markers.windows(2) {
        assert!(pair[1].position > pair[0].position, "sorted, no repeats");
    }
}

#[test]
fn an_unchanged_take_writes_nothing_at_all() {
    let (daw, item, secs, _dir) = project(60.0, 0.6);
    let mut s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    assert_eq!(s.write_back(&daw).expect("wrote"), WriteOutcome::Unchanged);
    assert!(
        daw.get_stretch_markers(ProjectContext::Current, item, TakeRef::Active)
            .is_empty(),
        "opening a take must not give it a warp"
    );
}

#[cfg(feature = "render")]
#[test]
fn a_pitch_edit_renders_without_baking_the_timing_in() {
    // Both kinds at once. The render must carry the pitch and *not* the
    // warp, or the timing lands twice — once in the audio and again
    // from the markers on top.
    let (daw, item, secs, _dir) = project(60.0, 1.0);
    let mut s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    {
        let n = s.editor.doc.note_mut(NoteId(1)).unwrap();
        n.row = 64;
        n.start += 8.0;
        n.end += 8.0;
    }

    let out = s.write_back(&daw).expect("wrote");
    let WriteOutcome::Rendered { path, markers } = out else {
        panic!("a pitch edit renders");
    };
    assert!(markers >= 2, "and the timing still went to the host");

    // The rendered *file* sits where it was analysed — the markers do
    // the moving. The accessor reads the take as played (placement and
    // markers applied, the same as REAPER's — r[drums.open.
    // accessor-placement]), so a reload sees the note at its *moved*
    // position: the timing edit round-trips visibly.
    let rendered = std::fs::read(&path).expect("render exists");
    assert!(rendered.len() > 44);
    let reopened = open(&daw, item, secs, 1.0).expect("reload");
    let n = reopened.editor.doc.note(NoteId(1)).unwrap();
    assert_eq!(n.row, 64, "the pitch edit is in the audio");
    assert!(
        (n.start - 8.0).abs() < 3.0,
        "and the note is heard where it was moved to, got {}",
        n.start
    );
}

// ── dynamics: lanes summed into the take volume envelope ─────────────

#[cfg(feature = "render")]
#[test]
fn the_lane_sum_lands_on_the_takes_volume_envelope() {
    use daw::service::automation::Automation;
    use daw::service::{EnvelopeLocation, EnvelopeRef, TakeEnvelopeKind};
    use expression_editor_audio::dynamics::GainPoint;
    use expression_editor_audio::lanes::take_volume_to_db;
    use expression_editor_audio::{DynamicsLane, Lanes};

    let (daw, item, secs, _dir) = project(60.0, 1.0);
    let s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    let frames = s.analysis().frames.frames.len();

    // Two lanes, known values, so the sum is checkable.
    let mut lanes = Lanes::from_dynamics(&Default::default(), frames);
    lanes.set(
        DynamicsLane::Gate,
        (0..frames)
            .map(|f| GainPoint { frame: f, db: -3.0 })
            .collect(),
    );
    lanes.set(
        DynamicsLane::Sibilance,
        (0..frames)
            .map(|f| GainPoint { frame: f, db: -5.0 })
            .collect(),
    );

    let written = s.write_dynamics(&daw, &lanes, &Default::default(), false);
    assert!(written.points >= 2, "got {}", written.points);
    assert!(
        written.points < frames / 4,
        "thinned: {} points for {frames} frames",
        written.points
    );

    let env = EnvelopeLocation {
        track: daw::service::TrackRef::Index(0),
        envelope: EnvelopeRef::Take {
            item_guid: match &item {
                daw::service::ItemRef::Guid(g) => g.clone(),
                _ => panic!(),
            },
            take_guid: String::new(),
            kind: TakeEnvelopeKind::Volume,
        },
    };
    let points = daw.points(ProjectContext::Current, env);
    assert_eq!(points.len(), written.points);
    // -3 and -5 make -8, and the envelope holds a linear multiplier.
    let db = take_volume_to_db(points[0].value);
    assert!((db + 8.0).abs() < 0.3, "wanted -8 dB, envelope holds {db}");
}

#[cfg(feature = "render")]
#[test]
fn switching_every_lane_off_removes_the_envelope_rather_than_flattening_it() {
    use daw::service::automation::Automation;
    use daw::service::{EnvelopeLocation, EnvelopeRef, TakeEnvelopeKind};
    use expression_editor_audio::dynamics::GainPoint;
    use expression_editor_audio::{DynamicsLane, Lanes};

    let (daw, item, secs, _dir) = project(60.0, 0.8);
    let s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    let frames = s.analysis().frames.frames.len();
    let env = EnvelopeLocation {
        track: daw::service::TrackRef::Index(0),
        envelope: EnvelopeRef::Take {
            item_guid: match &item {
                daw::service::ItemRef::Guid(g) => g.clone(),
                _ => panic!(),
            },
            take_guid: String::new(),
            kind: TakeEnvelopeKind::Volume,
        },
    };

    let mut lanes = Lanes::from_dynamics(&Default::default(), frames);
    lanes.set(
        DynamicsLane::Gate,
        (0..frames)
            .map(|f| GainPoint { frame: f, db: -4.0 })
            .collect(),
    );
    s.write_dynamics(&daw, &lanes, &Default::default(), false);
    assert!(!daw.points(ProjectContext::Current, env.clone()).is_empty());

    // Off. Not flat at unity — gone.
    lanes.clear(DynamicsLane::Gate);
    let written = s.write_dynamics(&daw, &lanes, &Default::default(), false);
    assert_eq!(written.points, 0);
    assert!(
        daw.points(ProjectContext::Current, env).is_empty(),
        "dead automation is the user's problem to delete, so do not leave any"
    );
}

#[cfg(feature = "render")]
#[test]
fn rewriting_replaces_the_envelope_instead_of_stacking_on_it() {
    use daw::service::automation::Automation;
    use daw::service::{EnvelopeLocation, EnvelopeRef, TakeEnvelopeKind};
    use expression_editor_audio::dynamics::GainPoint;
    use expression_editor_audio::lanes::take_volume_to_db;
    use expression_editor_audio::{DynamicsLane, Lanes};

    let (daw, item, secs, _dir) = project(60.0, 0.8);
    let s = open(&daw, item.clone(), secs, 1.0).expect("loaded");
    let frames = s.analysis().frames.frames.len();
    let env = EnvelopeLocation {
        track: daw::service::TrackRef::Index(0),
        envelope: EnvelopeRef::Take {
            item_guid: match &item {
                daw::service::ItemRef::Guid(g) => g.clone(),
                _ => panic!(),
            },
            take_guid: String::new(),
            kind: TakeEnvelopeKind::Volume,
        },
    };

    let mut lanes = Lanes::from_dynamics(&Default::default(), frames);
    let write = |db: f64, lanes: &mut Lanes| {
        lanes.set(
            DynamicsLane::Gate,
            (0..frames).map(|f| GainPoint { frame: f, db }).collect(),
        );
        s.write_dynamics(&daw, lanes, &Default::default(), false)
    };

    write(-4.0, &mut lanes);
    write(-4.0, &mut lanes);
    let points = daw.points(ProjectContext::Current, env);
    let db = take_volume_to_db(points[0].value);
    assert!(
        (db + 4.0).abs() < 0.3,
        "the envelope is derived, so writing twice is still -4 dB, got {db}"
    );
}

#[cfg(feature = "render")]
#[test]
fn detections_are_marked_on_the_item_even_when_nothing_is_ducked() {
    use daw::service::Takes;
    use expression_editor_audio::Lanes;
    use expression_editor_audio::dynamics::{Detection, Dynamics, Region};

    let (daw, item, secs, _dir) = project(60.0, 0.8);
    let s = open(&daw, item.clone(), secs, 1.0).expect("loaded");

    // A found breath and a found sibilant, neither being reduced.
    let dynamics = Dynamics {
        regions: vec![
            Region {
                kind: Detection::Breath,
                start: 10,
                end: 20,
                peak: 0.01,
            },
            Region {
                kind: Detection::Sibilance,
                start: 40,
                end: 50,
                peak: 0.2,
            },
        ],
        ..Default::default()
    };
    let lanes = Lanes::default();

    let written = s.write_dynamics(&daw, &lanes, &dynamics, true);
    assert_eq!(written.points, 0, "nothing was ducked");
    assert_eq!(written.markers, 2);

    let markers = daw.get_take_markers(ProjectContext::Current, item, TakeRef::Active);
    assert_eq!(markers.len(), 2);
    let names: Vec<&str> = markers.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"breath"));
    assert!(names.contains(&"sibilance"));
}
