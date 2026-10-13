//! Sung notes draw as amplitude blobs, not rectangles.

use expression_editor_core::doc::{ExpressionDoc, Note, NoteId, TimeBase};
use expression_editor_core::{Editor, Mode, Viewport};
use expression_editor_ui::canvas;
use expression_editor_ui::theme;

const PPQ: f64 = 960.0;

/// One note whose contour and amplitude both vary, so a body that
/// ignored either would be visibly wrong.
fn editor(mode: Mode, offset: f64) -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ * 4.0, 60);
    n.weight = 0.8;
    for k in 0..64 {
        let f = k as f64 / 63.0;
        let t = n.start + (n.end - n.start) * f;
        n.pitch
            .set(t, offset + 0.6 * (f * core::f64::consts::TAU * 4.0).sin());
        // Swells in, fades out.
        n.pressure.set(t, (f * core::f64::consts::PI).sin());
    }
    doc.push(n);
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.set_mode(mode);
    ed
}

fn points(s: &str) -> Vec<(f64, f64)> {
    s.split_whitespace()
        .filter_map(|p| {
            let (a, b) = p.split_once(',')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .collect()
}

#[test]
fn audio_notes_draw_as_blobs_and_midi_notes_do_not() {
    let audio = canvas::note_rects(&editor(Mode::PitchedAudio, 0.0));
    assert!(audio[0].blob.is_some(), "a sung note is not a rectangle");

    for mode in [Mode::Midi, Mode::Mpe, Mode::Drums, Mode::Guitar] {
        let rects = canvas::note_rects(&editor(mode, 0.0));
        assert!(
            rects[0].blob.is_none(),
            "{mode:?} has no envelope to draw a body from"
        );
    }
    // Vocals is the manual's own product, so it draws them too.
    assert!(
        canvas::note_rects(&editor(Mode::Vocals, 0.0))[0]
            .blob
            .is_some()
    );
}

#[test]
fn the_body_thickness_follows_the_amplitude() {
    let rects = canvas::note_rects(&editor(Mode::PitchedAudio, 0.0));
    let pts = points(rects[0].blob.as_ref().unwrap());
    // The polygon is the top edge then the bottom edge reversed, so the
    // pair straddling each x is `i` and `len - 1 - i`.
    let n = pts.len();
    assert!(n >= 8 && n.is_multiple_of(2));
    let thickness = |i: usize| (pts[n - 1 - i].1 - pts[i].1).abs();

    // Pressure is a sine swell: thin at the ends, fattest in the middle.
    let mid = thickness(n / 4);
    let start = thickness(0);
    let end = thickness(n / 2 - 1);
    assert!(
        mid > start * 1.5 && mid > end * 1.5,
        "the swell is visible in the body: {start} .. {mid} .. {end}"
    );
}

#[test]
fn the_body_is_flat_and_the_pitch_track_moves_against_it() {
    let ed = editor(Mode::PitchedAudio, 0.0);
    let rects = canvas::note_rects(&ed);
    let pts = points(rects[0].blob.as_ref().unwrap());
    let n = pts.len();
    // Centre line of the top/bottom pair, per sample.
    let centers: Vec<f64> = (0..n / 2)
        .map(|i| (pts[i].1 + pts[n - 1 - i].1) * 0.5)
        .collect();

    let lo = centers.iter().cloned().fold(f64::MAX, f64::min);
    let hi = centers.iter().cloned().fold(f64::MIN, f64::max);
    assert!(
        hi - lo < 0.5,
        "the body sits flat, the way a MIDI note sits on its row — the \
         pitch track is what moves, and the gap between them is the \
         thing being edited. Wandered {}px",
        hi - lo
    );
}

#[test]
fn the_body_sits_at_the_notes_own_pitch_not_its_row() {
    // A note sung a semitone and a half sharp draws where it sounds, so
    // the row it is nearest is still readable underneath it.
    let base = editor(Mode::PitchedAudio, 0.0);
    let mut sharp_ed = editor(Mode::PitchedAudio, 1.5);
    // `Editor::new` frames its own content, so the two would otherwise
    // be measured through different cameras and the comparison would be
    // about zoom rather than pitch.
    sharp_ed.camera = base.camera;

    let flat = canvas::note_rects(&base)[0].blob_center.unwrap();
    let sharp = canvas::note_rects(&sharp_ed)[0].blob_center.unwrap();
    let expected = 1.5 * base.camera.vertical.px_per_row;
    // Up the screen is a smaller y.
    let moved = flat - sharp;
    assert!(
        (moved - expected).abs() < expected * 0.15,
        "moved {moved}px for a {expected}px detune"
    );
}

#[test]
fn the_recorded_envelope_outranks_an_authored_curve() {
    // A note with both: the waveform is what was recorded and carries
    // detail no control-point curve could, so it wins.
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ * 4.0, 60);
    n.weight = 0.5;
    // Pressure says "flat and quiet".
    n.pressure.set(0.0, 0.2);
    n.pressure.set(PPQ * 4.0, 0.2);
    // The recording says "loud, with grain".
    n.envelope = (0..200)
        .map(|k| if k % 2 == 0 { 1.0 } else { 0.85 })
        .collect();
    doc.push(n);
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.set_mode(Mode::PitchedAudio);

    let rects = canvas::note_rects(&ed);
    let pts = points(rects[0].blob.as_ref().unwrap());
    let n_pts = pts.len();
    let thickness = (pts[n_pts - 1].1 - pts[0].1).abs();
    let quiet = 0.2 * ed.camera.vertical.px_per_row;
    assert!(
        thickness > quiet * 2.0,
        "the recording won, not the trim: {thickness}px"
    );
}

#[test]
fn a_note_too_narrow_to_shape_gets_no_blob() {
    let mut ed = editor(Mode::PitchedAudio, 0.0);
    // Zoom out until the note is a sliver.
    ed.camera.units_per_px = 1e6;
    let rects = canvas::note_rects(&ed);
    assert!(rects.is_empty() || rects[0].blob.is_none());
}

#[test]
fn colour_reports_how_far_out_of_tune_the_note_is() {
    // Dead on, and badly flat.
    let in_tune = canvas::note_rects(&editor(Mode::PitchedAudio, 0.0))[0].fill;
    let out = canvas::note_rects(&editor(Mode::PitchedAudio, -0.35))[0].fill;

    assert_eq!(in_tune, theme::TUNE_RAMP[0]);
    assert_eq!(out, theme::TUNE_RAMP[4]);
    assert_ne!(in_tune, out);
}

#[test]
fn vibrato_does_not_make_a_good_note_flash_red() {
    // The note is centred on pitch but swings ±60 cents four times
    // across its length. Colouring from an instant would strobe; the
    // colour comes from the contour's centre, so it stays in tune.
    let rects = canvas::note_rects(&editor(Mode::PitchedAudio, 0.0));
    assert_eq!(
        rects[0].fill,
        theme::TUNE_RAMP[0],
        "a vibrato around the target is in tune, not out"
    );
}

#[test]
fn the_ribbon_is_dropped_where_the_blob_already_shows_it() {
    let audio = canvas::note_rects(&editor(Mode::PitchedAudio, 0.0));
    assert!(
        audio[0].ribbon.is_none(),
        "the blob is the envelope; a ribbon would draw it twice, and in \
         the row rather than on the pitch"
    );
    // MPE still gets the ribbon — it has no blob to carry the reading.
    let mpe = canvas::note_rects(&editor(Mode::Mpe, 0.0));
    assert!(mpe[0].ribbon.is_some());
}

#[test]
fn an_unedited_note_still_has_a_body() {
    // No authored Pressure at all: the analysis weight has to carry the
    // shape, or a freshly-analyzed take renders as hairlines.
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ * 4.0, 60);
    n.weight = 0.9;
    doc.push(n);
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.set_mode(Mode::PitchedAudio);

    let rects = canvas::note_rects(&ed);
    let pts = points(rects[0].blob.as_ref().unwrap());
    let n_pts = pts.len();
    let thickness = (pts[n_pts - 1].1 - pts[0].1).abs();
    assert!(thickness > 2.0, "got {thickness}px of body");
}

#[test]
fn three_notes_on_one_row_separate_by_how_sharp_they_are() {
    // The whole claim, measured: same row, different centre pitches, so
    // the only thing that can move the bodies apart is the audio's own
    // pitch. If these three overlap, the waveform is not being carried
    // to the pitch and the surface is lying about what was sung.
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 12.0);
    for (i, offset) in [-0.5_f64, 0.0, 0.5].iter().enumerate() {
        let start = PPQ * 4.0 * i as f64;
        let mut n = Note::new(NoteId(i as u64 + 1), start, start + PPQ * 3.0, 60);
        n.weight = 0.9;
        n.pitch.set(start, *offset);
        n.pitch.set(start + PPQ * 3.0, *offset);
        doc.push(n);
    }
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.set_mode(Mode::PitchedAudio);

    let rects = canvas::note_rects(&ed);
    let cy: Vec<f64> = rects.iter().map(|r| r.blob_center.unwrap()).collect();
    let half_semitone = 0.5 * ed.camera.vertical.px_per_row;

    // Flat sits below, sharp sits above, by half a semitone each.
    assert!(
        (cy[0] - cy[1] - half_semitone).abs() < half_semitone * 0.1,
        "50 cents flat draws half a semitone low: {} vs {}",
        cy[0],
        cy[1]
    );
    assert!(
        (cy[1] - cy[2] - half_semitone).abs() < half_semitone * 0.1,
        "50 cents sharp draws half a semitone high"
    );
    // And all three are on the same row, so the row is not what moved.
    assert!(rects.iter().all(|r| r.row == 60));
}

// ── backdrop, unvoiced spans and sibilant scope ──────────────────────

use expression_editor_core::Dimension;
use expression_editor_core::handles::{Handle, HandleDrag, Scope};

/// A note spanning a consonant: voiced, unvoiced, voiced.
fn sibilant_editor() -> Editor {
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, PPQ * 8.0);
    let mut n = Note::new(NoteId(1), 0.0, PPQ * 4.0, 60);
    n.weight = 0.8;
    for k in 0..64 {
        let f = k as f64 / 63.0;
        n.pitch.set(n.start + (n.end - n.start) * f, 0.0);
    }
    doc.push(n);
    doc.unvoiced = vec![(PPQ * 1.5, PPQ * 2.5)];
    doc.peaks = (0..500).map(|k| ((k % 17) as f32) / 17.0).collect();
    let mut ed = Editor::new(doc, Viewport::new(900.0, 500.0));
    ed.set_mode(Mode::PitchedAudio);
    ed
}

#[test]
fn the_take_waveform_is_drawn_only_where_there_is_audio() {
    assert!(canvas::take_waveform(&sibilant_editor()).is_some());

    // No peaks: nothing to draw.
    let mut ed = sibilant_editor();
    ed.doc.peaks.clear();
    assert!(canvas::take_waveform(&ed).is_none());

    // MIDI: there is no take.
    let mut ed = sibilant_editor();
    ed.set_mode(Mode::Midi);
    assert!(canvas::take_waveform(&ed).is_none());
}

#[test]
fn the_pitch_track_stops_where_there_was_no_pitch() {
    let ed = sibilant_editor();
    let paths: Vec<_> = canvas::curve_paths(&ed)
        .into_iter()
        .filter(|p| p.dimension == Dimension::Pitch)
        .collect();
    assert!(
        paths.len() >= 2,
        "one note either side of the consonant, so the track is in two \
         pieces rather than one line drawn straight through it"
    );

    // Without the gap it is a single run.
    let mut whole = sibilant_editor();
    whole.doc.unvoiced.clear();
    let joined: Vec<_> = canvas::curve_paths(&whole)
        .into_iter()
        .filter(|p| p.dimension == Dimension::Pitch)
        .collect();
    assert_eq!(joined.len(), 1);
}

#[test]
fn sibilant_bands_appear_only_while_the_scope_is_armed() {
    let mut ed = sibilant_editor();
    assert!(canvas::sibilant_bands(&ed).is_empty());
    ed.sibilant_scope = true;
    assert_eq!(canvas::sibilant_bands(&ed).len(), 1);

    // Never in a mode with no sibilants to shade.
    ed.set_mode(Mode::Midi);
    assert!(canvas::sibilant_bands(&ed).is_empty());
}

#[test]
fn the_sibilant_scope_rides_the_consonant_and_leaves_the_singing() {
    let mut ed = sibilant_editor();
    let note = ed.doc.note(NoteId(1)).unwrap().clone();
    let before_voiced = note
        .pressure
        .sample(PPQ * 0.5, Dimension::Pressure.default_value());

    let mut d = HandleDrag::begin_with(Handle::Amplitude, &note, Scope::Note, 250.0, true);
    ed.begin_gesture();
    ed.drag_handle(&mut d, 150.0, false);

    let n = ed.doc.note(NoteId(1)).unwrap();
    let in_consonant = n
        .pressure
        .sample(PPQ * 2.0, Dimension::Pressure.default_value());
    let in_singing = n
        .pressure
        .sample(PPQ * 0.5, Dimension::Pressure.default_value());

    assert!(
        in_consonant > before_voiced + 0.05,
        "the consonant was ridden: {before_voiced} -> {in_consonant}"
    );
    assert!(
        (in_singing - before_voiced).abs() < 0.05,
        "and the singing either side of it did not move: {in_singing}"
    );
}

#[test]
fn the_whole_note_scope_still_moves_everything() {
    let mut ed = sibilant_editor();
    let note = ed.doc.note(NoteId(1)).unwrap().clone();
    let mut d = HandleDrag::begin_with(Handle::Amplitude, &note, Scope::Note, 250.0, false);
    ed.begin_gesture();
    ed.drag_handle(&mut d, 150.0, false);

    let n = ed.doc.note(NoteId(1)).unwrap();
    let a = n
        .pressure
        .sample(PPQ * 0.5, Dimension::Pressure.default_value());
    let b = n
        .pressure
        .sample(PPQ * 2.0, Dimension::Pressure.default_value());
    assert!(
        (a - b).abs() < 1e-6,
        "one level across the note: {a} vs {b}"
    );
}

#[test]
fn a_sibilant_drag_does_not_compound_across_moves() {
    let mut ed = sibilant_editor();
    let note = ed.doc.note(NoteId(1)).unwrap().clone();
    let mut d = HandleDrag::begin_with(Handle::Amplitude, &note, Scope::Note, 250.0, true);
    ed.begin_gesture();

    // Out and back: each frame restores the captured dimension first, so the
    // consonant must land exactly where it started.
    let before = ed
        .doc
        .note(NoteId(1))
        .unwrap()
        .pressure
        .sample(PPQ * 2.0, Dimension::Pressure.default_value());
    for step in 1..=8 {
        ed.drag_handle(&mut d, 250.0 - step as f64 * 10.0, false);
    }
    ed.drag_handle(&mut d, 250.0, false);

    let after = ed
        .doc
        .note(NoteId(1))
        .unwrap()
        .pressure
        .sample(PPQ * 2.0, Dimension::Pressure.default_value());
    assert!((after - before).abs() < 1e-6, "{before} -> {after}");
}
