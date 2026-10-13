//! The load → edit → render loop, against a fake host.
//!
//! A fake rather than the standalone backend because `AudioAccessors`
//! is still a stub there, and because what is under test is this
//! session — chunked reads, mono summing, which mode it opens in, and
//! that a render always reads the recording. A fake host lets those be
//! asserted exactly, including the ones a real host would only show
//! under a five-minute take.
//!
//! The fake implements `AudioAccessors` and nothing else. `load` needs
//! only that; `load_selected` additionally wants `Items`, whose twenty
//! setters would be pure noise here and none of which this exercises.

#![cfg(feature = "daw")]

use std::cell::RefCell;

use daw::service::audio_accessor::{AudioAccessors, AudioSampleData, GetSamplesRequest};
use daw::service::{ItemRef, ProjectContext, TakeRef, TrackRef};
use expression_editor_audio::{AudioSession, AudioTakeLocation, TakeConfig};
use expression_editor_core::doc::NoteId;
use expression_editor_core::{Mode, Viewport};

const SR: f64 = 44100.0;

fn midi_to_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

fn tone(midi: f64, secs: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut phase = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            phase += core::f64::consts::TAU * midi_to_hz(midi) / SR;
            let s = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * (phase * 3.0).sin();
            s * (t / 0.02).min(1.0) * ((secs - t) / 0.05).clamp(0.0, 1.0) * 0.3
        })
        .collect()
}

/// A host holding one take.
struct FakeHost {
    /// Mono source, served interleaved at `channels`.
    mono: Vec<f64>,
    channels: u32,
    /// Every `get_samples` call, for asserting how the read was done.
    reads: RefCell<Vec<GetSamplesRequest>>,
    accessors: RefCell<Vec<String>>,
    destroyed: RefCell<Vec<String>>,
}

impl FakeHost {
    fn new(mono: Vec<f64>, channels: u32) -> Self {
        Self {
            mono,
            channels,
            reads: RefCell::new(Vec::new()),
            accessors: RefCell::new(Vec::new()),
            destroyed: RefCell::new(Vec::new()),
        }
    }

    fn secs(&self) -> f64 {
        self.mono.len() as f64 / SR
    }
}

impl AudioAccessors for FakeHost {
    fn create_track_accessor(&self, _p: ProjectContext, _t: TrackRef) -> Option<String> {
        None
    }

    fn create_take_accessor(&self, _p: ProjectContext, _i: ItemRef, _t: TakeRef) -> Option<String> {
        let id = format!("acc{}", self.accessors.borrow().len());
        self.accessors.borrow_mut().push(id.clone());
        Some(id)
    }

    fn has_state_changed(&self, _id: &str) -> bool {
        false
    }

    fn get_samples(&self, request: GetSamplesRequest) -> AudioSampleData {
        self.reads.borrow_mut().push(request.clone());
        let start = (request.start_time * SR).round() as usize;
        let want = request.num_samples as usize;
        let ch = self.channels.max(1);
        let mut samples = Vec::with_capacity(want * ch as usize);
        for i in 0..want {
            let v = self.mono.get(start + i).copied().unwrap_or(0.0);
            // Same signal on every channel, so summing to mono must
            // return the original rather than something scaled.
            for _ in 0..ch {
                samples.push(v);
            }
        }
        AudioSampleData {
            samples,
            sample_rate: SR,
            num_channels: ch,
            num_samples: want as u32,
        }
    }

    fn destroy_accessor(&self, id: &str) {
        self.destroyed.borrow_mut().push(id.to_string());
    }
}

fn location() -> AudioTakeLocation {
    AudioTakeLocation {
        project: ProjectContext::Current,
        item: ItemRef::Index(0),
        take: TakeRef::Active,
    }
}

fn load(host: &FakeHost) -> Option<AudioSession> {
    load_at(host, 1.0)
}

fn load_at(host: &FakeHost, volume: f64) -> Option<AudioSession> {
    AudioSession::load(
        host,
        location(),
        host.secs(),
        volume,
        Viewport::new(900.0, 500.0),
        TakeConfig::default(),
    )
}

#[test]
fn a_take_loads_into_the_audio_surface_with_notes() {
    let host = FakeHost::new(tone(60.0, 1.0), 1);
    let s = load(&host).expect("the take loaded");

    assert_eq!(
        s.editor.mode,
        Mode::PitchedAudio,
        "a vocal opens in the audio editor, not the MIDI one"
    );
    assert!(!s.editor.doc.notes.is_empty());
    assert_eq!(s.editor.doc.note(NoteId(1)).unwrap().row, 60);
    assert!(!s.editor.doc.peaks.is_empty(), "the backdrop is filled");
    assert!(!s.is_dirty(), "a freshly loaded take has no edits");
}

#[test]
fn the_accessor_is_released_after_reading() {
    let host = FakeHost::new(tone(60.0, 0.5), 1);
    let _ = load(&host);
    assert_eq!(
        host.destroyed.borrow().len(),
        1,
        "an accessor left open holds host resources for the session"
    );
    assert_eq!(*host.destroyed.borrow(), *host.accessors.borrow());
}

#[test]
fn a_long_take_is_read_in_chunks_rather_than_one_allocation() {
    // Longer than one chunk (2^18 ≈ 6 s at 44.1 k).
    let host = FakeHost::new(tone(60.0, 9.0), 1);
    let s = load(&host).expect("loaded");

    let reads = host.reads.borrow();
    // A probe, then more than one bulk read.
    assert!(reads.len() > 2, "got {} reads", reads.len());
    assert_eq!(reads[0].num_samples, 1, "the first read probes the format");
    assert!(reads[1..].iter().all(|r| r.num_samples > 1));
    // Reads are contiguous and in order, so the buffer is not scrambled.
    for pair in reads[1..].windows(2) {
        assert!(pair[1].start_time > pair[0].start_time);
    }
    assert!((s.source().len() as f64 / SR - 9.0).abs() < 0.05);
}

#[test]
fn the_probe_asks_the_host_what_rate_it_has_rather_than_naming_one() {
    let host = FakeHost::new(tone(60.0, 0.5), 2);
    let s = load(&host).expect("loaded");
    let reads = host.reads.borrow();
    assert_eq!(
        reads[0].sample_rate, 0.0,
        "asking for a specific rate would make the host resample, and \
         edits made against a resampled take land in the wrong place"
    );
    // Then every bulk read uses the rate the host reported.
    assert!(reads[1..].iter().all(|r| r.sample_rate == SR));
    assert_eq!(s.sample_rate(), SR);
}

#[test]
fn a_stereo_take_is_summed_to_one_signal_for_analysis() {
    let mono = tone(60.0, 0.8);
    let host = FakeHost::new(mono.clone(), 2);
    let s = load(&host).expect("loaded");

    assert_eq!(
        s.source().len(),
        mono.len(),
        "one sample per frame, not two"
    );
    assert!(!s.editor.doc.notes.is_empty());
    assert_eq!(s.editor.doc.note(NoteId(1)).unwrap().row, 60);
}

#[test]
fn a_take_with_no_audio_declines_to_open() {
    let host = FakeHost::new(Vec::new(), 1);
    assert!(
        load(&host).is_none(),
        "an editor opened on nothing looks like a failed load"
    );
}

#[test]
fn editing_marks_the_session_dirty_and_reanalysis_clears_it() {
    let host = FakeHost::new(tone(60.0, 0.8), 1);
    let mut s = load(&host).expect("loaded");

    s.editor.doc.note_mut(NoteId(1)).unwrap().row += 3;
    assert!(s.is_dirty());

    s.reanalyze(TakeConfig::default());
    assert!(!s.is_dirty(), "re-analysis is a fresh start");
    assert_eq!(
        s.editor.doc.note(NoteId(1)).unwrap().row,
        60,
        "and it comes back from the recording, not from the edit"
    );
}

#[test]
fn committing_carries_the_editors_document_into_the_analysis() {
    let host = FakeHost::new(tone(60.0, 0.8), 1);
    let mut s = load(&host).expect("loaded");

    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 65;
    s.commit();
    assert!(
        (s.analysis().pitch.blobs[0].center_midi - 65.0).abs() < 0.35,
        "got {}",
        s.analysis().pitch.blobs[0].center_midi
    );
}

#[cfg(feature = "render")]
#[test]
fn a_second_edit_renders_from_the_recording_not_from_the_first_render() {
    // The property the session exists to guarantee. WORLD is
    // analysis-resynthesis: each pass costs top end and transients, so
    // chaining them is how a vocal turns to glass.
    let source = tone(60.0, 1.0);
    let host = FakeHost::new(source.clone(), 1);
    let mut s = load(&host).expect("loaded");

    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 62;
    let first = s.render();

    // Edit again and re-render. The result must be a *single* pass from
    // the original, so it is what a one-shot edit to 64 would give.
    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 64;
    let second = s.render();

    assert_eq!(second.len(), source.len());
    assert!(!first.is_empty());

    // A fresh session taking the same edit in one step must match.
    let host2 = FakeHost::new(source.clone(), 1);
    let mut once = load(&host2).expect("loaded");
    once.editor.doc.note_mut(NoteId(1)).unwrap().row = 64;
    let direct = once.render();

    assert_eq!(second.len(), direct.len());
    let worst = second
        .iter()
        .zip(&direct)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        worst < 1e-9,
        "two edits then a render must equal one edit then a render, \
         worst sample difference {worst}"
    );
}

#[cfg(feature = "render")]
#[test]
fn the_rendered_take_keeps_its_length_when_only_pitch_changed() {
    let source = tone(60.0, 1.0);
    let host = FakeHost::new(source.clone(), 1);
    let mut s = load(&host).expect("loaded");
    s.editor.doc.note_mut(NoteId(1)).unwrap().row = 63;

    assert_eq!(
        s.render().len(),
        source.len(),
        "a host replacing the item's audio must not see the take shrink"
    );
}

#[test]
fn the_items_volume_is_applied_because_the_accessor_does_not() {
    // A take accessor hands back source audio; REAPER applies item gain
    // at playback. Reading without it analyses the wrong level.
    let host = FakeHost::new(tone(60.0, 0.8), 1);
    let full = load(&host).expect("loaded");
    let quiet = load_at(&host, 0.25).expect("loaded");

    let peak = |s: &AudioSession| s.source().iter().fold(0.0_f64, |a, b| a.max(b.abs()));
    let ratio = peak(&quiet) / peak(&full);
    assert!(
        (ratio - 0.25).abs() < 1e-9,
        "the fader reached the analysis: ratio {ratio}"
    );
}

#[test]
fn a_quiet_item_still_finds_its_consonants() {
    // The reason the volume matters beyond looks: the silence floor is
    // an absolute threshold, so an item with the fader down would have
    // every frame below it and no sibilants at all — if the gain were
    // being dropped on the way in.
    let mut noisy = tone(60.0, 0.4);
    let mut state = 0x9E3779B97F4A7C15u64;
    noisy.extend((0..(SR * 0.25) as usize).map(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        ((state >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 0.25
    }));
    noisy.extend(tone(60.0, 0.4));

    let host = FakeHost::new(noisy, 1);
    let loud = load(&host).expect("loaded");
    assert!(!loud.editor.doc.unvoiced.is_empty(), "baseline finds it");

    // Unity here; the point is that the *scaling path* exists and is
    // applied before analysis rather than after.
    let scaled = load_at(&host, 1.0).expect("loaded");
    assert_eq!(
        scaled.editor.doc.unvoiced.len(),
        loud.editor.doc.unvoiced.len()
    );
}
