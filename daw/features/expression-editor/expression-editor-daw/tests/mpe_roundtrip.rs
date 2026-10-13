//! Scenario 3: MPE parameters through the facade and back (#167).
//!
//! The founding claim of the MPE half of the editor is that per-note
//! bend, pressure and timbre are properties *of a note* rather than
//! entries in raw controller lanes. That only holds if they survive the
//! trip to a backend and back — otherwise the model is a UI fiction.
//!
//! Driven through the `Midi` trait the REAPER build implements, against
//! the standalone backend, so the whole loop runs with no DAW.

use daw::service::midi::{Midi, MidiNoteCreate, MidiTakeLocation};
use daw::service::project::ProjectInfo;
use daw::service::{ProjectContext, TrackRef, Tracks};
use daw::standalone::Standalone;
use expression_editor_core::{Dimension, Point, Viewport};
use expression_editor_daw::Session;

const PPQ: f64 = 960.0;
/// The editor's own default, and the number that has to reach the
/// instrument or every curve reads wrong by a factor.
const BEND_RANGE: f64 = 48.0;

fn fixture() -> (Standalone, MidiTakeLocation) {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "test-proj".into(),
        name: "Test".into(),
        path: String::new(),
    });
    let project = ProjectContext::Current;
    <Standalone as Tracks>::add(&daw, project.clone(), "Test", None).expect("add track");
    let location =
        Midi::create_midi_item(&daw, project, TrackRef::Index(0), 0.0, 8.0).expect("midi item");

    // One note per member channel, so per-note expression is
    // attributable — the whole basis of MPE.
    let notes = (0..3)
        .map(|i| MidiNoteCreate {
            channel: i as u8 + 1,
            pitch: 60 + i as u8 * 4,
            velocity: 100,
            start_ppq: PPQ * i as f64,
            length_ppq: PPQ,
        })
        .collect();
    // In the units `notes()` reads back (what the session edits);
    // `add_notes` takes REAPER's tick lengths.
    Midi::add_notes_ppq(&daw, location.clone(), notes);
    (daw, location)
}

fn session(daw: &Standalone, location: &MidiTakeLocation) -> Session {
    Session::load(
        daw,
        location.clone(),
        BEND_RANGE,
        Viewport::new(800.0, 600.0),
    )
}

/// Write the edits and read the take back fresh.
fn round_trip(daw: &Standalone, location: &MidiTakeLocation, mut s: Session) -> Session {
    s.write_back(daw);
    session(daw, location)
}

#[test]
fn a_bend_curve_survives_the_facade() {
    let (daw, location) = fixture();
    let mut s = session(&daw, &location);

    let id = s.editor.doc.notes[0].id;
    let note = s.editor.doc.note_mut(id).expect("note");
    *note.curve_mut(Dimension::Pitch) = expression_editor_core::Curve::from_points(vec![
        Point::new(0.0, 0.0),
        Point::new(PPQ * 0.5, 2.0),
        Point::new(PPQ, 0.0),
    ]);

    let back = round_trip(&daw, &location, s);
    let curve = back.editor.doc.notes[0].curve(Dimension::Pitch);
    assert!(!curve.points().is_empty(), "the bend did not survive");
    let peak = curve
        .points()
        .iter()
        .map(|p| p.value)
        .fold(f64::MIN, f64::max);
    assert!(
        (peak - 2.0).abs() < 0.15,
        "a two-semitone bend came back as {peak}"
    );
}

#[test]
fn channel_pressure_survives_the_facade() {
    let (daw, location) = fixture();
    let mut s = session(&daw, &location);
    let id = s.editor.doc.notes[0].id;
    *s.editor
        .doc
        .note_mut(id)
        .unwrap()
        .curve_mut(Dimension::Pressure) = expression_editor_core::Curve::from_points(vec![
        Point::new(0.0, 0.2),
        Point::new(PPQ, 0.9),
    ]);

    let back = round_trip(&daw, &location, s);
    let curve = back.editor.doc.notes[0].curve(Dimension::Pressure);
    assert!(!curve.points().is_empty(), "pressure did not survive");
    let last = curve.points().last().unwrap().value;
    assert!((last - 0.9).abs() < 0.02, "pressure came back as {last}");
}

#[test]
fn cc74_timbre_survives_the_facade() {
    let (daw, location) = fixture();
    let mut s = session(&daw, &location);
    let id = s.editor.doc.notes[0].id;
    *s.editor
        .doc
        .note_mut(id)
        .unwrap()
        .curve_mut(Dimension::Timbre) = expression_editor_core::Curve::from_points(vec![
        Point::new(0.0, 0.1),
        Point::new(PPQ, 0.8),
    ]);

    let back = round_trip(&daw, &location, s);
    let curve = back.editor.doc.notes[0].curve(Dimension::Timbre);
    assert!(!curve.points().is_empty(), "timbre did not survive");
    let last = curve.points().last().unwrap().value;
    assert!((last - 0.8).abs() < 0.02, "timbre came back as {last}");
}

#[test]
fn every_note_keeps_its_own_channel() {
    // Per-note channel assignment *is* MPE: collapse it and every
    // note's expression lands on every other note.
    let (daw, location) = fixture();
    let s = session(&daw, &location);
    let before: Vec<Option<u8>> = s.editor.doc.notes.iter().map(|n| n.channel).collect();
    assert!(
        before.iter().all(|c| c.is_some()),
        "the loader dropped channel assignment: {before:?}"
    );

    let back = round_trip(&daw, &location, s);
    let after: Vec<Option<u8>> = back.editor.doc.notes.iter().map(|n| n.channel).collect();
    assert_eq!(before, after);

    let mut distinct = after.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 3, "three notes should hold three channels");
}

#[test]
fn the_three_dimensions_are_independent_of_each_other() {
    // Editing pressure must not disturb the bend — they are separate
    // properties of the note, not one blended stream.
    let (daw, location) = fixture();
    let mut s = session(&daw, &location);
    let id = s.editor.doc.notes[0].id;
    {
        let n = s.editor.doc.note_mut(id).unwrap();
        *n.curve_mut(Dimension::Pitch) =
            expression_editor_core::Curve::from_points(vec![Point::new(0.0, 1.0)]);
        *n.curve_mut(Dimension::Pressure) =
            expression_editor_core::Curve::from_points(vec![Point::new(0.0, 0.5)]);
    }

    let back = round_trip(&daw, &location, s);
    let n = &back.editor.doc.notes[0];
    assert!(!n.curve(Dimension::Pitch).points().is_empty());
    assert!(!n.curve(Dimension::Pressure).points().is_empty());
}

#[test]
fn the_bend_range_the_session_loads_with_is_the_one_it_writes_with() {
    // If these disagree, every pitch curve is wrong by a factor — the
    // failure the ticket calls out by name.
    let (daw, location) = fixture();
    let s = session(&daw, &location);
    assert_eq!(s.bend_range, BEND_RANGE);

    let mut s = s;
    let id = s.editor.doc.notes[0].id;
    *s.editor
        .doc
        .note_mut(id)
        .unwrap()
        .curve_mut(Dimension::Pitch) =
        expression_editor_core::Curve::from_points(vec![Point::new(0.0, 12.0)]);
    s.write_back(&daw);

    // Reloading at *half* the range must read the same wheel data as
    // twice the semitones, which is what makes the factor visible.
    let narrow = Session::load(
        &daw,
        location.clone(),
        BEND_RANGE / 2.0,
        Viewport::new(800.0, 600.0),
    );
    let v = narrow.editor.doc.notes[0]
        .curve(Dimension::Pitch)
        .points()
        .first()
        .map(|p| p.value)
        .unwrap_or(0.0);
    assert!(
        (v - 6.0).abs() < 0.2,
        "12 semitones at half the range should read as 6, got {v}"
    );
}

#[test]
fn an_untouched_take_round_trips_unchanged() {
    // The base case worth pinning: opening and writing back a take
    // nobody edited must not perturb it.
    let (daw, location) = fixture();
    let s = session(&daw, &location);
    let before: Vec<(f64, i32)> = s
        .editor
        .doc
        .notes
        .iter()
        .map(|n| (n.start, n.row))
        .collect();

    let back = round_trip(&daw, &location, s);
    let after: Vec<(f64, i32)> = back
        .editor
        .doc
        .notes
        .iter()
        .map(|n| (n.start, n.row))
        .collect();
    assert_eq!(before, after);
}
