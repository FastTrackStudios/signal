//! Session round trips against the standalone backend.
//!
//! Real service calls — `create_midi_item`, `write_take`, `read_take` —
//! through the same `Midi` trait REAPER implements. Being generic over
//! the traits rather than a backend is what lets the whole load → edit
//! → write-back loop be tested with no DAW running.

use daw::service::midi::{Midi, MidiTakeLocation};
use daw::service::project::ProjectInfo;
use daw::service::{ProjectContext, TrackRef, Tracks};
use daw::standalone::Standalone;
use expression_editor_core::{Edit, Viewport};
use expression_editor_daw::Session;

const PPQ: f64 = 960.0;

/// A project with one track holding one empty MIDI item.
fn fixture() -> (Standalone, MidiTakeLocation) {
    let daw = Standalone::new();
    // Standalone starts with no projects; one has to be seeded before
    // anything can resolve `Current`.
    daw.seed_project(ProjectInfo {
        guid: "test-proj".into(),
        name: "Test".into(),
        path: String::new(),
    });
    let project = ProjectContext::Current;
    <Standalone as Tracks>::add(&daw, project.clone(), "Test", None).expect("add track");
    let location = Midi::create_midi_item(&daw, project.clone(), TrackRef::Index(0), 0.0, 8.0)
        .expect("standalone must be able to create a MIDI item");
    (daw, location)
}

fn seed(daw: &Standalone, location: &MidiTakeLocation, pitches: &[u8]) {
    use daw::service::midi::MidiNoteCreate;
    let notes = pitches
        .iter()
        .enumerate()
        .map(|(i, &pitch)| MidiNoteCreate {
            channel: i as u8 % 16,
            pitch,
            velocity: 100,
            start_ppq: PPQ * i as f64,
            length_ppq: PPQ,
        })
        .collect();
    Midi::add_notes(daw, location.clone(), notes);
}

#[test]
fn a_session_loads_a_take_and_reports_itself_clean() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60, 64, 67]);

    let session = Session::load(&daw, location, 48.0, Viewport::new(900.0, 500.0));
    assert_eq!(session.editor.doc.notes.len(), 3);
    assert!(!session.is_dirty(), "a freshly loaded session has no edits");
    assert!(session.warnings().is_empty());
}

#[test]
fn editing_marks_the_session_dirty_and_writing_clears_it() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60, 64]);
    let mut session = Session::load(&daw, location.clone(), 48.0, Viewport::new(900.0, 500.0));

    let id = session.editor.doc.notes[0].id;
    session.editor.apply(&Edit::Transpose {
        notes: vec![id],
        semitones: 12,
    });
    assert!(session.is_dirty());

    session.write_back(&daw);
    assert!(!session.is_dirty(), "writing makes the take the baseline");

    // And the change is genuinely in the take, not just in the document.
    let back = Midi::read_take(&daw, location);
    assert!(back.notes.iter().any(|n| n.pitch == 72));
    assert_eq!(back.notes.len(), 2, "a replace must not duplicate");
}

#[test]
fn a_write_back_replaces_rather_than_appends() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60, 62, 64, 65]);
    let mut session = Session::load(&daw, location.clone(), 48.0, Viewport::new(900.0, 500.0));

    // Delete half, then write.
    let doomed: Vec<_> = session.editor.doc.notes[..2].iter().map(|n| n.id).collect();
    session.editor.apply(&Edit::DeleteNotes(doomed));
    session.write_back(&daw);

    let back = Midi::read_take(&daw, location);
    assert_eq!(
        back.notes.len(),
        2,
        "the deleted notes are gone from the take"
    );
}

#[test]
fn reload_discards_local_edits_but_keeps_the_camera() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60, 64]);
    let mut session = Session::load(&daw, location, 48.0, Viewport::new(900.0, 500.0));

    session.editor.zoom_in_at(400.0, 200.0, 3.0);
    let camera = session.editor.camera;

    let id = session.editor.doc.notes[0].id;
    session.editor.apply(&Edit::DeleteNotes(vec![id]));
    assert!(session.is_dirty());

    session.reload(&daw);
    assert!(!session.is_dirty());
    assert_eq!(
        session.editor.doc.notes.len(),
        2,
        "the take is authoritative"
    );
    // Throwing away the user's view on every refresh would be its own
    // kind of data loss.
    assert_eq!(
        session.editor.camera, camera,
        "the camera survives a reload"
    );
}

#[test]
fn velocity_edits_reach_the_take() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60]);
    let mut session = Session::load(&daw, location.clone(), 48.0, Viewport::new(900.0, 500.0));

    let id = session.editor.doc.notes[0].id;
    session.editor.apply(&Edit::SetVelocity {
        notes: vec![id],
        velocity: 0.25,
    });
    session.write_back(&daw);

    let back = Midi::read_take(&daw, location);
    assert_eq!(back.notes[0].velocity, 32, "0.25 × 127, rounded");
}

#[test]
fn a_pitch_curve_becomes_bend_events_in_the_take() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60]);
    let mut session = Session::load(&daw, location.clone(), 48.0, Viewport::new(900.0, 500.0));

    let id = session.editor.doc.notes[0].id;
    {
        let n = session.editor.doc.note_mut(id).unwrap();
        n.pitch.set(n.start, 0.0);
        n.pitch.set(n.end, 2.0);
    }
    session.write_back(&daw);

    let back = Midi::read_take(&daw, location);
    assert!(!back.pitch_bends.is_empty(), "the curve was written");
    // The last sample should be near +2 semitones at a 48-semitone
    // range: 2/48 of full deflection.
    let last = back.pitch_bends.iter().map(|b| b.value).max().unwrap();
    let expected = ((2.0f64 / 48.0) * 8191.0) as i16;
    assert!(
        (last - expected).abs() < 40,
        "expected about {expected}, got {last}"
    );
}

#[test]
fn a_range_write_leaves_the_rest_of_the_take_alone() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60, 62, 64, 65]);
    let mut session = Session::load(&daw, location.clone(), 48.0, Viewport::new(900.0, 500.0));

    // Transpose only what is inside the range, then write only that.
    let inside: Vec<_> = session
        .editor
        .doc
        .notes
        .iter()
        .filter(|n| n.start >= PPQ && n.start < PPQ * 3.0)
        .map(|n| n.id)
        .collect();
    session.editor.apply(&Edit::Transpose {
        notes: inside,
        semitones: 12,
    });
    session.write_range(&daw, PPQ, PPQ * 3.0);

    let back = Midi::read_take(&daw, location);
    assert_eq!(back.notes.len(), 4, "nothing was added or lost");
    let mut pitches: Vec<u8> = back.notes.iter().map(|n| n.pitch).collect();
    pitches.sort_unstable();
    // 62 and 64 moved up an octave; 60 and 65 are untouched.
    assert_eq!(pitches, vec![60, 65, 74, 76]);
}

#[test]
fn loading_the_selected_item_finds_nothing_when_nothing_is_selected() {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "test-proj".into(),
        name: "Test".into(),
        path: String::new(),
    });
    // An empty editor opened on nothing looks like a broken load, so
    // the caller has to be able to tell the difference.
    assert!(
        Session::load_selected(
            &daw,
            ProjectContext::Current,
            48.0,
            Viewport::new(900.0, 500.0)
        )
        .is_none()
    );
}

#[test]
fn a_midi_file_opens_as_a_session_and_exports_back() {
    let (daw, location) = fixture();
    seed(&daw, &location, &[60, 64, 67]);
    let session = Session::load(&daw, location.clone(), 48.0, Viewport::new(900.0, 500.0));

    let dir = std::env::temp_dir().join("fts-expression-editor-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.mid");
    let path = path.to_string_lossy().to_string();

    assert!(session.export_file(&daw, &path), "export must succeed");

    let reopened = Session::from_file(&daw, &path, 0, location, 48.0, Viewport::new(900.0, 500.0))
        .expect("the file we just wrote must open");
    assert_eq!(
        reopened.editor.doc.notes.len(),
        session.editor.doc.notes.len()
    );
    let a_src = &session.editor.doc;
    let mut a: Vec<i32> = a_src.notes.iter().map(|n| n.row).collect();
    let mut b: Vec<i32> = reopened.editor.doc.notes.iter().map(|n| n.row).collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);

    let _ = std::fs::remove_file(&path);
}
